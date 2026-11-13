use chrono::{DateTime, Utc};
use poise::serenity_prelude as serenity;
use songbird::Songbird;
use songbird::tracks::TrackHandle;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, mpsc, oneshot, watch};
use tracing::{debug, error, info, warn};

use crate::audio::pcm::pcm16_to_f32_le_bytes;
use crate::audio::voice::VoiceSessionManager;
use crate::config::Config;
use crate::session::jobs::{GuildAdmissionController, GuildJobQueue, SpeechJob};
use crate::speech::provider::{ProviderError, SpeechProvider};
use crate::storage::{GuildSettingsRecord, MonthlyUsageSummary, Storage};
use crate::text::normalize::{NormalizationContext, normalize_message_text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Disconnected,
    Connecting,
    Connected,
    Paused,
}

#[derive(Debug, Clone)]
pub struct GuildStatusReport {
    pub guild_id: u64,
    pub state: SessionState,
    pub active_voice_channel_id: Option<u64>,
    pub human_voice_member_count: usize,
    pub queue_length: usize,
    pub selected_voice: String,
    pub volume: u8,
    pub monthly_usage: MonthlyUsageSummary,
    pub budget_available: bool,
}

pub enum ActorCommand {
    Join {
        voice_channel_id: u64,
        invoker_id: u64,
        reply_tx: oneshot::Sender<Result<String, String>>,
    },
    Leave {
        invoker_id: u64,
        reply_tx: oneshot::Sender<Result<String, String>>,
    },
    Pause {
        invoker_id: u64,
        reply_tx: oneshot::Sender<Result<String, String>>,
    },
    Resume {
        invoker_id: u64,
        reply_tx: oneshot::Sender<Result<String, String>>,
    },
    Skip {
        invoker_id: u64,
        reply_tx: oneshot::Sender<Result<String, String>>,
    },
    Clear {
        invoker_id: u64,
        reply_tx: oneshot::Sender<Result<String, String>>,
    },
    SetVoice {
        voice: String,
        invoker_id: u64,
        reply_tx: oneshot::Sender<Result<String, String>>,
    },
    SetVolume {
        volume: u8,
        invoker_id: u64,
        reply_tx: oneshot::Sender<Result<String, String>>,
    },
    GetStatus {
        reply_tx: oneshot::Sender<GuildStatusReport>,
    },
    IncomingMessage {
        message_id: u64,
        channel_id: u64,
        author_id: u64,
        is_bot: bool,
        raw_content: String,
        created_at: DateTime<Utc>,
        user_mentions: HashMap<u64, String>,
        role_mentions: HashMap<u64, String>,
        channel_mentions: HashMap<u64, String>,
    },
    MessageDeleted {
        message_id: u64,
        channel_id: u64,
    },
    VoiceStateUpdate {
        user_id: u64,
        old_channel_id: Option<u64>,
        new_channel_id: Option<u64>,
    },
}

pub struct GuildActor {
    pub guild_id: u64,
    config: Arc<Config>,
    storage: Storage,
    songbird: Arc<Songbird>,
    provider: Arc<dyn SpeechProvider>,
    generation_semaphore: Arc<Semaphore>,

    state: SessionState,
    active_voice_channel_id: Option<u64>,
    session_epoch: u64,
    activation_cutoff_message_id: u64,
    queue: GuildJobQueue,
    admission: GuildAdmissionController,

    current_job_id: Option<String>,
    current_cancel_tx: Option<watch::Sender<bool>>,
    current_track: Option<TrackHandle>,

    human_voice_member_count: usize,
    selected_voice: String,
    volume: u8,

    cmd_rx: mpsc::Receiver<ActorCommand>,
    empty_channel_deadline: Option<Instant>,
}

impl GuildActor {
    pub fn spawn(
        guild_id: u64,
        config: Arc<Config>,
        storage: Storage,
        songbird: Arc<Songbird>,
        provider: Arc<dyn SpeechProvider>,
        generation_semaphore: Arc<Semaphore>,
    ) -> mpsc::Sender<ActorCommand> {
        let (cmd_tx, cmd_rx) = mpsc::channel(128);

        let initial_settings =
            storage
                .get_guild_settings(guild_id)
                .unwrap_or(GuildSettingsRecord {
                    guild_id,
                    selected_voice: config.default_voice.clone(),
                    volume: 100,
                });

        let mut actor = Self {
            guild_id,
            config: config.clone(),
            storage,
            songbird,
            provider,
            generation_semaphore,
            state: SessionState::Disconnected,
            active_voice_channel_id: None,
            session_epoch: 0,
            activation_cutoff_message_id: 0,
            queue: GuildJobQueue::new(config.max_pending_per_guild),
            admission: GuildAdmissionController::new(),
            current_job_id: None,
            current_cancel_tx: None,
            current_track: None,
            human_voice_member_count: 0,
            selected_voice: initial_settings.selected_voice,
            volume: initial_settings.volume,
            cmd_rx,
            empty_channel_deadline: None,
        };

        tokio::spawn(async move {
            actor.run().await;
        });

        cmd_tx
    }

    async fn run(&mut self) {
        let mut interval = tokio::time::interval(Duration::from_millis(100));

        loop {
            tokio::select! {
                cmd = self.cmd_rx.recv() => {
                    match cmd {
                        Some(c) => self.handle_command(c).await,
                        None => break, // Channel closed
                    }
                }
                _ = interval.tick() => {
                    self.check_empty_channel_timeout().await;
                    self.process_queue_tick().await;
                }
            }
        }
    }

    async fn handle_command(&mut self, cmd: ActorCommand) {
        match cmd {
            ActorCommand::Join {
                voice_channel_id,
                invoker_id: _,
                reply_tx,
            } => {
                let res = self.handle_join(voice_channel_id).await;
                let _ = reply_tx.send(res);
            }
            ActorCommand::Leave {
                invoker_id: _,
                reply_tx,
            } => {
                let res = self.handle_leave().await;
                let _ = reply_tx.send(res);
            }
            ActorCommand::Pause {
                invoker_id: _,
                reply_tx,
            } => {
                let res = self.handle_pause().await;
                let _ = reply_tx.send(res);
            }
            ActorCommand::Resume {
                invoker_id: _,
                reply_tx,
            } => {
                let res = self.handle_resume().await;
                let _ = reply_tx.send(res);
            }
            ActorCommand::Skip {
                invoker_id: _,
                reply_tx,
            } => {
                let res = self.handle_skip().await;
                let _ = reply_tx.send(res);
            }
            ActorCommand::Clear {
                invoker_id: _,
                reply_tx,
            } => {
                let count = self.queue.clear();
                let _ = reply_tx.send(Ok(format!("Cleared {count} pending messages from queue.")));
            }
            ActorCommand::SetVoice {
                voice,
                invoker_id: _,
                reply_tx,
            } => {
                let res = self.handle_set_voice(voice).await;
                let _ = reply_tx.send(res);
            }
            ActorCommand::SetVolume {
                volume,
                invoker_id: _,
                reply_tx,
            } => {
                let res = self.handle_set_volume(volume).await;
                let _ = reply_tx.send(res);
            }
            ActorCommand::GetStatus { reply_tx } => {
                let summary = self
                    .storage
                    .get_guild_usage_summary(self.guild_id)
                    .unwrap_or_default();
                let budget = self.storage.get_global_budget_status().unwrap_or_default();

                let report = GuildStatusReport {
                    guild_id: self.guild_id,
                    state: self.state,
                    active_voice_channel_id: self.active_voice_channel_id,
                    human_voice_member_count: self.human_voice_member_count,
                    queue_length: self.queue.len(),
                    selected_voice: self.selected_voice.clone(),
                    volume: self.volume,
                    monthly_usage: summary,
                    budget_available: budget.is_available,
                };
                let _ = reply_tx.send(report);
            }
            ActorCommand::IncomingMessage {
                message_id,
                channel_id,
                author_id,
                is_bot,
                raw_content,
                created_at,
                user_mentions,
                role_mentions,
                channel_mentions,
            } => {
                self.handle_incoming_message(
                    message_id,
                    channel_id,
                    author_id,
                    is_bot,
                    raw_content,
                    created_at,
                    user_mentions,
                    role_mentions,
                    channel_mentions,
                );
            }
            ActorCommand::MessageDeleted {
                message_id,
                channel_id,
            } => {
                self.handle_message_deleted(message_id, channel_id);
            }
            ActorCommand::VoiceStateUpdate {
                user_id,
                old_channel_id,
                new_channel_id,
            } => {
                self.handle_voice_state_update(user_id, old_channel_id, new_channel_id)
                    .await;
            }
        }
    }

    fn invalidate_session(&mut self) {
        self.session_epoch += 1;

        if let Some(cancel_tx) = self.current_cancel_tx.take() {
            let _ = cancel_tx.send(true);
        }

        if let Some(track) = self.current_track.take() {
            let _ = track.stop();
        }

        self.current_job_id = None;
        self.queue.clear();
    }

    async fn handle_join(&mut self, voice_channel_id: u64) -> Result<String, String> {
        // Idempotent check
        if self.state == SessionState::Connected
            && self.active_voice_channel_id == Some(voice_channel_id)
        {
            return Ok("Already joined and reading this voice channel.".to_string());
        }

        self.invalidate_session();

        let guild_id = serenity::GuildId::new(self.guild_id);
        let channel_id = serenity::ChannelId::new(voice_channel_id);

        let join_res = self.songbird.join(guild_id, channel_id).await;
        match join_res {
            Ok(call_lock) => {
                // Self-deafen the bot as required by Section 7
                let mut call = call_lock.lock().await;
                let _ = call.deafen(true).await;

                self.state = SessionState::Connected;
                self.active_voice_channel_id = Some(voice_channel_id);

                // Set cutoff snowflake to current time
                let now_millis = Utc::now().timestamp_millis() as u64;
                let snowflake = (now_millis.saturating_sub(1420070400000)) << 22;
                self.activation_cutoff_message_id = snowflake;
                self.empty_channel_deadline = None;

                info!(
                    guild_id = self.guild_id,
                    vc_id = voice_channel_id,
                    epoch = self.session_epoch,
                    "Joined voice channel and activated reading"
                );

                Ok("Connected to voice channel. AI voice reading is enabled in this voice channel's text chat.".to_string())
            }
            Err(e) => {
                self.state = SessionState::Disconnected;
                self.active_voice_channel_id = None;
                Err(format!("Failed to join voice channel: {e}"))
            }
        }
    }

    async fn handle_leave(&mut self) -> Result<String, String> {
        self.invalidate_session();
        self.state = SessionState::Disconnected;
        self.active_voice_channel_id = None;
        self.empty_channel_deadline = None;

        let guild_id = serenity::GuildId::new(self.guild_id);
        let _ = self.songbird.remove(guild_id).await;

        info!(
            guild_id = self.guild_id,
            "Left voice channel and disconnected"
        );
        Ok("Disconnected and reading session stopped.".to_string())
    }

    async fn handle_pause(&mut self) -> Result<String, String> {
        if self.state != SessionState::Connected {
            return Err("Bot is not currently active in a voice channel.".to_string());
        }

        self.invalidate_session();
        self.state = SessionState::Paused;

        info!(guild_id = self.guild_id, "Paused voice reading session");
        Ok("Reading paused. Incoming messages will not be read.".to_string())
    }

    async fn handle_resume(&mut self) -> Result<String, String> {
        if self.state != SessionState::Paused {
            return Err("Bot is not currently paused.".to_string());
        }

        self.session_epoch += 1;
        self.state = SessionState::Connected;

        // Establish a fresh cutoff so messages typed during pause are not backlogged
        let now_millis = Utc::now().timestamp_millis() as u64;
        let snowflake = (now_millis.saturating_sub(1420070400000)) << 22;
        self.activation_cutoff_message_id = snowflake;

        info!(
            guild_id = self.guild_id,
            epoch = self.session_epoch,
            "Resumed voice reading session"
        );
        Ok("Reading resumed for new messages.".to_string())
    }

    async fn handle_skip(&mut self) -> Result<String, String> {
        if let Some(cancel_tx) = self.current_cancel_tx.take() {
            let _ = cancel_tx.send(true);
        }
        if let Some(track) = self.current_track.take() {
            let _ = track.stop();
        }
        self.current_job_id = None;

        Ok("Skipped current message.".to_string())
    }

    async fn handle_set_voice(&mut self, voice: String) -> Result<String, String> {
        if !self.config.allowed_voices.contains(&voice) {
            return Err(format!(
                "Voice '{voice}' is not allowed. Choose from: {:?}",
                self.config.allowed_voices
            ));
        }

        self.invalidate_session();
        self.selected_voice = voice.clone();
        let _ = self.storage.set_guild_voice(self.guild_id, &voice);

        Ok(format!("Voice set to '{voice}'."))
    }

    async fn handle_set_volume(&mut self, volume: u8) -> Result<String, String> {
        if volume > 100 {
            return Err("Volume must be an integer between 0 and 100.".to_string());
        }

        self.volume = volume;
        let _ = self.storage.set_guild_volume(self.guild_id, volume);

        if let Some(ref track) = self.current_track {
            let _ = track.set_volume(volume as f32 / 100.0);
        }

        Ok(format!("Volume set to {volume}%."))
    }

    fn handle_incoming_message(
        &mut self,
        message_id: u64,
        channel_id: u64,
        author_id: u64,
        is_bot: bool,
        raw_content: String,
        created_at: DateTime<Utc>,
        user_mentions: HashMap<u64, String>,
        role_mentions: HashMap<u64, String>,
        channel_mentions: HashMap<u64, String>,
    ) {
        // Section 5.2 Admission Rules
        // Rule 2: Connected, unpaused session
        if self.state != SessionState::Connected {
            return;
        }

        // Rule 3: Channel ID must match active voice channel ID exactly!
        if Some(channel_id) != self.active_voice_channel_id {
            return;
        }

        // Rule 4: Author must be human
        if is_bot {
            return;
        }

        // Rule 6: Newer than cutoff and within age limit (60s)
        if message_id <= self.activation_cutoff_message_id {
            return;
        }
        if (Utc::now() - created_at).num_seconds() > self.config.max_job_age_seconds as i64 {
            return;
        }

        // Rule 7: Deduplication and deletion tombstone
        if self.admission.is_deleted(message_id) {
            return;
        }
        if self.admission.check_and_record_duplicate(message_id) {
            return;
        }

        // Rule 8: Normalization
        let ctx = NormalizationContext {
            user_mentions: &user_mentions,
            role_mentions: &role_mentions,
            channel_mentions: &channel_mentions,
            max_chars: self.config.max_text_chars,
            max_bytes: self.config.max_text_bytes,
        };
        let normalized = match normalize_message_text(&raw_content, &ctx) {
            Some(t) => t,
            None => return,
        };

        // Rule 9: User rate limits
        if !self.admission.check_user_rate_limit(author_id) {
            warn!(
                author_id,
                "User rate limit exceeded in guild {}", self.guild_id
            );
            return;
        }

        // Rule 10: Human audience present
        if self.human_voice_member_count == 0 {
            debug!("Discarding message because voice channel has no human participants");
            return;
        }

        let job = SpeechJob {
            job_id: format!("{}_{}", self.guild_id, message_id),
            guild_id: self.guild_id,
            voice_channel_id: channel_id,
            session_epoch: self.session_epoch,
            message_id,
            author_id,
            received_at_monotonic: Instant::now(),
            message_created_at: created_at,
            normalized_text: normalized,
            retry_count: 0,
        };

        if let Err(dropped) = self.queue.enqueue(job) {
            warn!(
                guild_id = self.guild_id,
                job_id = %dropped.job_id,
                "Queue full: dropped newest message"
            );
        } else {
            debug!(
                guild_id = self.guild_id,
                queue_len = self.queue.len(),
                "Enqueued eligible voice channel message"
            );
        }
    }

    fn handle_message_deleted(&mut self, message_id: u64, channel_id: u64) {
        if Some(channel_id) != self.active_voice_channel_id {
            return;
        }

        self.admission.record_deletion(message_id);
        self.queue.cancel_message(message_id);

        if let Some(ref cur_id) = self.current_job_id {
            if cur_id.ends_with(&format!("_{message_id}")) {
                if let Some(cancel_tx) = self.current_cancel_tx.take() {
                    let _ = cancel_tx.send(true);
                }
                if let Some(track) = self.current_track.take() {
                    let _ = track.stop();
                }
                self.current_job_id = None;
            }
        }
    }

    async fn handle_voice_state_update(
        &mut self,
        user_id: u64,
        old_channel_id: Option<u64>,
        new_channel_id: Option<u64>,
    ) {
        let active_vc = match self.active_voice_channel_id {
            Some(id) => id,
            None => return,
        };

        let bot_user_id = self.config.discord_application_id;

        // If the bot itself was moved or disconnected
        if user_id == bot_user_id {
            if new_channel_id.is_none() {
                // External disconnect
                warn!(guild_id = self.guild_id, "Bot was disconnected externally");
                let _ = self.handle_leave().await;
            } else if new_channel_id != Some(active_vc) {
                // Bot was moved to another VC: remain paused in destination, require /reader join
                warn!(
                    guild_id = self.guild_id,
                    "Bot was moved externally to another VC"
                );
                self.invalidate_session();
                self.state = SessionState::Paused;
                self.active_voice_channel_id = new_channel_id;
            }
            return;
        }

        // Participant count tracking
        if old_channel_id == Some(active_vc) && new_channel_id != Some(active_vc) {
            self.human_voice_member_count = self.human_voice_member_count.saturating_sub(1);
        } else if old_channel_id != Some(active_vc) && new_channel_id == Some(active_vc) {
            self.human_voice_member_count += 1;
        }

        if self.human_voice_member_count == 0 {
            // Stop generation/playback immediately and start empty grace timer
            if let Some(cancel_tx) = self.current_cancel_tx.take() {
                let _ = cancel_tx.send(true);
            }
            if let Some(track) = self.current_track.take() {
                let _ = track.stop();
            }
            self.current_job_id = None;
            self.queue.clear();

            if self.empty_channel_deadline.is_none() {
                self.empty_channel_deadline = Some(
                    Instant::now() + Duration::from_secs(self.config.empty_channel_timeout_seconds),
                );
                debug!(
                    guild_id = self.guild_id,
                    "VC became empty; started 60s timeout"
                );
            }
        } else {
            self.empty_channel_deadline = None;
        }
    }

    async fn check_empty_channel_timeout(&mut self) {
        if let Some(deadline) = self.empty_channel_deadline {
            if Instant::now() >= deadline && self.state == SessionState::Connected {
                info!(
                    guild_id = self.guild_id,
                    "Empty VC grace period expired; disconnecting"
                );
                let _ = self.handle_leave().await;
            }
        }
    }

    async fn process_queue_tick(&mut self) {
        if self.state != SessionState::Connected || self.current_job_id.is_some() {
            return;
        }

        let job = match self.queue.pop_next() {
            Some(j) => j,
            None => return,
        };

        if job.is_expired(self.config.max_job_age_seconds) {
            debug!(job_id = %job.job_id, "Dropping expired queued job");
            return;
        }

        // Section 5.3 Recheck tuple at irreversible boundaries
        let expected_epoch = self.session_epoch;
        let expected_vc = match self.active_voice_channel_id {
            Some(vc) => vc,
            None => return,
        };

        if job.session_epoch != expected_epoch || job.voice_channel_id != expected_vc {
            return;
        }

        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.current_job_id = Some(job.job_id.clone());
        self.current_cancel_tx = Some(cancel_tx);

        let provider = self.provider.clone();
        let storage = self.storage.clone();
        let semaphore = self.generation_semaphore.clone();
        let songbird = self.songbird.clone();
        let voice = self.selected_voice.clone();
        let volume = self.volume;
        let guild_id = self.guild_id;

        // Perform synthesis and playback in background task without holding actor lock
        tokio::spawn(async move {
            // Acquire generation permit (max 2 concurrent global)
            let _permit = match semaphore.acquire().await {
                Ok(p) => p,
                Err(_) => return,
            };

            // Budget reservation
            let attempt_id = format!("att_{}_{}", job.job_id, job.retry_count);
            if let Err(e) = storage.reserve_attempt_budget(
                &attempt_id,
                &job.job_id,
                guild_id,
                job.normalized_text.len(),
            ) {
                warn!(guild_id, error = %e, "Budget reservation denied");
                return;
            }

            // Synthesize
            let synth_res = provider.synthesize(&job, &voice, cancel_rx).await;

            // Release semaphore immediately after synthesis
            drop(_permit);

            match synth_res {
                Ok(res) => {
                    // Finalize usage
                    let _ = storage.finalize_attempt_usage(
                        &attempt_id,
                        guild_id,
                        res.duration_ms,
                        res.duration_ms,
                        res.input_tokens,
                        res.cached_tokens,
                        res.output_text_tokens,
                        res.output_audio_tokens,
                        "completed",
                    );

                    // Convert audio to f32
                    let f32_bytes = match pcm16_to_f32_le_bytes(&res.audio_pcm16) {
                        Ok(b) => b,
                        Err(e) => {
                            error!(guild_id, error = %e, "PCM conversion failed");
                            return;
                        }
                    };

                    // Playback via Songbird
                    let guild_serenity_id = serenity::GuildId::new(guild_id);
                    if let Some(call_lock) = songbird.get(guild_serenity_id) {
                        let mut call = call_lock.lock().await;
                        let input = VoiceSessionManager::create_track(f32_bytes);
                        let handle = call.play_input(input);
                        let _ = handle.set_volume(volume as f32 / 100.0);

                        // Wait for track duration
                        let dur = Duration::from_millis(res.duration_ms);
                        tokio::time::sleep(dur).await;
                    }
                }
                Err(e) => {
                    warn!(guild_id, job_id = %job.job_id, error = %e, "Synthesis failed");
                    let status = match e {
                        ProviderError::Canceled => "canceled",
                        ProviderError::Timeout => "uncertain",
                        _ => "failed",
                    };
                    let _ = storage.finalize_attempt_usage(
                        &attempt_id,
                        guild_id,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        status,
                    );
                }
            }
        });

        // Let loop advance current_job_id on next cycle
        self.current_job_id = None;
    }
}
