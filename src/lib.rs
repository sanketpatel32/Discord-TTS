#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::chunks_exact_to_as_chunks)]

pub mod audio;
pub mod config;
pub mod discord;
pub mod session;
pub mod speech;
pub mod storage;
pub mod telemetry;
pub mod text;

pub use config::Config;
pub use storage::Storage;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, AppData, Error>;

#[derive(Clone)]
pub struct AppData {
    pub config: Arc<Config>,
    pub storage: Storage,
    pub guild_actors: Arc<HashMap<u64, mpsc::Sender<ActorCommand>>>,
}

use session::actor::ActorCommand;
