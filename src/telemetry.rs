use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_telemetry() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,vc_reader=debug,songbird=info,serenity=warn"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().compact())
        .init();
}

#[derive(Default, Debug)]
pub struct Metrics {
    pub messages_accepted: std::sync::atomic::AtomicU64,
    pub messages_ignored: std::sync::atomic::AtomicU64,
    pub messages_duplicate: std::sync::atomic::AtomicU64,
    pub messages_truncated: std::sync::atomic::AtomicU64,
    pub messages_expired: std::sync::atomic::AtomicU64,
    pub messages_queue_full: std::sync::atomic::AtomicU64,
    pub messages_narration_rejected: std::sync::atomic::AtomicU64,
    pub messages_canceled: std::sync::atomic::AtomicU64,
    pub messages_provider_failed: std::sync::atomic::AtomicU64,
    pub messages_played: std::sync::atomic::AtomicU64,
    pub messages_budget_denied: std::sync::atomic::AtomicU64,
    pub generated_audio_ms: std::sync::atomic::AtomicU64,
    pub played_audio_ms: std::sync::atomic::AtomicU64,
}
