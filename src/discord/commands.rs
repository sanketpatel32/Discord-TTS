use poise::serenity_prelude as serenity;
use tokio::sync::oneshot;

use crate::discord::permissions::{get_user_voice_channel, is_operator};
use crate::session::actor::{ActorCommand, SessionState};
use crate::{Context, Error};

#[poise::command(
    slash_command,
    guild_only,
    subcommands(
        "join", "leave", "pause", "resume", "skip", "clear", "status", "voice", "volume", "help"
    )
)]
pub async fn reader(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Join caller's current voice channel and enable reading its text chat.
#[poise::command(slash_command, guild_only)]
pub async fn join(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = match ctx.guild_id() {
        Some(g) => g.get(),
        None => {
            ctx.say("This command can only be used in a server.")
                .await?;
            return Ok(());
        }
    };

    let user_vc = match get_user_voice_channel(&ctx) {
        Some(vc) => vc,
        None => {
            ctx.say("You must be connected to a voice channel to use this command.")
                .await?;
            return Ok(());
        }
    };

    // Verify ordinary voice channel
    let is_voice = if let Some(guild) = ctx.cache().guild(guild_id) {
        guild
            .channels
            .get(&user_vc)
            .map(|c| c.kind == serenity::ChannelType::Voice)
            .unwrap_or(true)
    } else {
        true
    };

    if !is_voice {
        ctx.say("AI voice reading can only join ordinary voice channels.")
            .await?;
        return Ok(());
    }

    let actor_tx = match ctx.data().guild_actors.get(&guild_id) {
        Some(tx) => tx.clone(),
        None => {
            ctx.say("Guild is not configured for voice reading.")
                .await?;
            return Ok(());
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    actor_tx
        .send(ActorCommand::Join {
            voice_channel_id: user_vc.get(),
            invoker_id: ctx.author().id.get(),
            reply_tx,
        })
        .await
        .map_err(|e| format!("Actor communication failed: {e}"))?;

    match reply_rx.await {
        Ok(Ok(msg)) => {
            ctx.say(&msg).await?;

            // Post disclosure notice in the VC's text chat (Section 6.1)
            let disclosure = "AI voice reading is enabled in this voice channel. New text here is sent to OpenAI to generate speech. Use /reader pause or /reader leave to stop.";
            let builder = serenity::CreateMessage::new()
                .content(disclosure)
                .allowed_mentions(
                    serenity::CreateAllowedMentions::new()
                        .empty_users()
                        .empty_roles(),
                );

            let _ = user_vc.send_message(ctx.http(), builder).await;
        }
        Ok(Err(err)) => {
            ctx.say(format!("Failed to join voice channel: {err}"))
                .await?;
        }
        Err(_) => {
            ctx.say("Voice actor failed to respond.").await?;
        }
    }

    Ok(())
}

/// Disconnect the bot from the voice channel and stop reading.
#[poise::command(slash_command, guild_only)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap().get();
    let actor_tx = match ctx.data().guild_actors.get(&guild_id) {
        Some(tx) => tx.clone(),
        None => {
            ctx.say("Guild is not active.").await?;
            return Ok(());
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    actor_tx
        .send(ActorCommand::Leave {
            invoker_id: ctx.author().id.get(),
            reply_tx,
        })
        .await
        .map_err(|e| format!("Actor communication failed: {e}"))?;

    match reply_rx.await {
        Ok(Ok(msg)) => {
            ctx.say(msg).await?;
        }
        Ok(Err(err)) => {
            ctx.say(err).await?;
        }
        Err(_) => {
            ctx.say("Actor failed to respond.").await?;
        }
    }

    Ok(())
}

/// Pause voice reading. Messages will not be read while paused.
#[poise::command(slash_command, guild_only)]
pub async fn pause(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap().get();
    let actor_tx = match ctx.data().guild_actors.get(&guild_id) {
        Some(tx) => tx.clone(),
        None => {
            ctx.say("Guild is not active.").await?;
            return Ok(());
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    actor_tx
        .send(ActorCommand::Pause {
            invoker_id: ctx.author().id.get(),
            reply_tx,
        })
        .await
        .map_err(|e| format!("Actor communication failed: {e}"))?;

    match reply_rx.await {
        Ok(Ok(msg)) => {
            ctx.say(msg).await?;
        }
        Ok(Err(err)) => {
            ctx.say(err).await?;
        }
        Err(_) => {
            ctx.say("Actor failed to respond.").await?;
        }
    }

    Ok(())
}

/// Resume voice reading for fresh messages.
#[poise::command(slash_command, guild_only)]
pub async fn resume(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap().get();
    let actor_tx = match ctx.data().guild_actors.get(&guild_id) {
        Some(tx) => tx.clone(),
        None => {
            ctx.say("Guild is not active.").await?;
            return Ok(());
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    actor_tx
        .send(ActorCommand::Resume {
            invoker_id: ctx.author().id.get(),
            reply_tx,
        })
        .await
        .map_err(|e| format!("Actor communication failed: {e}"))?;

    match reply_rx.await {
        Ok(Ok(msg)) => {
            ctx.say(msg).await?;
        }
        Ok(Err(err)) => {
            ctx.say(err).await?;
        }
        Err(_) => {
            ctx.say("Actor failed to respond.").await?;
        }
    }

    Ok(())
}

/// Skip the currently generating or playing message.
#[poise::command(slash_command, guild_only)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap().get();
    let actor_tx = match ctx.data().guild_actors.get(&guild_id) {
        Some(tx) => tx.clone(),
        None => {
            ctx.say("Guild is not active.").await?;
            return Ok(());
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    actor_tx
        .send(ActorCommand::Skip {
            invoker_id: ctx.author().id.get(),
            reply_tx,
        })
        .await
        .map_err(|e| format!("Actor communication failed: {e}"))?;

    match reply_rx.await {
        Ok(Ok(msg)) => {
            ctx.say(msg).await?;
        }
        Ok(Err(err)) => {
            ctx.say(err).await?;
        }
        Err(_) => {
            ctx.say("Actor failed to respond.").await?;
        }
    }

    Ok(())
}

/// Clear all pending messages from the queue without interrupting current speech.
#[poise::command(slash_command, guild_only)]
pub async fn clear(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap().get();
    let actor_tx = match ctx.data().guild_actors.get(&guild_id) {
        Some(tx) => tx.clone(),
        None => {
            ctx.say("Guild is not active.").await?;
            return Ok(());
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    actor_tx
        .send(ActorCommand::Clear {
            invoker_id: ctx.author().id.get(),
            reply_tx,
        })
        .await
        .map_err(|e| format!("Actor communication failed: {e}"))?;

    match reply_rx.await {
        Ok(Ok(msg)) => {
            ctx.say(msg).await?;
        }
        Ok(Err(err)) => {
            ctx.say(err).await?;
        }
        Err(_) => {
            ctx.say("Actor failed to respond.").await?;
        }
    }

    Ok(())
}

/// Show status, joined voice channel, queue length, current voice, and monthly usage.
#[poise::command(slash_command, guild_only)]
pub async fn status(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap().get();
    let actor_tx = match ctx.data().guild_actors.get(&guild_id) {
        Some(tx) => tx.clone(),
        None => {
            ctx.say("Guild is not active.").await?;
            return Ok(());
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    actor_tx
        .send(ActorCommand::GetStatus { reply_tx })
        .await
        .map_err(|e| format!("Actor communication failed: {e}"))?;

    let report = reply_rx.await.map_err(|_| "Actor failed to respond")?;
    let is_op = is_operator(&ctx, &ctx.data().config).await;

    let state_str = match report.state {
        SessionState::Disconnected => "Disconnected",
        SessionState::Connecting => "Connecting",
        SessionState::Connected => "Connected & Reading",
        SessionState::Paused => "Paused",
    };

    let channel_info = match report.active_voice_channel_id {
        Some(vc_id) => format!("<#{vc_id}>"),
        None => "None".to_string(),
    };

    let played_mins = report.monthly_usage.played_ms / 60000;
    let spent_dollars = report.monthly_usage.estimated_microdollars as f64 / 1_000_000.0;

    let mut response = format!(
        "**VC Reader Status**\n\
         • **State:** {state_str}\n\
         • **Voice Channel:** {channel_info}\n\
         • **Audience:** {} human participants\n\
         • **Queue:** {} pending messages\n\
         • **Voice:** {}\n\
         • **Volume:** {}%\n\
         • **This Guild's Usage ({})**: {} requests (~{} mins played, ~${:.3})\n\
         • **Service Available:** {}",
        report.human_voice_member_count,
        report.queue_length,
        report.selected_voice,
        report.volume,
        report.monthly_usage.period,
        report.monthly_usage.request_count,
        played_mins,
        spent_dollars,
        if report.budget_available {
            "Yes"
        } else {
            "Budget Exhausted"
        }
    );

    if is_op {
        if let Ok(global_budget) = ctx.data().storage.get_global_budget_status() {
            response.push_str(&format!(
                "\n\n**Operator Global Budget**\n\
                 • Period: {}\n\
                 • Spent: ${:.2} / ${:.2}\n\
                 • Reserved: ${:.3}\n\
                 • Remaining: ${:.2}",
                global_budget.period,
                global_budget.spent_microdollars as f64 / 1_000_000.0,
                global_budget.budget_allowance_microdollars as f64 / 1_000_000.0,
                global_budget.reserved_microdollars as f64 / 1_000_000.0,
                global_budget.remaining_microdollars as f64 / 1_000_000.0,
            ));
        }
    }

    ctx.say(response).await?;
    Ok(())
}

/// Set the OpenAI TTS voice (operator only).
#[poise::command(slash_command, guild_only)]
pub async fn voice(
    ctx: Context<'_>,
    #[description = "Voice name (e.g. marin or cedar)"] voice_name: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    if !is_operator(&ctx, &ctx.data().config).await {
        ctx.say("Only server operators or bot owners can change the voice setting.")
            .await?;
        return Ok(());
    }

    let guild_id = ctx.guild_id().unwrap().get();
    let actor_tx = match ctx.data().guild_actors.get(&guild_id) {
        Some(tx) => tx.clone(),
        None => {
            ctx.say("Guild is not active.").await?;
            return Ok(());
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    actor_tx
        .send(ActorCommand::SetVoice {
            voice: voice_name,
            invoker_id: ctx.author().id.get(),
            reply_tx,
        })
        .await
        .map_err(|e| format!("Actor communication failed: {e}"))?;

    match reply_rx.await {
        Ok(Ok(msg)) => {
            ctx.say(msg).await?;
        }
        Ok(Err(err)) => {
            ctx.say(err).await?;
        }
        Err(_) => {
            ctx.say("Actor failed to respond.").await?;
        }
    }

    Ok(())
}

/// Set playback volume 0-100% (operator only).
#[poise::command(slash_command, guild_only)]
pub async fn volume(
    ctx: Context<'_>,
    #[description = "Volume level 0-100"] percent: u8,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    if !is_operator(&ctx, &ctx.data().config).await {
        ctx.say("Only server operators or bot owners can change the volume setting.")
            .await?;
        return Ok(());
    }

    let guild_id = ctx.guild_id().unwrap().get();
    let actor_tx = match ctx.data().guild_actors.get(&guild_id) {
        Some(tx) => tx.clone(),
        None => {
            ctx.say("Guild is not active.").await?;
            return Ok(());
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    actor_tx
        .send(ActorCommand::SetVolume {
            volume: percent,
            invoker_id: ctx.author().id.get(),
            reply_tx,
        })
        .await
        .map_err(|e| format!("Actor communication failed: {e}"))?;

    match reply_rx.await {
        Ok(Ok(msg)) => {
            ctx.say(msg).await?;
        }
        Ok(Err(err)) => {
            ctx.say(err).await?;
        }
        Err(_) => {
            ctx.say("Actor failed to respond.").await?;
        }
    }

    Ok(())
}

/// Explain how VC Reader works and available controls.
#[poise::command(slash_command, guild_only)]
pub async fn help(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let help_text = "**VC Reader — Help & Usage Guide**\n\n\
        **How it works:**\n\
        1. Join an ordinary voice channel (e.g. `Gaming`).\n\
        2. Run `/reader join`.\n\
        3. Type messages in **that voice channel's built-in text chat**.\n\
        4. The bot reads your messages aloud in that same voice channel!\n\n\
        **Channel Scope:**\n\
        • The bot ONLY reads messages typed in the joined voice channel's text chat.\n\
        • Messages in other channels (`#general`, other VCs, threads) are completely ignored.\n\n\
        **Languages Supported:**\n\
        • English, Hindi (Devanagari script), Romanized Hindi, and Hinglish.\n\n\
        **Commands:**\n\
        • `/reader join` — Join your voice channel and start reading.\n\
        • `/reader leave` — Stop reading and disconnect.\n\
        • `/reader pause` — Pause reading temporarily.\n\
        • `/reader resume` — Resume reading fresh messages.\n\
        • `/reader skip` — Skip the current spoken message.\n\
        • `/reader clear` — Discard pending messages in queue.\n\
        • `/reader status` — View status, voice, and monthly usage.\n\
        • `/reader voice <name>` — Set voice (e.g. `marin`, `cedar`).\n\
        • `/reader volume <0-100>` — Adjust playback volume.\n\
        • `/reader help` — Display this guide.";

    ctx.say(help_text).await?;
    Ok(())
}
