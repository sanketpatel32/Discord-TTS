pub mod fidelity;
pub mod openai_realtime;
pub mod provider;

pub use fidelity::{NarrationOutcome, verify_narration};
pub use openai_realtime::OpenAIRealtimeProvider;
pub use provider::{MockSpeechProvider, ProviderError, SpeechProvider, SynthesisResult};
