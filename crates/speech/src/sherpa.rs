//! The sherpa-onnx plugin: Silero VAD, Parakeet TDT (NeMo transducer), and speaker labels from WeSpeaker ResNet34-LM
//! embeddings with our own clustering ([`PauseLabeller`], the default) or with sherpa-onnx's pyannote pipeline
//! ([`PyannoteLabeller`], kept for comparison: phase 0 found it much weaker on focus groups). All on the CPU.

use crate::models::{EMBEDDING_FILE, PARAKEET_DIR, PARAKEET_LANGUAGES, SEGMENTATION_DIR};
use crate::pipeline::words_from_tokens;
use crate::{Result, SpeakerLabeler, SpeakerSpan, SpeechError, SpeechRecognizer, SpeechSpan, VoiceDetector, Word, SAMPLE_RATE};
use sherpa_onnx as so;
use std::path::Path;

fn path_str(dir: &Path, rel: &str) -> Result<String> {
    file_str(&dir.join(rel))
}

fn file_str(p: &Path) -> Result<String> {
    if !p.is_file() {
        return Err(SpeechError::MissingModel(p.display().to_string()));
    }
    Ok(p.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------------------------------------
// Job 1: detect voice
// ---------------------------------------------------------------------------------------------------------

pub struct SileroVad {
    config: so::VadModelConfig,
}

impl SileroVad {
    /// `min_silence` is the shortest pause, in seconds, that ends a stretch of speech; `max_speech` force-splits a
    /// longer stretch so no chunk grows without limit.
    pub fn new(model_dir: &Path, min_silence: f32, max_speech: f32) -> Result<Self> {
        let mut config = so::VadModelConfig::default();
        config.silero_vad.model = Some(path_str(model_dir, "silero_vad.onnx")?);
        config.silero_vad.threshold = 0.5;
        config.silero_vad.min_silence_duration = min_silence;
        config.silero_vad.min_speech_duration = 0.15;
        config.silero_vad.window_size = 512;
        config.silero_vad.max_speech_duration = max_speech;
        config.sample_rate = SAMPLE_RATE as i32;
        config.num_threads = 1;
        Ok(Self { config })
    }
}

impl VoiceDetector for SileroVad {
    fn detect(&self, samples: &[f32]) -> Result<Vec<SpeechSpan>> {
        let buffer_seconds = self.config.silero_vad.max_speech_duration + 5.0;
        let vad = so::VoiceActivityDetector::create(&self.config, buffer_seconds)
            .ok_or_else(|| SpeechError::LoadFailed { what: "Silero VAD".into() })?;
        let mut spans = Vec::new();
        let take =|vad: &so::VoiceActivityDetector, spans: &mut Vec<SpeechSpan>| {
            while !vad.is_empty() {
                if let Some(seg) = vad.front() {
                    let start = seg.start() as f64 / SAMPLE_RATE as f64;
                    spans.push(SpeechSpan { start, end: start + seg.n() as f64 / SAMPLE_RATE as f64 });
                }
                vad.pop();
            }
        };
        for window in samples.chunks(512) {
            vad.accept_waveform(window);
            take(&vad, &mut spans);
        }
        vad.flush();
        take(&vad, &mut spans);
        Ok(spans)
    }
}

// ---------------------------------------------------------------------------------------------------------
// Job 2: recognize speech
// ---------------------------------------------------------------------------------------------------------

pub struct Parakeet {
    recognizer: so::OfflineRecognizer,
}

impl Parakeet {
    pub fn new(model_dir: &Path, threads: i32) -> Result<Self> {
        let d = model_dir.join(PARAKEET_DIR);
        let mut c = so::OfflineRecognizerConfig::default();
        c.model_config.transducer = so::OfflineTransducerModelConfig {
            encoder: Some(path_str(&d, "encoder.int8.onnx")?),
            decoder: Some(path_str(&d, "decoder.int8.onnx")?),
            joiner: Some(path_str(&d, "joiner.int8.onnx")?),
        };
        c.model_config.tokens = Some(path_str(&d, "tokens.txt")?);
        c.model_config.model_type = Some("nemo_transducer".into());
        c.model_config.num_threads = threads;
        c.decoding_method = Some("greedy_search".into());
        let recognizer = so::OfflineRecognizer::create(&c).ok_or_else(|| SpeechError::LoadFailed { what: "Parakeet".into() })?;
        Ok(Self { recognizer })
    }
}

impl SpeechRecognizer for Parakeet {
    fn recognize(&self, samples: &[f32], offset: f64, _language: Option<&str>) -> Result<Vec<Word>> {
        // Parakeet v3 detects the language itself among its 25; there is no language option to pass.
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, samples);
        self.recognizer.decode(&stream);
        let r = stream.get_result().ok_or_else(|| SpeechError::NoResult { what: "a stretch of speech".into() })?;
        let ts = r.timestamps.unwrap_or_default();
        let durations = r.durations.unwrap_or_default();
        let logp = r.ys_log_probs.unwrap_or_default();
        Ok(words_from_tokens(&r.tokens, &ts, &durations, &logp, offset))
    }

    fn languages(&self) -> &[&'static str] {
        PARAKEET_LANGUAGES
    }
}

// ---------------------------------------------------------------------------------------------------------
// Job 3: label speakers
// ---------------------------------------------------------------------------------------------------------

/// Speaker labels from the voice detector's pauses, speaker embeddings and our own clustering
/// ([`crate::cluster`]): speech is cut at every pause of `min_silence` seconds, each stretch is embedded in
/// windows of at most `window` seconds (stepping by half a window), the windows are clustered, and each stretch of
/// time goes to the label of the window centred nearest it.
pub struct PauseLabeller {
    vad: SileroVad,
    extractor: so::SpeakerEmbeddingExtractor,
    window: f64,
    threshold: f32,
}

impl PauseLabeller {
    /// `embedding` replaces the light pack's embedding model with another one sherpa-onnx can load (phase 0 used
    /// it to compare CAM++, community-1's ResNet34 and WeSpeaker's).
    pub fn new(model_dir: &Path, threads: i32, threshold: f32, window: f64, embedding: Option<&Path>) -> Result<Self> {
        let vad = SileroVad::new(model_dir, 0.2, 20.0)?;
        let mut c = so::SpeakerEmbeddingExtractorConfig::default();
        c.model = Some(match embedding {
            Some(p) => file_str(p)?,
            None => path_str(model_dir, EMBEDDING_FILE)?,
        });
        c.num_threads = threads;
        let extractor = so::SpeakerEmbeddingExtractor::create(&c).ok_or_else(|| SpeechError::LoadFailed { what: "the speaker embedding model".into() })?;
        Ok(Self { vad, extractor, window, threshold })
    }

    fn embed(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let s = self.extractor.create_stream()?;
        s.accept_waveform(SAMPLE_RATE as i32, samples);
        s.input_finished();
        if !self.extractor.is_ready(&s) {
            return None;
        }
        self.extractor.compute(&s)
    }
}

impl SpeakerLabeler for PauseLabeller {
    fn label(&self, samples: &[f32], expected_speakers: Option<u32>) -> Result<Vec<SpeakerSpan>> {
        let spans = self.vad.detect(samples)?;
        let sr = SAMPLE_RATE as f64;
        // Windows: (span index, start, end).
        let mut windows: Vec<(usize, f64, f64)> = Vec::new();
        for (si, sp) in spans.iter().enumerate() {
            let len = sp.end - sp.start;
            if len <= self.window * 1.5 {
                windows.push((si, sp.start, sp.end));
            } else {
                let step = self.window / 2.0;
                let mut t = sp.start;
                while t + self.window < sp.end {
                    windows.push((si, t, t + self.window));
                    t += step;
                }
                windows.push((si, (sp.end - self.window).max(sp.start), sp.end));
            }
        }
        let mut embs: Vec<Vec<f32>> = Vec::new();
        let mut kept: Vec<(usize, f64, f64)> = Vec::new();
        for &(si, a, b) in &windows {
            let s = (a * sr) as usize;
            let e = ((b * sr) as usize).min(samples.len());
            if let Some(v) = self.embed(&samples[s..e]) {
                embs.push(v);
                kept.push((si, a, b));
            }
        }
        let cut = match expected_speakers {
            Some(k) => crate::cluster::Cut::Clusters(k as usize),
            None => crate::cluster::Cut::Distance(self.threshold),
        };
        let labels = crate::cluster::cluster_speakers(&embs, cut);
        // Each speech stretch is split between its windows at the midpoints of their centres.
        let mut out: Vec<SpeakerSpan> = Vec::new();
        let mut push = |start: f64, end: f64, speaker: u32| {
            if end <= start {
                return;
            }
            match out.last_mut() {
                Some(l) if l.speaker == speaker && start - l.end < 0.05 => l.end = end,
                _ => out.push(SpeakerSpan { start, end, speaker }),
            }
        };
        let mut k = 0usize;
        for (si, sp) in spans.iter().enumerate() {
            let first = k;
            while k < kept.len() && kept[k].0 == si {
                k += 1;
            }
            let ws = &kept[first..k];
            if ws.is_empty() {
                // Too short to embed: the speaker of the nearest labelled window.
                let mid = (sp.start + sp.end) / 2.0;
                if let Some((i, _)) = kept.iter().enumerate().min_by(|a, b| {
                    let da = ((a.1 .1 + a.1 .2) / 2.0 - mid).abs();
                    let db = ((b.1 .1 + b.1 .2) / 2.0 - mid).abs();
                    da.total_cmp(&db)
                }) {
                    push(sp.start, sp.end, labels[i]);
                }
                continue;
            }
            let centres: Vec<f64> = ws.iter().map(|w| (w.1 + w.2) / 2.0).collect();
            let mut lab: Vec<u32> = (first..k).map(|i| labels[i]).collect();
            // A lone window between two of the same other label is taken as a slip, not a speaker change.
            for i in 1..lab.len().saturating_sub(1) {
                if lab[i - 1] == lab[i + 1] && lab[i] != lab[i - 1] {
                    lab[i] = lab[i - 1];
                }
            }
            // Between two windows with the same label the split point does not matter; where the label changes,
            // the change goes at the quietest moment between the two windows' centres, which is where one person
            // stops and the next starts far more often than the midpoint is.
            let cuts: Vec<f64> = (1..ws.len())
                .map(|i| if lab[i] != lab[i - 1] { quietest(samples, centres[i - 1], centres[i]) } else { (centres[i - 1] + centres[i]) / 2.0 })
                .collect();
            for i in 0..ws.len() {
                let a = if i == 0 { sp.start } else { cuts[i - 1] };
                let b = if i + 1 == ws.len() { sp.end } else { cuts[i] };
                push(a, b, lab[i]);
            }
        }
        Ok(out)
    }
}

/// The centre of the quietest 100 ms (by mean square) between `a` and `b` seconds, stepping by 20 ms.
fn quietest(samples: &[f32], a: f64, b: f64) -> f64 {
    let sr = SAMPLE_RATE as f64;
    let (frame, step) = ((0.1 * sr) as usize, (0.02 * sr) as usize);
    let (s, e) = ((a * sr) as usize, ((b * sr) as usize).min(samples.len()));
    if e <= s + frame {
        return (a + b) / 2.0;
    }
    let mut best = (f32::INFINITY, s);
    let mut i = s;
    while i + frame <= e {
        let energy = samples[i..i + frame].iter().map(|x| x * x).sum::<f32>();
        if energy < best.0 {
            best = (energy, i);
        }
        i += step;
    }
    (best.1 + frame / 2) as f64 / sr
}

pub struct PyannoteLabeller {
    config: so::OfflineSpeakerDiarizationConfig,
}

impl PyannoteLabeller {
    /// `threshold` is the clustering distance used when no speaker count is given (sherpa-onnx's default is 0.5;
    /// a smaller value finds more speakers).
    /// `segmentation` and `embedding` replace the light pack's models (phase 0 used them to try community-1's).
    pub fn new(model_dir: &Path, threads: i32, threshold: f32, segmentation: Option<&Path>, embedding: Option<&Path>) -> Result<Self> {
        let mut c = so::OfflineSpeakerDiarizationConfig::default();
        c.segmentation.pyannote.model = Some(match segmentation {
            Some(p) => file_str(p)?,
            None => path_str(&model_dir.join(SEGMENTATION_DIR), "model.onnx")?,
        });
        c.segmentation.num_threads = threads;
        c.embedding.model = Some(match embedding {
            Some(p) => file_str(p)?,
            None => path_str(model_dir, EMBEDDING_FILE)?,
        });
        c.embedding.num_threads = threads;
        c.clustering.threshold = threshold;
        Ok(Self { config: c })
    }
}

impl SpeakerLabeler for PyannoteLabeller {
    fn label(&self, samples: &[f32], expected_speakers: Option<u32>) -> Result<Vec<SpeakerSpan>> {
        let mut c = self.config.clone();
        c.clustering.num_clusters = expected_speakers.map(|n| n as i32).unwrap_or(-1);
        let d = so::OfflineSpeakerDiarization::create(&c).ok_or_else(|| SpeechError::LoadFailed { what: "speaker diarization".into() })?;
        let r = d.process(samples).ok_or_else(|| SpeechError::NoResult { what: "speaker diarization".into() })?;
        Ok(r.sort_by_start_time()
            .into_iter()
            .map(|s| SpeakerSpan { start: s.start as f64, end: s.end as f64, speaker: s.speaker.max(0) as u32 })
            .collect())
    }
}
