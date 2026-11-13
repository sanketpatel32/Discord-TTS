pub const SAMPLE_RATE: u32 = 24000;
pub const CHANNELS: u8 = 1;
pub const MAX_PCM16_BYTES_PER_ATTEMPT: usize = 30 * SAMPLE_RATE as usize * CHANNELS as usize * 2; // 1,440,000 bytes

#[derive(Debug, PartialEq, Eq)]
pub enum AudioValidationError {
    UnalignedBytes(usize),
    BufferExceeded { current: usize, max: usize },
    EmptyAudio,
}

impl std::fmt::Display for AudioValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnalignedBytes(len) => {
                write!(f, "PCM16 audio buffer length {len} is not 2-byte aligned")
            }
            Self::BufferExceeded { current, max } => {
                write!(
                    f,
                    "Audio buffer size {current} bytes exceeds limit of {max} bytes (30s)"
                )
            }
            Self::EmptyAudio => write!(f, "Audio buffer is empty"),
        }
    }
}

impl std::error::Error for AudioValidationError {}

/// Converts little-endian 16-bit signed PCM mono audio bytes into Songbird-compatible interleaved f32 bytes.
pub fn pcm16_to_f32_le_bytes(pcm16: &[u8]) -> Result<Vec<u8>, AudioValidationError> {
    if pcm16.is_empty() {
        return Err(AudioValidationError::EmptyAudio);
    }
    if pcm16.len() % 2 != 0 {
        return Err(AudioValidationError::UnalignedBytes(pcm16.len()));
    }
    if pcm16.len() > MAX_PCM16_BYTES_PER_ATTEMPT {
        return Err(AudioValidationError::BufferExceeded {
            current: pcm16.len(),
            max: MAX_PCM16_BYTES_PER_ATTEMPT,
        });
    }

    let sample_count = pcm16.len() / 2;
    let mut f32_bytes = Vec::with_capacity(sample_count * 4);

    for chunk in pcm16.chunks_exact(2) {
        let sample_i16 = i16::from_le_bytes([chunk[0], chunk[1]]);
        let sample_f32 = sample_i16 as f32 / 32768.0;
        f32_bytes.extend_from_slice(&sample_f32.to_le_bytes());
    }

    Ok(f32_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcm16_to_f32() {
        // Sample silence (0)
        let silence_i16: [i16; 2] = [0, 0];
        let mut pcm = Vec::new();
        for s in silence_i16 {
            pcm.extend_from_slice(&s.to_le_bytes());
        }

        let f32_res = pcm16_to_f32_le_bytes(&pcm).unwrap();
        assert_eq!(f32_res.len(), 8); // 2 samples * 4 bytes
        let s0 = f32::from_le_bytes(f32_res[0..4].try_into().unwrap());
        let s1 = f32::from_le_bytes(f32_res[4..8].try_into().unwrap());
        assert_eq!(s0, 0.0);
        assert_eq!(s1, 0.0);
    }

    #[test]
    fn test_unaligned_bytes() {
        let pcm = vec![0, 1, 2];
        let res = pcm16_to_f32_le_bytes(&pcm);
        assert_eq!(res, Err(AudioValidationError::UnalignedBytes(3)));
    }

    #[test]
    fn test_empty_audio() {
        let pcm = vec![];
        let res = pcm16_to_f32_le_bytes(&pcm);
        assert_eq!(res, Err(AudioValidationError::EmptyAudio));
    }

    #[test]
    fn test_exceeded_buffer() {
        let pcm = vec![0u8; MAX_PCM16_BYTES_PER_ATTEMPT + 2];
        let res = pcm16_to_f32_le_bytes(&pcm);
        assert!(matches!(
            res,
            Err(AudioValidationError::BufferExceeded { .. })
        ));
    }
}
