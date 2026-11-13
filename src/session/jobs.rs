use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct SpeechJob {
    pub job_id: String,
    pub guild_id: u64,
    pub voice_channel_id: u64,
    pub session_epoch: u64,
    pub message_id: u64,
    pub author_id: u64,
    pub received_at_monotonic: Instant,
    pub message_created_at: DateTime<Utc>,
    pub normalized_text: String,
    pub retry_count: u32,
}

impl SpeechJob {
    pub fn is_expired(&self, max_age_seconds: u64) -> bool {
        self.received_at_monotonic.elapsed().as_secs() >= max_age_seconds
    }
}

pub struct UserRateLimiter {
    tokens: f32,
    last_update: Instant,
}

impl Default for UserRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl UserRateLimiter {
    pub fn new() -> Self {
        Self {
            tokens: 3.0, // burst 3
            last_update: Instant::now(),
        }
    }

    pub fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        // refill 1 message every 3 seconds = 1.0 / 3.0 tokens per second
        self.tokens = (self.tokens + elapsed * (1.0 / 3.0)).min(3.0);

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

pub struct GuildAdmissionController {
    author_limiters: HashMap<u64, UserRateLimiter>,
    dedupe_ring: VecDeque<u64>,
    dedupe_set: std::collections::HashSet<u64>,
    deleted_tombstones: VecDeque<u64>,
    max_dedupe_entries: usize,
    max_authors: usize,
}

impl Default for GuildAdmissionController {
    fn default() -> Self {
        Self::new()
    }
}

impl GuildAdmissionController {
    pub fn new() -> Self {
        Self {
            author_limiters: HashMap::new(),
            dedupe_ring: VecDeque::with_capacity(1024),
            dedupe_set: std::collections::HashSet::with_capacity(1024),
            deleted_tombstones: VecDeque::with_capacity(256),
            max_dedupe_entries: 1024,
            max_authors: 1024,
        }
    }

    pub fn is_deleted(&self, message_id: u64) -> bool {
        self.deleted_tombstones.contains(&message_id)
    }

    pub fn record_deletion(&mut self, message_id: u64) {
        if !self.deleted_tombstones.contains(&message_id) {
            if self.deleted_tombstones.len() >= 256 {
                self.deleted_tombstones.pop_front();
            }
            self.deleted_tombstones.push_back(message_id);
        }
    }

    pub fn check_and_record_duplicate(&mut self, message_id: u64) -> bool {
        if self.dedupe_set.contains(&message_id) {
            return true; // Duplicate
        }

        if self.dedupe_ring.len() >= self.max_dedupe_entries {
            if let Some(old_id) = self.dedupe_ring.pop_front() {
                self.dedupe_set.remove(&old_id);
            }
        }

        self.dedupe_ring.push_back(message_id);
        self.dedupe_set.insert(message_id);
        false
    }

    pub fn check_user_rate_limit(&mut self, author_id: u64) -> bool {
        // Enforce max authors bound: if map exceeds 1024, evict random or oldest entries
        if self.author_limiters.len() >= self.max_authors
            && !self.author_limiters.contains_key(&author_id)
        {
            // Evict 64 idle entries
            let keys_to_remove: Vec<u64> = self.author_limiters.keys().take(64).cloned().collect();
            for k in keys_to_remove {
                self.author_limiters.remove(&k);
            }
        }

        let limiter = self.author_limiters.entry(author_id).or_default();

        limiter.try_consume()
    }
}

pub struct GuildJobQueue {
    queue: VecDeque<SpeechJob>,
    max_capacity: usize,
}

impl GuildJobQueue {
    pub fn new(max_capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(max_capacity),
            max_capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Enqueues a job. If full, drops the newest arrival (returns Err) and preserves existing work.
    pub fn enqueue(&mut self, job: SpeechJob) -> Result<(), SpeechJob> {
        if self.queue.len() >= self.max_capacity {
            Err(job)
        } else {
            self.queue.push_back(job);
            Ok(())
        }
    }

    pub fn pop_next(&mut self) -> Option<SpeechJob> {
        self.queue.pop_front()
    }

    pub fn clear(&mut self) -> usize {
        let count = self.queue.len();
        self.queue.clear();
        count
    }

    pub fn cancel_message(&mut self, message_id: u64) -> bool {
        if let Some(pos) = self.queue.iter().position(|j| j.message_id == message_id) {
            self.queue.remove(pos);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_overflow_drops_newest() {
        let mut q = GuildJobQueue::new(2);
        let make_job = |id| SpeechJob {
            job_id: format!("job_{id}"),
            guild_id: 1,
            voice_channel_id: 10,
            session_epoch: 1,
            message_id: id,
            author_id: 99,
            received_at_monotonic: Instant::now(),
            message_created_at: Utc::now(),
            normalized_text: "test".to_string(),
            retry_count: 0,
        };

        assert!(q.enqueue(make_job(1)).is_ok());
        assert!(q.enqueue(make_job(2)).is_ok());
        // Third should fail and preserve 1 and 2
        assert!(q.enqueue(make_job(3)).is_err());
        assert_eq!(q.pop_next().unwrap().message_id, 1);
        assert_eq!(q.pop_next().unwrap().message_id, 2);
    }

    #[test]
    fn test_deduplication() {
        let mut ctrl = GuildAdmissionController::new();
        assert!(!ctrl.check_and_record_duplicate(101));
        assert!(ctrl.check_and_record_duplicate(101));
        assert!(!ctrl.check_and_record_duplicate(102));
    }

    #[test]
    fn test_rate_limiter() {
        let mut ctrl = GuildAdmissionController::new();
        assert!(ctrl.check_user_rate_limit(1));
        assert!(ctrl.check_user_rate_limit(1));
        assert!(ctrl.check_user_rate_limit(1));
        // Burst is 3, 4th must fail
        assert!(!ctrl.check_user_rate_limit(1));
    }
}
