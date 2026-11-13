use tokio::sync::watch;

use crate::session::jobs::SpeechJob;
use crate::speech::fidelity::NarrationOutcome;

#[derive(Debug, Clone)]
pub struct SynthesisResult {
    pub audio_pcm16: Vec<u8>,
    pub transcript: String,
    pub input_tokens: u64,
    pub cached_tokens: u64,
    pub output_text_tokens: u64,
    pub output_audio_tokens: u64,
    pub duration_ms: u64,
    pub outcome: NarrationOutcome,
}

#[derive(Debug, Clone)]
pub enum ProviderError {
    AuthenticationFailed,
    Throttled(Option<u64>),
    Timeout,
    Canceled,
    ConnectionFailed(String),
    BufferLimitExceeded(String),
    InvalidAudio(String),
    Refused(String),
    FidelityCheckFailed {
        outcome: NarrationOutcome,
        transcript: String,
    },
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthenticationFailed => write!(f, "OpenAI API authentication failed"),
            Self::Throttled(wait) => write!(f, "OpenAI API throttled (retry after {wait:?}s)"),
            Self::Timeout => write!(f, "Speech generation timed out"),
            Self::Canceled => write!(f, "Speech generation canceled"),
            Self::ConnectionFailed(msg) => write!(f, "OpenAI connection failed: {msg}"),
            Self::BufferLimitExceeded(msg) => write!(f, "Buffer limit exceeded: {msg}"),
            Self::InvalidAudio(msg) => write!(f, "Invalid audio output: {msg}"),
            Self::Refused(msg) => write!(f, "Speech generation refused by provider: {msg}"),
            Self::FidelityCheckFailed {
                outcome,
                transcript: _,
            } => {
                write!(f, "Narration fidelity check failed ({outcome:?})")
            }
        }
    }
}

impl std::error::Error for ProviderError {}

#[async_trait::async_trait]
pub trait SpeechProvider: Send + Sync {
    async fn synthesize(
        &self,
        job: &SpeechJob,
        voice: &str,
        cancel_rx: watch::Receiver<bool>,
    ) -> Result<SynthesisResult, ProviderError>;
}

/// Mock provider for testing and offline development
pub struct MockSpeechProvider {
    pub should_fail: bool,
    pub fail_fidelity: bool,
}

impl Default for MockSpeechProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockSpeechProvider {
    pub fn new() -> Self {
        Self {
            should_fail: false,
            fail_fidelity: false,
        }
    }
}

#[async_trait::async_trait]
impl SpeechProvider for MockSpeechProvider {
    async fn synthesize(
        &self,
        job: &SpeechJob,
        _voice: &str,
        cancel_rx: watch::Receiver<bool>,
    ) -> Result<SynthesisResult, ProviderError> {
        if *cancel_rx.borrow() {
            return Err(ProviderError::Canceled);
        }

        if self.should_fail {
            return Err(ProviderError::ConnectionFailed("Mock failure".to_string()));
        }

        let transcript = if self.fail_fidelity {
            "I will not read that, here is a joke instead.".to_string()
        } else {
            job.normalized_text.clone()
        };

        let outcome = crate::speech::fidelity::verify_narration(&job.normalized_text, &transcript);
        if outcome != NarrationOutcome::Accepted {
            return Err(ProviderError::FidelityCheckFailed {
                outcome,
                transcript,
            });
        }

        // Generate 0.5s of silent 24kHz mono 16-bit PCM (12,000 samples = 24,000 bytes)
        let sample_count = 12000;
        let mut audio_pcm16 = vec![0u8; sample_count * 2];

        // Synthesize a subtle 440Hz sine wave tone so it's audible if played
        for i in 0..sample_count {
            let t = i as f32 / 24000.0;
            let sample = (f32::sin(2.0 * std::f32::consts::PI * 440.0 * t) * 1000.0) as i16;
            let bytes = sample.to_le_bytes();
            audio_pcm16[i * 2] = bytes[0];
            audio_pcm16[i * 2 + 1] = bytes[1];
        }

        Ok(SynthesisResult {
            audio_pcm16,
            transcript,
            input_tokens: 150,
            cached_tokens: 0,
            output_text_tokens: 50,
            output_audio_tokens: 500,
            duration_ms: 500,
            outcome,
        })
    }
}
