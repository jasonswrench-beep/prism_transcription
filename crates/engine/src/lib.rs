//! Ties the parts together for the app and the command line: import a recording into a project, transcribe it
//! (resumable), save the transcript, export it. Settings (Offline, model folder, FFmpeg) live here too.

use prism_transcribe_core::model::{Provenance, Role, Snapshot, Speaker, Transcript, Turn, Word};
use prism_transcribe_core::project::{now, ImportFields, MediaStatus, Project, ProjectError};
use prism_transcribe_media as media;
use prism_transcribe_speech::job::{self, JobOptions};
use prism_transcribe_speech::models::{self, LIGHT_PACK};
use prism_transcribe_speech::sherpa::{Parakeet, PauseLabeller, SileroVad};
use prism_transcribe_speech::{wav, SpeechError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub use job::Progress;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("{0}")]
    Project(#[from] ProjectError),
    #[error("{0}")]
    Media(#[from] media::MediaError),
    #[error("{0}")]
    Speech(#[from] SpeechError),
    #[error("{0}")]
    Install(#[from] prism_transcribe_speech::install::InstallError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("the speech models are not installed yet: {0}")]
    ModelsMissing(String),
    #[error("{0} has not been transcribed yet")]
    NoTranscript(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// The app's settings, in `%APPDATA%\Prism Transcribe\settings.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// The Offline switch: no network call of any kind (model downloads are the only one the app makes).
    #[serde(default)]
    pub offline: bool,
    /// The shared speech-model folder; empty for the default.
    #[serde(default)]
    pub model_dir: Option<PathBuf>,
    /// An FFmpeg program to use for video and unusual audio; empty to look for one.
    #[serde(default)]
    pub ffmpeg: Option<PathBuf>,
    /// Threads for recognition; empty for up to 8.
    #[serde(default)]
    pub threads: Option<i32>,
}

impl Settings {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("Prism Transcribe").join("settings.json")
    }

    pub fn load() -> Settings {
        std::fs::read_to_string(Self::path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let p = Self::path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        prism_transcribe_core::project::write_atomic(&p, (serde_json::to_string_pretty(self).unwrap() + "\n").as_bytes())?;
        Ok(())
    }

    pub fn model_dir(&self) -> PathBuf {
        self.model_dir.clone().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(models::default_model_dir)
    }

    pub fn ffmpeg(&self) -> Option<PathBuf> {
        self.ffmpeg.clone().filter(|p| p.is_file()).or_else(media::find_ffmpeg)
    }

    pub fn threads(&self) -> i32 {
        self.threads.filter(|t| *t > 0).unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get().min(8) as i32).unwrap_or(4))
    }
}

fn wav16_path(p: &Project, id: &str) -> PathBuf {
    p.temp_dir().join(format!("{id}.16k.wav"))
}

/// Import a recording: hash it, link or copy it, make the 16 kHz copy and measure it. On a decoding failure the file
/// stays in the project marked Failed, with the reason, so the user sees what went wrong.
pub fn import(dir: &Path, file: &Path, fields: ImportFields, copy: bool, ffmpeg: Option<&Path>, progress: &mut dyn FnMut(f64) -> bool) -> Result<String> {
    let (id, src, wav16) = Project::update(dir, |p| {
        let id = p.add_media(file, fields, copy)?;
        let src = p.media_path(p.media(&id)?);
        Ok((id.clone(), src, wav16_path(p, &id)))
    })?;
    match media::to_16k_wav(&src, &wav16, ffmpeg, progress) {
        Ok(info) => {
            Project::update(dir, |p| {
                let m = p.media_mut(&id)?;
                m.duration = info.duration;
                m.has_video = info.has_video;
                Ok(())
            })?;
            Ok(id)
        }
        Err(e) => {
            set_status(dir, &id, MediaStatus::Failed, Some(e.to_string()))?;
            Err(e.into())
        }
    }
}

fn set_status(dir: &Path, id: &str, status: MediaStatus, error: Option<String>) -> Result<()> {
    Project::update(dir, |p| {
        let m = p.media_mut(id)?;
        m.status = status;
        m.error = error;
        Ok(())
    })?;
    Ok(())
}

/// Speaker ids S1, S2 ... in order of first appearance, labels "Speaker 1" ...
fn to_transcript(media_id: &str, turns: Vec<prism_transcribe_speech::pipeline::Turn>) -> Transcript {
    let mut order: Vec<u32> = Vec::new();
    for t in &turns {
        if !order.contains(&t.speaker) {
            order.push(t.speaker);
        }
    }
    let sid = |s: u32| format!("S{}", order.iter().position(|x| *x == s).unwrap() + 1);
    let mut tr = Transcript::new(media_id);
    tr.speakers = (1..=order.len()).map(|n| Speaker { id: format!("S{n}"), label: format!("Speaker {n}"), name: None, role: Role::Other, attributes: Default::default() }).collect();
    tr.turns = turns
        .into_iter()
        .map(|t| {
            let id = sid(t.speaker);
            Turn {
                speaker: id.clone(),
                start: t.start,
                end: t.end,
                words: t.words.into_iter().map(|w| Word { text: w.text, start: w.start, end: w.end, confidence: w.confidence, speaker: id.clone() }).collect(),
            }
        })
        .collect();
    tr
}

/// Transcribe one recording with the light pack and save the transcript (raw output kept as version 0). The job
/// resumes from its checkpoint after a cancel or a crash. Temporary audio is deleted when the job ends.
pub fn transcribe(dir: &Path, id: &str, settings: &Settings, progress: &mut dyn FnMut(Progress) -> bool) -> Result<Transcript> {
    let model_dir = settings.model_dir();
    models::verify(&LIGHT_PACK, &model_dir, false).map_err(|e| EngineError::ModelsMissing(e.to_string()))?;
    let started = std::time::Instant::now();
    let (m, src, wav16, checkpoint) = Project::update(dir, |p| {
        let m = p.media(id)?.clone();
        let src = p.media_path(&m);
        p.media_mut(id)?.status = MediaStatus::Transcribing;
        Ok((m, src, wav16_path(p, id), p.temp_dir().join(format!("{id}.job.json"))))
    })?;
    let result = (|| -> Result<Transcript> {
        if !wav16.exists() {
            media::to_16k_wav(&src, &wav16, settings.ffmpeg().as_deref(), &mut |_| true)?;
        }
        let audio = wav::read_wav(&wav16)?;
        let threads = settings.threads();
        let vad = SileroVad::new(&model_dir, 0.2, 20.0)?;
        let asr = Parakeet::new(&model_dir, threads)?;
        let labeller = PauseLabeller::new(&model_dir, threads, 0.6, 1.5, None)?;
        let opts = JobOptions { language: m.fields.language.clone(), expected_speakers: m.fields.expected_speakers, ..Default::default() };
        let key = format!("{}|{}|{:?}|{:?}|{}|{}", m.sha256, LIGHT_PACK.id, opts.expected_speakers, opts.language, opts.chunk_seconds, opts.regap_seconds);
        let out = job::run(&audio.samples, &vad, &asr, &labeller, &opts, &checkpoint, &key, progress)?;
        let mut t = to_transcript(id, out.turns);
        let created = now();
        t.snapshots.push(Snapshot { name: "Version 0 (what the recognizer heard)".into(), created: created.clone(), turns: t.turns.clone() });
        t.provenance = Provenance { engine: "sherpa-onnx 2.0.1".into(), pack: LIGHT_PACK.id.into(), created, seconds: started.elapsed().as_secs_f64(), expected_speakers: m.fields.expected_speakers };
        Ok(t)
    })();
    match result {
        Ok(t) => {
            Project::open(dir)?.save_transcript(&t)?;
            set_status(dir, id, MediaStatus::Transcribed, None)?;
            let _ = std::fs::remove_file(&wav16);
            Ok(t)
        }
        Err(EngineError::Speech(SpeechError::Cancelled)) => {
            // Keep the 16 kHz copy and the checkpoint: the next run carries on.
            set_status(dir, id, MediaStatus::Imported, None)?;
            Err(EngineError::Speech(SpeechError::Cancelled))
        }
        Err(e) => {
            set_status(dir, id, MediaStatus::Failed, Some(e.to_string()))?;
            Err(e)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Text,
    Word,
}

impl ExportFormat {
    pub fn from_path(p: &Path) -> Option<Self> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "txt" => Some(Self::Text),
            "docx" => Some(Self::Word),
            _ => None,
        }
    }
}

pub fn export(p: &Project, id: &str, format: ExportFormat, out: &Path, opts: &prism_transcribe_core::export::ExportOptions) -> Result<()> {
    use prism_transcribe_core::export;
    let m = p.media(id)?;
    let t = p.load_transcript(id)?.ok_or_else(|| EngineError::NoTranscript(m.fields.label.clone()))?;
    let date = m.fields.date.as_deref();
    match format {
        ExportFormat::Text => std::fs::write(out, export::to_text(&t, &m.fields.label, date, opts))?,
        ExportFormat::Word => std::fs::write(out, export::to_docx(&t, &m.fields.label, date, &m.fields.label, opts)?)?,
    }
    Ok(())
}
