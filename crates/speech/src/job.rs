//! One transcription job: detect voice, recognize chunk by chunk, label speakers, make turns (T2).
//!
//! Long files are cut at silences into chunks so memory stays flat, and a checkpoint file records each finished
//! chunk, so a job that was cancelled or crashed resumes from the last finished chunk. The checkpoint is keyed by a
//! string the caller builds from everything that changes the result (the recording's SHA-256, the pack, the
//! options); a checkpoint with another key is ignored and replaced.

use crate::pipeline::{self, Chunk, Turn};
use crate::{Result, SpeakerLabeler, SpeakerSpan, SpeechError, SpeechRecognizer, SpeechSpan, VoiceDetector, Word, SAMPLE_RATE};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CHECKPOINT_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct JobOptions {
    pub language: Option<String>,
    pub expected_speakers: Option<u32>,
    /// Longest chunk, seconds.
    pub chunk_seconds: f64,
    /// A wordless stretch of at least this many seconds inside a chunk is recognized again (phase 0, problem 1).
    pub regap_seconds: f64,
    /// Label speakers at all (false for a file with one known speaker).
    pub label_speakers: bool,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self { language: None, expected_speakers: None, chunk_seconds: 30.0, regap_seconds: 1.5, label_speakers: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum Progress {
    DetectVoice,
    Recognize { done: usize, total: usize, seconds: f64 },
    LabelSpeakers,
    Finished,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Checkpoint {
    version: u32,
    key: String,
    speech: Vec<SpeechSpan>,
    chunks: Vec<Chunk>,
    /// One entry per chunk; `None` until the chunk is recognized.
    words: Vec<Option<Vec<Word>>>,
    speakers: Option<Vec<SpeakerSpan>>,
}

impl Checkpoint {
    fn load(path: &Path, key: &str) -> Option<Self> {
        let c: Checkpoint = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        (c.version == CHECKPOINT_VERSION && c.key == key && c.words.len() == c.chunks.len()).then_some(c)
    }

    fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("saving");
        std::fs::write(&tmp, serde_json::to_vec(self).map_err(|e| SpeechError::Io(std::io::Error::other(e)))?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

pub struct JobOutput {
    pub speech: Vec<SpeechSpan>,
    pub speaker_spans: Vec<SpeakerSpan>,
    pub turns: Vec<Turn>,
    /// How many chunks came from the checkpoint rather than being recognized now.
    pub resumed_chunks: usize,
}

/// Run the job on 16 kHz mono `samples`. `progress` returns false to cancel; the checkpoint stays, so the next run
/// with the same key carries on. The checkpoint is removed when the job finishes.
pub fn run(
    samples: &[f32],
    vad: &dyn VoiceDetector,
    asr: &dyn SpeechRecognizer,
    labeller: &dyn SpeakerLabeler,
    opts: &JobOptions,
    checkpoint: &Path,
    key: &str,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<JobOutput> {
    let duration = samples.len() as f64 / SAMPLE_RATE as f64;
    let mut cp = match Checkpoint::load(checkpoint, key) {
        Some(c) => c,
        None => {
            if !progress(Progress::DetectVoice) {
                return Err(SpeechError::Cancelled);
            }
            let speech = vad.detect(samples)?;
            let chunks = pipeline::chunks_from_speech(&speech, opts.chunk_seconds, 0.25, duration);
            let c = Checkpoint { version: CHECKPOINT_VERSION, key: key.into(), words: vec![None; chunks.len()], speech, chunks, speakers: None };
            c.save(checkpoint)?;
            c
        }
    };
    let resumed = cp.words.iter().filter(|w| w.is_some()).count();
    let total = cp.chunks.len();
    for i in 0..total {
        if cp.words[i].is_some() {
            continue;
        }
        let done = cp.words.iter().filter(|w| w.is_some()).count();
        if !progress(Progress::Recognize { done, total, seconds: cp.chunks[i].start }) {
            return Err(SpeechError::Cancelled);
        }
        let w = pipeline::recognize_chunk(asr, samples, cp.chunks[i], opts.language.as_deref(), opts.regap_seconds)?;
        cp.words[i] = Some(w);
        cp.save(checkpoint)?;
    }
    if !progress(Progress::Recognize { done: total, total, seconds: duration }) {
        return Err(SpeechError::Cancelled);
    }
    let speaker_spans = match cp.speakers.clone() {
        Some(s) => s,
        None if opts.label_speakers => {
            if !progress(Progress::LabelSpeakers) {
                return Err(SpeechError::Cancelled);
            }
            let s = labeller.label_speech(samples, &cp.speech, opts.expected_speakers)?;
            cp.speakers = Some(s.clone());
            cp.save(checkpoint)?;
            s
        }
        None => Vec::new(),
    };
    let mut words: Vec<Word> = cp.words.iter().flatten().flatten().cloned().collect();
    words.sort_by(|a, b| a.start.total_cmp(&b.start));
    let labels = if speaker_spans.is_empty() { vec![0; words.len()] } else { pipeline::speakers_for_words(&words, &speaker_spans) };
    let turns = pipeline::turns(words, &labels);
    let _ = std::fs::remove_file(checkpoint);
    progress(Progress::Finished);
    Ok(JobOutput { speech: cp.speech, speaker_spans, turns, resumed_chunks: resumed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Speech everywhere the signal is not zero, in 1-second blocks.
    struct BlockVad;
    impl VoiceDetector for BlockVad {
        fn detect(&self, s: &[f32]) -> Result<Vec<SpeechSpan>> {
            Ok(s.chunks(16_000)
                .enumerate()
                .filter(|(_, b)| b.iter().any(|x| *x != 0.0))
                .map(|(i, _)| SpeechSpan { start: i as f64 + 0.1, end: i as f64 + 0.9 })
                .collect())
        }
    }

    /// One word per call per second of input, counting how often it is called.
    struct CountingAsr<'a>(&'a Cell<usize>);
    impl SpeechRecognizer for CountingAsr<'_> {
        fn recognize(&self, s: &[f32], offset: f64, _: Option<&str>) -> Result<Vec<Word>> {
            self.0.set(self.0.get() + 1);
            let n = (s.len() as f64 / 16_000.0).round().max(1.0) as usize;
            Ok((0..n).map(|i| Word { text: format!("w{:.0}", offset + i as f64), start: offset + i as f64 + 0.3, end: offset + i as f64 + 0.6, confidence: 1.0 }).collect())
        }
        fn languages(&self) -> &[&'static str] {
            &["en"]
        }
    }

    struct TwoHalves;
    impl SpeakerLabeler for TwoHalves {
        fn label(&self, s: &[f32], _: Option<u32>) -> Result<Vec<SpeakerSpan>> {
            let mid = s.len() as f64 / 32_000.0;
            Ok(vec![SpeakerSpan { start: 0.0, end: mid, speaker: 0 }, SpeakerSpan { start: mid, end: mid * 2.0, speaker: 1 }])
        }
    }

    #[test]
    fn cancel_then_resume_from_the_last_finished_chunk() {
        let d = tempfile::tempdir().unwrap();
        let cp = d.path().join("job.json");
        let samples = vec![0.1f32; 16_000 * 40];
        let opts = JobOptions { chunk_seconds: 5.0, ..Default::default() };
        let calls = Cell::new(0);
        let asr = CountingAsr(&calls);

        // Cancel when the third chunk is about to start.
        let r = run(&samples, &BlockVad, &asr, &TwoHalves, &opts, &cp, "key-1", &mut |p| !matches!(p, Progress::Recognize { done: 2, .. }));
        assert!(matches!(r, Err(SpeechError::Cancelled)));
        assert!(cp.exists(), "the checkpoint stays after a cancel");
        let first = calls.get();

        // The next run carries on: it does not hear the first two chunks again.
        let out = run(&samples, &BlockVad, &asr, &TwoHalves, &opts, &cp, "key-1", &mut |_| true).unwrap();
        assert_eq!(out.resumed_chunks, 2);
        let total_chunks = out.speech.len().div_ceil(5);
        assert!(calls.get() - first <= total_chunks - 2 + 1, "only the remaining chunks were recognized");
        assert!(!cp.exists(), "the checkpoint is removed when the job finishes");
        let words: Vec<&Word> = out.turns.iter().flat_map(|t| &t.words).collect();
        assert!(words.windows(2).all(|p| p[0].start <= p[1].start), "word times only go forward");
        assert_eq!(out.turns.len(), 2, "two speakers, two turns");
    }

    #[test]
    fn a_checkpoint_for_another_key_is_ignored() {
        let d = tempfile::tempdir().unwrap();
        let cp = d.path().join("job.json");
        let samples = vec![0.1f32; 16_000 * 12];
        let opts = JobOptions { chunk_seconds: 5.0, ..Default::default() };
        let calls = Cell::new(0);
        let asr = CountingAsr(&calls);
        let _ = run(&samples, &BlockVad, &asr, &TwoHalves, &opts, &cp, "a", &mut |p| !matches!(p, Progress::Recognize { done: 1, .. }));
        let out = run(&samples, &BlockVad, &asr, &TwoHalves, &opts, &cp, "b", &mut |_| true).unwrap();
        assert_eq!(out.resumed_chunks, 0);
    }
}
