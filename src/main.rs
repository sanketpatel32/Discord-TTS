use vc_reader::{config, discord, session, speech, storage, telemetry};

use poise::serenity_prelude as serenity;
use songbird::SerenityInit;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{error, info};

use crate::config::Config;
use crate::discord::events::DiscordEventHandler;
use crate::session::actor::GuildActor;
use crate::speech::openai_realtime::OpenAIRealtimeProvider;
use crate::speech::provider::SpeechProvider;
use crate::storage::Storage;
use crate::telemetry::init_telemetry;

use vc_reader::{AppData, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    init_telemetry();

    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--check-config") {
        match Config::from_env() {
            Ok(cfg) => {
                info!("Configuration is valid: {:?}", cfg);
                return Ok(());
            }
            Err(e) => {
                error!("Configuration validation error: {e}");
                std::process::exit(1);
            }
        }
    }

    info!("Starting Discord VC Reader (Single-service, max 4 guilds)");

    let config = match Config::from_env() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            error!("Failed to load configuration: {e}");
            std::process::exit(1);
        }
    };

    // Ensure single instance lock file on host
    if let Some(parent) = config.database_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let lock_path = config
        .database_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("vc-reader.lock");
    let _lock_file = File::create(&lock_path)
        .map_err(|e| format!("Failed to create single-instance lock file at {lock_path:?}: {e}"))?;

    let storage = Storage::open(&config.database_path, config.clone())
        .map_err(|e| format!("Database initialization failed: {e}"))?;

    let songbird = songbird::Songbird::serenity();
    let generation_semaphore = Arc::new(Semaphore::new(config.max_concurrent_generations));

    let provider: Arc<dyn SpeechProvider> = Arc::new(OpenAIRealtimeProvider::new(config.clone()));

    let mut guild_actors = HashMap::new();
    for &gid in &config.allowed_guild_ids {
        info!("Spawning guild actor for allowlisted guild {gid}");
        let actor_tx = GuildActor::spawn(
            gid,
            config.clone(),
            storage.clone(),
            songbird.clone(),
            provider.clone(),
            generation_semaphore.clone(),
        );
        guild_actors.insert(gid, actor_tx);
    }
    let guild_actors = Arc::new(guild_actors);

    let app_data = AppData {
        config: config.clone(),
        storage,
        guild_actors: guild_actors.clone(),
    };

    let allowed_guild_ids = config.allowed_guild_ids.clone();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![discord::reader()],
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                // Register slash commands only in allowlisted guilds (Section 6.2)
                for &gid in &allowed_guild_ids {
                    let guild_id = serenity::GuildId::new(gid);
                    info!("Registering /reader slash commands in allowlisted guild {gid}");
                    poise::builtins::register_in_guild(
                        ctx,
                        &framework.options().commands,
                        guild_id,
                    )
                    .await?;
                }
                Ok(app_data)
            })
        })
        .build();

    // Required Gateway Intents (Section 7)
    let intents = serenity::GatewayIntents::GUILDS
        | serenity::GatewayIntents::GUILD_VOICE_STATES
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let event_handler = DiscordEventHandler {
        guild_actors: guild_actors.clone(),
    };

    let mut client = serenity::ClientBuilder::new(&config.discord_token, intents)
        .register_songbird_with(songbird)
        .framework(framework)
        .event_handler(event_handler)
        .await
        .map_err(|e| format!("Failed to create Discord client: {e}"))?;

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        info!("Shutdown signal received. Shutting down gracefully...");
        shard_manager.shutdown_all().await;
    });

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }

    info!("VC Reader stopped cleanly.");
    Ok(())
}
