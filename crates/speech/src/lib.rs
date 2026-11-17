//! The speech engine adapter (C5 in the Shared Core Development List).
//!
//! Prism Transcribe talks to speech models only through this crate, and only through three jobs:
//!
//! 1. **detect voice** ([`VoiceDetector`]): where in a recording someone is speaking;
//! 2. **recognize speech** ([`SpeechRecognizer`]): the words in a stretch of speech, each with its start, end and
//!    a confidence score;
//! 3. **label speakers** ([`SpeakerLabeler`]): who spoke when, as anonymous labels (0, 1, 2 ...).
//!
//! The contract is deliberately small so that this crate can move into the shared core (`openstage-core`) without
//! changing its callers, and so that a second engine (OpenStage Author's `whisper-cli`, or the heavy pack's
//! whisper.cpp) can sit behind the same traits as another plugin. What Transcribe asks of the core is written in
//! `CORE-REQUESTS.md` at the top of this repository.
//!
//! Audio crosses the contract as 16 kHz mono `f32` samples in -1..1 ([`SAMPLE_RATE`]). Times are seconds from the
//! start of the recording, as `f64`. Nothing in this crate touches the network.

pub mod cluster;
pub mod install;
pub mod job;
pub mod models;
pub mod pipeline;
pub mod sherpa;
pub mod wav;

use serde::{Deserialize, Serialize};

/// Every model behind the contract works at 16 kHz, mono.
pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
    #[error("a model file is missing: {0}")]
    MissingModel(String),
    #[error("the speech engine could not load {what}: check that the model files are complete")]
    LoadFailed { what: String },
    #[error("the speech engine returned no result for {what}")]
    NoResult { what: String },
    #[error("the audio file could not be read: {0}")]
    Audio(String),
    #[error("the job was cancelled")]
    Cancelled,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SpeechError>;

/// A stretch of a recording where a voice was detected.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeechSpan {
    pub start: f64,
    pub end: f64,
}

/// One recognized word. `text` keeps the punctuation the recognizer attached to it ("Thanks,").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
    /// 0..1. The recognizer's own probability for the word (geometric mean over its tokens). 1.0 when the engine
    /// gives no probabilities.
    pub confidence: f32,
}

/// A stretch of a recording given to one anonymous speaker label.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeakerSpan {
    pub start: f64,
    pub end: f64,
    pub speaker: u32,
}

/// Job 1: detect voice.
pub trait VoiceDetector {
    fn detect(&self, samples: &[f32]) -> Result<Vec<SpeechSpan>>;
}

/// Job 2: recognize speech. `offset` is the time of `samples[0]` in the recording, so the words come back in
/// recording time. `language` is an ISO 639-1 code or `None` for the model's own choice.
pub trait SpeechRecognizer {
    fn recognize(&self, samples: &[f32], offset: f64, language: Option<&str>) -> Result<Vec<Word>>;
    /// The languages the model can recognize (ISO 639-1), so the app can offer another pack for the rest.
    fn languages(&self) -> &[&'static str];
}

/// Job 3: label speakers. `expected_speakers` is the user's hint ("expected number of speakers" at import);
/// `None` lets the engine decide by its threshold.
pub trait SpeakerLabeler {
    fn label(&self, samples: &[f32], expected_speakers: Option<u32>) -> Result<Vec<SpeakerSpan>>;
    /// The same, reusing speech already detected by job 1, for labellers that cut at pauses. The default ignores
    /// the spans.
    fn label_speech(&self, samples: &[f32], speech: &[SpeechSpan], expected_speakers: Option<u32>) -> Result<Vec<SpeakerSpan>> {
        let _ = speech;
        self.label(samples, expected_speakers)
    }
}
