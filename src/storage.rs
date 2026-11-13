use chrono::Utc;
use chrono_tz::Tz;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

use crate::config::Config;

#[derive(Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
    config: Arc<Config>,
    tz: Tz,
}

#[derive(Debug, Clone)]
pub struct GuildSettingsRecord {
    pub guild_id: u64,
    pub selected_voice: String,
    pub volume: u8,
}

#[derive(Debug, Clone, Default)]
pub struct MonthlyUsageSummary {
    pub period: String,
    pub generated_ms: u64,
    pub played_ms: u64,
    pub request_count: u64,
    pub input_text_tokens: u64,
    pub output_text_tokens: u64,
    pub output_audio_tokens: u64,
    pub estimated_microdollars: i64,
}

#[derive(Debug, Clone, Default)]
pub struct GlobalBudgetStatus {
    pub period: String,
    pub spent_microdollars: i64,
    pub reserved_microdollars: i64,
    pub budget_allowance_microdollars: i64,
    pub remaining_microdollars: i64,
    pub is_available: bool,
}

impl Storage {
    pub fn open(path: &Path, config: Arc<Config>) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let tz = config
            .budget_timezone
            .parse::<Tz>()
            .unwrap_or(chrono_tz::Asia::Kolkata);

        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
            config,
            tz,
        };

        storage.migrate()?;
        storage.reconcile_unfinished_reservations_on_startup()?;

        Ok(storage)
    }

    fn migrate(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS guild_settings (
                guild_id TEXT PRIMARY KEY,
                selected_voice TEXT NOT NULL,
                volume INTEGER NOT NULL DEFAULT 100,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS usage_monthly (
                period TEXT NOT NULL,
                guild_id TEXT NOT NULL,
                generated_ms INTEGER NOT NULL DEFAULT 0,
                played_ms INTEGER NOT NULL DEFAULT 0,
                request_count INTEGER NOT NULL DEFAULT 0,
                input_text_tokens INTEGER NOT NULL DEFAULT 0,
                output_text_tokens INTEGER NOT NULL DEFAULT 0,
                output_audio_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_microdollars INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (period, guild_id)
            );

            CREATE TABLE IF NOT EXISTS generation_reservations (
                attempt_id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                guild_id TEXT NOT NULL,
                period TEXT NOT NULL,
                pricing_version TEXT NOT NULL,
                reserved_microdollars INTEGER NOT NULL,
                observed_microdollars INTEGER NOT NULL DEFAULT 0,
                terminal_status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            ",
        )?;

        let mut stmt = conn.prepare("SELECT COALESCE(MAX(version), 0) FROM schema_migrations")?;
        let current_version: i32 = stmt.query_row([], |row| row.get(0))?;

        if current_version < 1 {
            conn.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (1, ?1)",
                params![Utc::now().to_rfc3339()],
            )?;
            info!("Applied storage migration version 1");
        }

        Ok(())
    }

    pub fn current_period(&self) -> String {
        let now = Utc::now().with_timezone(&self.tz);
        now.format("%Y-%m").to_string()
    }

    fn reconcile_unfinished_reservations_on_startup(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let now_str = Utc::now().to_rfc3339();

        let updated = conn.execute(
            "UPDATE generation_reservations
             SET terminal_status = 'uncertain', updated_at = ?1
             WHERE terminal_status = 'active'",
            params![now_str],
        )?;

        if updated > 0 {
            warn!("Reconciled {updated} unfinished reservations from previous run as uncertain");
        }
        Ok(())
    }

    pub fn get_guild_settings(
        &self,
        guild_id: u64,
    ) -> Result<GuildSettingsRecord, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT selected_voice, volume FROM guild_settings WHERE guild_id = ?1")?;
        let res = stmt
            .query_row(params![guild_id.to_string()], |row| {
                Ok(GuildSettingsRecord {
                    guild_id,
                    selected_voice: row.get(0)?,
                    volume: row.get::<_, i64>(1)? as u8,
                })
            })
            .optional()?;

        Ok(res.unwrap_or_else(|| GuildSettingsRecord {
            guild_id,
            selected_voice: self.config.default_voice.clone(),
            volume: 100,
        }))
    }

    pub fn set_guild_voice(&self, guild_id: u64, voice: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let now_str = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO guild_settings (guild_id, selected_voice, volume, updated_at)
             VALUES (?1, ?2, 100, ?3)
             ON CONFLICT(guild_id) DO UPDATE SET selected_voice = ?2, updated_at = ?3",
            params![guild_id.to_string(), voice, now_str],
        )?;
        Ok(())
    }

    pub fn set_guild_volume(&self, guild_id: u64, volume: u8) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let now_str = Utc::now().to_rfc3339();
        let vol_i64 = volume as i64;
        conn.execute(
            "INSERT INTO guild_settings (guild_id, selected_voice, volume, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(guild_id) DO UPDATE SET volume = ?3, updated_at = ?4",
            params![
                guild_id.to_string(),
                &self.config.default_voice,
                vol_i64,
                now_str
            ],
        )?;
        Ok(())
    }

    pub fn get_global_budget_status(&self) -> Result<GlobalBudgetStatus, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let period = self.current_period();

        let mut spent_stmt = conn.prepare(
            "SELECT COALESCE(SUM(estimated_microdollars), 0) FROM usage_monthly WHERE period = ?1",
        )?;
        let spent: i64 = spent_stmt.query_row(params![&period], |row| row.get(0))?;

        let mut res_stmt = conn.prepare(
            "SELECT COALESCE(SUM(reserved_microdollars), 0) FROM generation_reservations
             WHERE period = ?1 AND terminal_status = 'active'",
        )?;
        let reserved: i64 = res_stmt.query_row(params![&period], |row| row.get(0))?;

        let budget_allowance = (self.config.monthly_budget_usd as i64
            - self.config.monthly_budget_reserve_usd as i64)
            * 1_000_000;

        let remaining = (budget_allowance - (spent + reserved)).max(0);
        let is_available = spent + reserved < budget_allowance;

        Ok(GlobalBudgetStatus {
            period,
            spent_microdollars: spent,
            reserved_microdollars: reserved,
            budget_allowance_microdollars: budget_allowance,
            remaining_microdollars: remaining,
            is_available,
        })
    }

    pub fn get_guild_usage_summary(
        &self,
        guild_id: u64,
    ) -> Result<MonthlyUsageSummary, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let period = self.current_period();

        let mut stmt = conn.prepare(
            "SELECT generated_ms, played_ms, request_count, input_text_tokens,
                    output_text_tokens, output_audio_tokens, estimated_microdollars
             FROM usage_monthly WHERE period = ?1 AND guild_id = ?2",
        )?;

        let row = stmt
            .query_row(params![&period, guild_id.to_string()], |row| {
                Ok(MonthlyUsageSummary {
                    period: period.clone(),
                    generated_ms: row.get::<_, i64>(0)? as u64,
                    played_ms: row.get::<_, i64>(1)? as u64,
                    request_count: row.get::<_, i64>(2)? as u64,
                    input_text_tokens: row.get::<_, i64>(3)? as u64,
                    output_text_tokens: row.get::<_, i64>(4)? as u64,
                    output_audio_tokens: row.get::<_, i64>(5)? as u64,
                    estimated_microdollars: row.get(6)?,
                })
            })
            .optional()?;

        Ok(row.unwrap_or_else(|| MonthlyUsageSummary {
            period,
            ..Default::default()
        }))
    }

    /// Conservatively estimates upper bound microdollars for 1 generation attempt.
    pub fn estimate_attempt_upper_bound(&self, text_chars: usize) -> i64 {
        // ~1.33 tokens per word; max 400 chars ~ 80 words ~ 110 tokens + 100 context = 210 tokens
        let input_tokens = (text_chars as f64 * 0.35 + 120.0).round() as i64;
        let max_output_tokens = 1024; // audio + transcript
        let input_cost =
            (input_tokens * self.config.price_text_input_microdollars_per_million) / 1_000_000;
        let audio_cost = (max_output_tokens
            * self.config.price_audio_output_microdollars_per_million)
            / 1_000_000;
        let text_out_cost =
            (200 * self.config.price_text_output_microdollars_per_million) / 1_000_000;

        // Add 10% safety margin
        let total = input_cost + audio_cost + text_out_cost;
        (total as f64 * 1.10).ceil() as i64
    }

    /// Atomically check budget and reserve microdollars for an attempt.
    pub fn reserve_attempt_budget(
        &self,
        attempt_id: &str,
        job_id: &str,
        guild_id: u64,
        text_chars: usize,
    ) -> Result<i64, String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction()
            .map_err(|e| format!("Database transaction error: {e}"))?;

        let period = self.current_period();

        let (spent, active_reservations) = {
            let mut spent_stmt = tx
                .prepare("SELECT COALESCE(SUM(estimated_microdollars), 0) FROM usage_monthly WHERE period = ?1")
                .map_err(|e| e.to_string())?;
            let spent: i64 = spent_stmt
                .query_row(params![&period], |row| row.get(0))
                .map_err(|e| e.to_string())?;

            let mut res_stmt = tx
                .prepare(
                    "SELECT COALESCE(SUM(reserved_microdollars), 0) FROM generation_reservations
                     WHERE period = ?1 AND terminal_status = 'active'",
                )
                .map_err(|e| e.to_string())?;
            let active_reservations: i64 = res_stmt
                .query_row(params![&period], |row| row.get(0))
                .map_err(|e| e.to_string())?;

            (spent, active_reservations)
        };

        let budget_allowance = (self.config.monthly_budget_usd as i64
            - self.config.monthly_budget_reserve_usd as i64)
            * 1_000_000;

        let reservation_amount = self.estimate_attempt_upper_bound(text_chars);

        if spent + active_reservations + reservation_amount > budget_allowance {
            return Err(format!(
                "Monthly budget limit reached: spent=${:.2}, reserved=${:.2}, allowance=${:.2}",
                spent as f64 / 1_000_000.0,
                active_reservations as f64 / 1_000_000.0,
                budget_allowance as f64 / 1_000_000.0
            ));
        }

        let now_str = Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO generation_reservations
             (attempt_id, job_id, guild_id, period, pricing_version, reserved_microdollars,
              observed_microdollars, terminal_status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 'active', ?7, ?7)",
            params![
                attempt_id,
                job_id,
                guild_id.to_string(),
                &period,
                &self.config.pricing_verified_date,
                reservation_amount,
                now_str
            ],
        )
        .map_err(|e| format!("Failed to record reservation: {e}"))?;

        tx.commit()
            .map_err(|e| format!("Failed to commit reservation: {e}"))?;

        Ok(reservation_amount)
    }

    /// Reconciles terminal usage and settles reservation.
    pub fn finalize_attempt_usage(
        &self,
        attempt_id: &str,
        guild_id: u64,
        generated_ms: u64,
        played_ms: u64,
        input_tokens: u64,
        cached_tokens: u64,
        output_text_tokens: u64,
        output_audio_tokens: u64,
        terminal_status: &str, // 'completed', 'canceled', 'failed', 'uncertain'
    ) -> Result<i64, rusqlite::Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let period = self.current_period();

        // Calculate actual microdollars
        let input_cost = (input_tokens as i64
            * self.config.price_text_input_microdollars_per_million)
            / 1_000_000;
        let cached_cost = (cached_tokens as i64
            * self.config.price_cached_text_input_microdollars_per_million)
            / 1_000_000;
        let text_out_cost = (output_text_tokens as i64
            * self.config.price_text_output_microdollars_per_million)
            / 1_000_000;
        let audio_out_cost = (output_audio_tokens as i64
            * self.config.price_audio_output_microdollars_per_million)
            / 1_000_000;
        let mut actual_cost = input_cost + cached_cost + text_out_cost + audio_out_cost;

        // If uncertain or failed without usage known, retain reservation amount
        if terminal_status == "uncertain" && actual_cost == 0 {
            let res_amt_opt: Option<i64> = {
                let mut stmt = tx.prepare("SELECT reserved_microdollars FROM generation_reservations WHERE attempt_id = ?1")?;
                stmt.query_row(params![attempt_id], |row| row.get::<_, i64>(0))
                    .optional()?
            };
            if let Some(res_amt) = res_amt_opt {
                actual_cost = res_amt;
            }
        }

        let now_str = Utc::now().to_rfc3339();

        tx.execute(
            "UPDATE generation_reservations
             SET observed_microdollars = ?1, terminal_status = ?2, updated_at = ?3
             WHERE attempt_id = ?4",
            params![actual_cost, terminal_status, now_str, attempt_id],
        )?;

        // Update monthly usage aggregate
        tx.execute(
            "INSERT INTO usage_monthly
             (period, guild_id, generated_ms, played_ms, request_count, input_text_tokens,
              output_text_tokens, output_audio_tokens, estimated_microdollars)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8)
             ON CONFLICT(period, guild_id) DO UPDATE SET
                 generated_ms = generated_ms + ?3,
                 played_ms = played_ms + ?4,
                 request_count = request_count + 1,
                 input_text_tokens = input_text_tokens + ?5,
                 output_text_tokens = output_text_tokens + ?6,
                 output_audio_tokens = output_audio_tokens + ?7,
                 estimated_microdollars = estimated_microdollars + ?8",
            params![
                &period,
                guild_id.to_string(),
                generated_ms as i64,
                played_ms as i64,
                input_tokens as i64,
                output_text_tokens as i64,
                output_audio_tokens as i64,
                actual_cost
            ],
        )?;

        tx.commit()?;
        Ok(actual_cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Arc<Config> {
        Arc::new(Config {
            discord_token: "test".to_string(),
            discord_application_id: 1,
            allowed_guild_ids: vec![100, 200],
            bot_owner_user_ids: [999].into_iter().collect(),
            controller_role_ids_by_guild: Default::default(),
            openai_api_key: "test".to_string(),
            openai_model: "gpt-realtime-2.1-mini".to_string(),
            openai_reasoning_effort: "minimal".to_string(),
            default_voice: "marin".to_string(),
            allowed_voices: vec!["marin".to_string(), "cedar".to_string()],
            database_path: ":memory:".into(),
            max_guilds: 4,
            max_pending_per_guild: 20,
            max_concurrent_generations: 2,
            max_text_chars: 400,
            max_text_bytes: 4096,
            max_audio_seconds: 30,
            max_job_age_seconds: 60,
            generation_timeout_seconds: 20,
            max_attempts_per_job: 2,
            empty_channel_timeout_seconds: 60,
            openai_idle_timeout_seconds: 120,
            monthly_budget_usd: 30,
            monthly_budget_reserve_usd: 0,
            budget_timezone: "Asia/Kolkata".to_string(),
            price_text_input_microdollars_per_million: 600_000,
            price_cached_text_input_microdollars_per_million: 60_000,
            price_text_output_microdollars_per_million: 2_400_000,
            price_audio_output_microdollars_per_million: 20_000_000,
            pricing_verified_date: "2026-11-13".to_string(),
        })
    }

    #[test]
    fn test_storage_reservation_and_finalization() {
        let storage = Storage::open(Path::new(":memory:"), test_config()).unwrap();

        let reserved = storage
            .reserve_attempt_budget("att_1", "job_1", 100, 50)
            .unwrap();
        assert!(reserved > 0);

        let status = storage.get_global_budget_status().unwrap();
        assert_eq!(status.reserved_microdollars, reserved);

        let finalized = storage
            .finalize_attempt_usage("att_1", 100, 3000, 3000, 150, 0, 50, 60, "completed")
            .unwrap();
        assert!(finalized > 0);

        let status_after = storage.get_global_budget_status().unwrap();
        assert_eq!(status_after.reserved_microdollars, 0);
        assert_eq!(status_after.spent_microdollars, finalized);
    }
}
