//! Read a WAV file into the contract's audio: 16 kHz, mono, `f32`.
//!
//! Phase 0 reads WAV only (bench and tests). Phase 1 adds decoding for MP3, M4A, FLAC and OGG in Rust and video
//! through the separate FFmpeg program; whatever decodes, it ends in [`to_contract_audio`].

use crate::{Result, SpeechError, SAMPLE_RATE};
use std::path::Path;

/// Audio as the speech engine wants it.
pub struct Audio {
    pub samples: Vec<f32>,
    /// The file's own rate and channel count, for the record.
    pub source_rate: u32,
    pub source_channels: u16,
}

impl Audio {
    pub fn duration(&self) -> f64 {
        self.samples.len() as f64 / SAMPLE_RATE as f64
    }
}

pub fn read_wav(path: &Path) -> Result<Audio> {
    let mut reader = hound::WavReader::open(path).map_err(|e| SpeechError::Audio(format!("{}: {e}", path.display())))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<std::result::Result<_, _>>(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect()
        }
    }
    .map_err(|e| SpeechError::Audio(format!("{}: {e}", path.display())))?;
    let mono: Vec<f32> = if channels == 1 {
        interleaved
    } else {
        interleaved.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
    };
    Ok(Audio { samples: to_contract_audio(mono, spec.sample_rate)?, source_rate: spec.sample_rate, source_channels: spec.channels })
}

/// Resample mono audio to 16 kHz.
pub fn to_contract_audio(mono: Vec<f32>, rate: u32) -> Result<Vec<f32>> {
    if rate == SAMPLE_RATE {
        return Ok(mono);
    }
    let r = sherpa_onnx::LinearResampler::create(rate as i32, SAMPLE_RATE as i32)
        .ok_or_else(|| SpeechError::Audio(format!("cannot resample from {rate} Hz")))?;
    // In pieces, so memory for a long file stays near the size of the input and output.
    let mut out = Vec::with_capacity(mono.len() * SAMPLE_RATE as usize / rate as usize + 1024);
    let piece = rate as usize * 60;
    let n = mono.len().div_ceil(piece).max(1);
    for (i, chunk) in mono.chunks(piece).enumerate() {
        out.extend(r.resample(chunk, i + 1 == n));
    }
    Ok(out)
}
