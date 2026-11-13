use songbird::input::RawAdapter;
use std::io::Cursor;

pub struct VoiceSessionManager;

impl VoiceSessionManager {
    /// Creates a Songbird Track from interleaved f32 PCM bytes at 24000Hz mono.
    pub fn create_track(f32_bytes: Vec<u8>) -> songbird::input::Input {
        let cursor = Cursor::new(f32_bytes);
        let adapter = RawAdapter::new(cursor, 24000, 1);
        adapter.into()
    }
}
