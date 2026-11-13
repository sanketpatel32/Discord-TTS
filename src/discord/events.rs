use poise::serenity_prelude as serenity;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

use crate::session::actor::ActorCommand;

pub struct DiscordEventHandler {
    pub guild_actors: Arc<HashMap<u64, mpsc::Sender<ActorCommand>>>,
}

#[serenity::async_trait]
impl serenity::EventHandler for DiscordEventHandler {
    async fn ready(&self, _: serenity::Context, ready: serenity::Ready) {
        info!(bot_name = %ready.user.name, bot_id = %ready.user.id, "Connected to Discord Gateway");
    }

    async fn message(&self, _: serenity::Context, msg: serenity::Message) {
        let guild_id = match msg.guild_id {
            Some(g) => g.get(),
            None => return, // Ignore DMs
        };

        // Section 5.2 Admission Check 1: Allowed guild
        let actor_tx = match self.guild_actors.get(&guild_id) {
            Some(tx) => tx,
            None => return, // Silently ignore non-allowlisted guilds before TTS work
        };

        let mut user_mentions = HashMap::new();
        for u in &msg.mentions {
            user_mentions.insert(u.id.get(), u.name.clone());
        }

        let mut role_mentions = HashMap::new();
        for r in &msg.mention_roles {
            role_mentions.insert(r.get(), "a role".to_string());
        }

        let mut channel_mentions = HashMap::new();
        for c in &msg.mention_channels {
            channel_mentions.insert(c.id.get(), c.name.clone());
        }

        let cmd = ActorCommand::IncomingMessage {
            message_id: msg.id.get(),
            channel_id: msg.channel_id.get(),
            author_id: msg.author.id.get(),
            is_bot: msg.author.bot,
            raw_content: msg.content.clone(),
            created_at: msg.timestamp.to_utc(),
            user_mentions,
            role_mentions,
            channel_mentions,
        };

        let _ = actor_tx.send(cmd).await;
    }

    async fn message_delete(
        &self,
        _: serenity::Context,
        channel_id: serenity::ChannelId,
        deleted_message_id: serenity::MessageId,
        guild_id: Option<serenity::GuildId>,
    ) {
        let gid = match guild_id {
            Some(g) => g.get(),
            None => return,
        };

        if let Some(actor_tx) = self.guild_actors.get(&gid) {
            let _ = actor_tx
                .send(ActorCommand::MessageDeleted {
                    message_id: deleted_message_id.get(),
                    channel_id: channel_id.get(),
                })
                .await;
        }
    }

    async fn voice_state_update(
        &self,
        _: serenity::Context,
        old: Option<serenity::VoiceState>,
        new: serenity::VoiceState,
    ) {
        let gid = match new
            .guild_id
            .or_else(|| old.as_ref().and_then(|o| o.guild_id))
        {
            Some(g) => g.get(),
            None => return,
        };

        if let Some(actor_tx) = self.guild_actors.get(&gid) {
            let user_id = new.user_id.get();
            let old_channel = old.and_then(|o| o.channel_id).map(|c| c.get());
            let new_channel = new.channel_id.map(|c| c.get());

            let _ = actor_tx
                .send(ActorCommand::VoiceStateUpdate {
                    user_id,
                    old_channel_id: old_channel,
                    new_channel_id: new_channel,
                })
                .await;
        }
    }
}
