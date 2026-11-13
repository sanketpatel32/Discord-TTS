use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Config {
    pub discord_token: String,
    pub discord_application_id: u64,
    pub allowed_guild_ids: Vec<u64>,
    pub bot_owner_user_ids: HashSet<u64>,
    pub controller_role_ids_by_guild: HashMap<u64, u64>,

    pub openai_api_key: String,
    pub openai_model: String,
    pub openai_reasoning_effort: String,
    pub default_voice: String,
    pub allowed_voices: Vec<String>,

    pub database_path: PathBuf,
    pub max_guilds: usize,
    pub max_pending_per_guild: usize,
    pub max_concurrent_generations: usize,
    pub max_text_chars: usize,
    pub max_text_bytes: usize,
    pub max_audio_seconds: u32,
    pub max_job_age_seconds: u64,
    pub generation_timeout_seconds: u64,
    pub max_attempts_per_job: u32,
    pub empty_channel_timeout_seconds: u64,
    pub openai_idle_timeout_seconds: u64,

    pub monthly_budget_usd: u32,
    pub monthly_budget_reserve_usd: u32,
    pub budget_timezone: String,

    pub price_text_input_microdollars_per_million: i64,
    pub price_cached_text_input_microdollars_per_million: i64,
    pub price_text_output_microdollars_per_million: i64,
    pub price_audio_output_microdollars_per_million: i64,
    pub pricing_verified_date: String,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("discord_token", &"[REDACTED]")
            .field("discord_application_id", &self.discord_application_id)
            .field("allowed_guild_ids", &self.allowed_guild_ids)
            .field("bot_owner_user_ids", &self.bot_owner_user_ids)
            .field(
                "controller_role_ids_by_guild",
                &self.controller_role_ids_by_guild,
            )
            .field("openai_api_key", &"[REDACTED]")
            .field("openai_model", &self.openai_model)
            .field("openai_reasoning_effort", &self.openai_reasoning_effort)
            .field("default_voice", &self.default_voice)
            .field("allowed_voices", &self.allowed_voices)
            .field("database_path", &self.database_path)
            .field("max_guilds", &self.max_guilds)
            .field("max_pending_per_guild", &self.max_pending_per_guild)
            .field(
                "max_concurrent_generations",
                &self.max_concurrent_generations,
            )
            .field("max_text_chars", &self.max_text_chars)
            .field("max_text_bytes", &self.max_text_bytes)
            .field("max_audio_seconds", &self.max_audio_seconds)
            .field("monthly_budget_usd", &self.monthly_budget_usd)
            .field(
                "monthly_budget_reserve_usd",
                &self.monthly_budget_reserve_usd,
            )
            .field("budget_timezone", &self.budget_timezone)
            .finish()
    }
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let _ = dotenvy::dotenv();

        let discord_token = env::var("DISCORD_TOKEN")
            .map_err(|_| "DISCORD_TOKEN environment variable is missing".to_string())?
            .trim()
            .to_string();
        if discord_token.is_empty() {
            return Err("DISCORD_TOKEN cannot be empty".to_string());
        }

        let discord_application_id: u64 = env::var("DISCORD_APPLICATION_ID")
            .unwrap_or_else(|_| "0".to_string())
            .trim()
            .parse()
            .unwrap_or(0);

        let allowed_guild_raw = env::var("ALLOWED_GUILD_IDS")
            .map_err(|_| "ALLOWED_GUILD_IDS environment variable is missing".to_string())?;

        let mut allowed_guild_ids = Vec::new();
        let mut seen_guilds = HashSet::new();
        for s in allowed_guild_raw.split(',') {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                continue;
            }
            let gid: u64 = trimmed
                .parse()
                .map_err(|e| format!("Invalid guild ID '{trimmed}' in ALLOWED_GUILD_IDS: {e}"))?;
            if !seen_guilds.insert(gid) {
                return Err(format!("Duplicate guild ID '{gid}' in ALLOWED_GUILD_IDS"));
            }
            allowed_guild_ids.push(gid);
        }

        if allowed_guild_ids.is_empty() {
            return Err("ALLOWED_GUILD_IDS must contain at least 1 guild ID".to_string());
        }
        if allowed_guild_ids.len() > 4 {
            return Err(format!(
                "ALLOWED_GUILD_IDS specifies {} guilds, but hard maximum is 4",
                allowed_guild_ids.len()
            ));
        }

        let mut bot_owner_user_ids = HashSet::new();
        if let Ok(owners_raw) = env::var("BOT_OWNER_USER_IDS") {
            for s in owners_raw.split(',') {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(uid) = trimmed.parse::<u64>() {
                    bot_owner_user_ids.insert(uid);
                }
            }
        }

        let mut controller_role_ids_by_guild = HashMap::new();
        if let Ok(roles_raw) = env::var("CONTROLLER_ROLE_IDS_BY_GUILD") {
            let trimmed = roles_raw.trim();
            if !trimmed.is_empty() && trimmed != "{}" {
                let parsed: Result<HashMap<String, String>, _> = serde_json::from_str(trimmed);
                match parsed {
                    Ok(map) => {
                        for (g_str, r_str) in map {
                            let gid: u64 = g_str.parse().map_err(|_| {
                                format!(
                                    "Invalid guild ID '{g_str}' in CONTROLLER_ROLE_IDS_BY_GUILD"
                                )
                            })?;
                            let rid: u64 = r_str.parse().map_err(|_| {
                                format!("Invalid role ID '{r_str}' in CONTROLLER_ROLE_IDS_BY_GUILD")
                            })?;
                            if !seen_guilds.contains(&gid) {
                                return Err(format!(
                                    "CONTROLLER_ROLE_IDS_BY_GUILD contains guild ID '{gid}' which is not in ALLOWED_GUILD_IDS"
                                ));
                            }
                            controller_role_ids_by_guild.insert(gid, rid);
                        }
                    }
                    Err(e) => {
                        return Err(format!(
                            "Failed to parse CONTROLLER_ROLE_IDS_BY_GUILD JSON: {e}"
                        ));
                    }
                }
            }
        }

        let openai_api_key = env::var("OPENAI_API_KEY")
            .map_err(|_| "OPENAI_API_KEY environment variable is missing".to_string())?
            .trim()
            .to_string();
        if openai_api_key.is_empty() {
            return Err("OPENAI_API_KEY cannot be empty".to_string());
        }

        let openai_model = env::var("OPENAI_MODEL")
            .unwrap_or_else(|_| "gpt-realtime-2.1-mini".to_string())
            .trim()
            .to_string();

        let openai_reasoning_effort = env::var("OPENAI_REASONING_EFFORT")
            .unwrap_or_else(|_| "minimal".to_string())
            .trim()
            .to_string();

        let default_voice = env::var("DEFAULT_VOICE")
            .unwrap_or_else(|_| "marin".to_string())
            .trim()
            .to_string();

        let allowed_voices_raw =
            env::var("ALLOWED_VOICES").unwrap_or_else(|_| "marin,cedar".to_string());
        let allowed_voices: Vec<String> = allowed_voices_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if !allowed_voices.contains(&default_voice) {
            return Err(format!(
                "DEFAULT_VOICE '{default_voice}' is not in ALLOWED_VOICES ({allowed_voices:?})"
            ));
        }

        let database_path = PathBuf::from(
            env::var("DATABASE_PATH").unwrap_or_else(|_| "./data/vc-reader.sqlite".to_string()),
        );

        let max_guilds = 4; // Hard boundary
        let max_pending_per_guild: usize = env::var("MAX_PENDING_PER_GUILD")
            .unwrap_or_else(|_| "20".to_string())
            .parse()
            .unwrap_or(20);

        let max_concurrent_generations: usize = env::var("MAX_CONCURRENT_GENERATIONS")
            .unwrap_or_else(|_| "2".to_string())
            .parse()
            .unwrap_or(2);

        let max_text_chars: usize = env::var("MAX_TEXT_CHARS")
            .unwrap_or_else(|_| "400".to_string())
            .parse()
            .unwrap_or(400);

        let max_text_bytes: usize = env::var("MAX_TEXT_BYTES")
            .unwrap_or_else(|_| "4096".to_string())
            .parse()
            .unwrap_or(4096);

        let max_audio_seconds: u32 = env::var("MAX_AUDIO_SECONDS")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .unwrap_or(30);

        let max_job_age_seconds: u64 = env::var("MAX_JOB_AGE_SECONDS")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .unwrap_or(60);

        let generation_timeout_seconds: u64 = env::var("GENERATION_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "20".to_string())
            .parse()
            .unwrap_or(20);

        let max_attempts_per_job: u32 = env::var("MAX_ATTEMPTS_PER_JOB")
            .unwrap_or_else(|_| "2".to_string())
            .parse()
            .unwrap_or(2);

        let empty_channel_timeout_seconds: u64 = env::var("EMPTY_CHANNEL_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .unwrap_or(60);

        let openai_idle_timeout_seconds: u64 = env::var("OPENAI_IDLE_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "120".to_string())
            .parse()
            .unwrap_or(120);

        let monthly_budget_usd: u32 = env::var("MONTHLY_BUDGET_USD")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .unwrap_or(30);

        let monthly_budget_reserve_usd: u32 = env::var("MONTHLY_BUDGET_RESERVE_USD")
            .unwrap_or_else(|_| "0".to_string())
            .parse()
            .unwrap_or(0);

        let budget_timezone = env::var("BUDGET_TIMEZONE")
            .unwrap_or_else(|_| "Asia/Kolkata".to_string())
            .trim()
            .to_string();

        let price_text_input = env::var("PRICE_TEXT_INPUT_PER_MILLION")
            .unwrap_or_else(|_| "0.60".to_string())
            .parse::<f64>()
            .unwrap_or(0.60);

        let price_cached_text_input = env::var("PRICE_CACHED_TEXT_INPUT_PER_MILLION")
            .unwrap_or_else(|_| "0.06".to_string())
            .parse::<f64>()
            .unwrap_or(0.06);

        let price_text_output = env::var("PRICE_TEXT_OUTPUT_PER_MILLION")
            .unwrap_or_else(|_| "2.40".to_string())
            .parse::<f64>()
            .unwrap_or(2.40);

        let price_audio_output = env::var("PRICE_AUDIO_OUTPUT_PER_MILLION")
            .unwrap_or_else(|_| "20.00".to_string())
            .parse::<f64>()
            .unwrap_or(20.00);

        let pricing_verified_date = env::var("PRICING_VERIFIED_DATE")
            .unwrap_or_else(|_| "2026-11-13".to_string())
            .trim()
            .to_string();

        Ok(Self {
            discord_token,
            discord_application_id,
            allowed_guild_ids,
            bot_owner_user_ids,
            controller_role_ids_by_guild,
            openai_api_key,
            openai_model,
            openai_reasoning_effort,
            default_voice,
            allowed_voices,
            database_path,
            max_guilds,
            max_pending_per_guild,
            max_concurrent_generations,
            max_text_chars,
            max_text_bytes,
            max_audio_seconds,
            max_job_age_seconds,
            generation_timeout_seconds,
            max_attempts_per_job,
            empty_channel_timeout_seconds,
            openai_idle_timeout_seconds,
            monthly_budget_usd,
            monthly_budget_reserve_usd,
            budget_timezone,
            price_text_input_microdollars_per_million: (price_text_input * 1_000_000.0).round()
                as i64,
            price_cached_text_input_microdollars_per_million: (price_cached_text_input
                * 1_000_000.0)
                .round() as i64,
            price_text_output_microdollars_per_million: (price_text_output * 1_000_000.0).round()
                as i64,
            price_audio_output_microdollars_per_million: (price_audio_output * 1_000_000.0).round()
                as i64,
            pricing_verified_date,
        })
    }
}
