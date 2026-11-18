//! Ties the parts together for the app and the command line: import a recording into a project, transcribe it
//! (resumable), save the transcript, export it. Settings (Offline, model folder, FFmpeg) live here too.

use prism_transcribe_core::model::{Provenance, Role, Snapshot, Speaker, Transcript, Turn, Word};
use prism_transcribe_core::project::{now, ImportFields, MediaStatus, Project, ProjectError, TrackMode};
use prism_transcribe_media as media;
use prism_transcribe_speech::job::{self, JobOptions, MaskedVad, NoSpeakers};
use prism_transcribe_speech::models::{self, ModelPack, HEAVY_PACK, LIGHT_PACK};
use prism_transcribe_speech::sherpa::{Parakeet, PauseLabeller, SileroVad};
use prism_transcribe_speech::{install, pipeline, wav, whisper, SpeechError, SpeechRecognizer};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod deid;
pub mod spell;
pub mod statement;

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
    #[error("key file: {0}")]
    Key(String),
    #[error("{0}")]
    NotDeidentified(String),
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
    /// The whisper.cpp program for the heavy pack (`whisper-cli.exe`, or its folder); empty to look for one.
    #[serde(default)]
    pub whisper: Option<PathBuf>,
    /// Threads for recognition; empty for up to 8.
    #[serde(default)]
    pub threads: Option<i32>,
    /// The editor's keys, action name to key (T4: all remappable). Missing actions use [`default_hotkeys`].
    #[serde(default)]
    pub hotkeys: std::collections::BTreeMap<String, String>,
    /// Seconds played again when playback resumes after a pause (T4, default 1.5).
    #[serde(default)]
    pub rewind_on_resume: Option<f64>,
    /// Where each project's key file is (T6), by project id. Kept here rather than in the project folder, so the
    /// project folder does not point at the real names.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub key_files: std::collections::BTreeMap<String, PathBuf>,
}

/// The editor's default keys (T4): foot-pedal style F-keys, Ctrl+T for a timestamp, Alt+1 to Alt+9 for speakers. Key names are the browser's
/// `KeyboardEvent.key`, with modifiers first ("Control+t").
pub fn default_hotkeys() -> std::collections::BTreeMap<String, String> {
    [
        ("play_pause", "F4"),
        ("back", "F3"),
        ("forward", "F5"),
        ("slower", "F6"),
        ("faster", "F7"),
        ("timestamp", "Control+t"),
        ("notation", "Control+k"),
        ("mark_checked", "Control+m"),
        ("find", "Control+f"),
        ("snapshot", "Control+Shift+s"),
        ("next_turn", "Control+ArrowDown"),
        ("previous_turn", "Control+ArrowUp"),
        ("play_turn", "Control+Enter"),
        ("speakers", "Control+Shift+p"),
    ]
    .into_iter()
    .map(|(a, k)| (a.to_string(), k.to_string()))
    // Give the turn to speaker n (T3: "1 to 9 for the first nine speakers"). Alt, because a plain digit is typed
    // into the turn.
    .chain((1..=9).map(|n| (format!("speaker_{n}"), format!("Alt+{n}"))))
    .collect()
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

    /// Every action's key: the user's choice, else the default.
    pub fn effective_hotkeys(&self) -> std::collections::BTreeMap<String, String> {
        let mut k = default_hotkeys();
        for (a, key) in &self.hotkeys {
            if k.contains_key(a) && !key.trim().is_empty() {
                k.insert(a.clone(), key.clone());
            }
        }
        k
    }

    pub fn rewind(&self) -> f64 {
        self.rewind_on_resume.filter(|r| (0.0..=10.0).contains(r)).unwrap_or(1.5)
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
            if let Ok(pk) = media::peaks_from_wav(&wav16, PEAKS_PER_SECOND) {
                let p = Project::open(dir)?;
                std::fs::create_dir_all(p.cache_dir())?;
                std::fs::write(peaks_path(&p, &id), pk.iter().map(|x| *x as u8).collect::<Vec<u8>>())?;
            }
            Project::update(dir, |p| {
                let m = p.media_mut(&id)?;
                m.duration = info.duration;
                m.has_video = info.has_video;
                m.audio_tracks = info.audio_tracks as u32;
                m.channels = info.source_channels as u32;
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

/// Speaker ids S1, S2 ... in order of first appearance, labels "Speaker 1" ... With `labels`, speaker `k` is
/// `S{k+1}` with label `labels[k]` and every one is listed, spoken or not (one per track).
fn to_transcript(media_id: &str, turns: Vec<prism_transcribe_speech::pipeline::Turn>, labels: Option<&[String]>) -> Transcript {
    let mut order: Vec<u32> = Vec::new();
    match labels {
        Some(l) => order.extend(0..l.len() as u32),
        None => {
            for t in &turns {
                if !order.contains(&t.speaker) {
                    order.push(t.speaker);
                }
            }
        }
    }
    let sid = |s: u32| format!("S{}", order.iter().position(|x| *x == s).unwrap() + 1);
    let mut tr = Transcript::new(media_id);
    tr.speakers = (1..=order.len())
        .map(|n| Speaker {
            id: format!("S{n}"),
            label: labels.map(|l| l[n - 1].clone()).unwrap_or_else(|| format!("Speaker {n}")),
            name: None,
            role: Role::Other,
            attributes: Default::default(),
        })
        .collect();
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

/// Which pack a recording is transcribed with (T2): the file's choice, or the light pack unless the file's language
/// is one Parakeet does not cover, then the heavy pack.
pub fn choose_pack(fields: &ImportFields) -> &'static ModelPack {
    match fields.pack.as_deref() {
        Some(id) if id == HEAVY_PACK.id => &HEAVY_PACK,
        Some(_) => &LIGHT_PACK,
        None => match fields.language.as_deref() {
            Some(l) if !models::PARAKEET_LANGUAGES.contains(&l) => &HEAVY_PACK,
            _ => &LIGHT_PACK,
        },
    }
}

/// What is installed, for the model manager and before a job starts.
#[derive(Clone, Debug, Serialize)]
pub struct PackStatus {
    pub id: String,
    pub name: String,
    pub installed: bool,
    /// What is missing, in words, when not installed.
    pub message: String,
    pub download_mb: u64,
    /// The whisper.cpp program the heavy pack runs, if found (heavy pack only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
}

pub fn pack_status(settings: &Settings, pack: &ModelPack) -> PackStatus {
    let dir = settings.model_dir();
    let mut r = models::verify(pack, &dir, false).map_err(|e| e.to_string());
    let mut program = None;
    if pack.id == HEAVY_PACK.id {
        program = whisper::find_program(settings.whisper.as_deref(), &dir).map(|p| p.display().to_string());
        if r.is_ok() && program.is_none() {
            r = Err("the whisper.cpp program (whisper-cli.exe) was not found: set it in Settings".into());
        }
        if r.is_ok() {
            if let Err(e) = models::verify(&LIGHT_PACK, &dir, false) {
                r = Err(format!("the heavy pack also needs the light pack (for voice detection and speakers): {e}"));
            }
        }
    }
    let download_mb = install::downloads_for(pack.id).map(|(_, d)| d.iter().map(|x| x.bytes).sum::<u64>() / 1_000_000).unwrap_or(0);
    PackStatus { id: pack.id.into(), name: pack.name.into(), installed: r.is_ok(), message: r.err().unwrap_or_default(), download_mb, program }
}

/// The recognizer for a pack: Parakeet inside this program, or Whisper through whisper.cpp. Also returns the engine's
/// name for the provenance and the job options that suit it.
fn recognizer(pack: &ModelPack, settings: &Settings, model_dir: &Path, work: &Path) -> Result<(Box<dyn SpeechRecognizer>, String, usize, bool)> {
    let threads = settings.threads();
    if pack.id == HEAVY_PACK.id {
        let program = whisper::find_program(settings.whisper.as_deref(), model_dir).ok_or_else(|| EngineError::ModelsMissing("the whisper.cpp program (whisper-cli.exe) was not found; set it in Settings".into()))?;
        let w = whisper::WhisperCli::new(&program, &model_dir.join(models::WHISPER_MODEL), threads, work)?;
        Ok((Box::new(w), "whisper.cpp (whisper-cli) with sherpa-onnx 2.0.1".into(), 8, true))
    } else {
        Ok((Box::new(Parakeet::new(model_dir, threads)?), "sherpa-onnx 2.0.1".into(), 1, false))
    }
}

/// Transcribe one recording and save the transcript (raw output kept as version 0). The pack comes from
/// [`choose_pack`]; a file marked one-microphone-per-person is transcribed track by track (or channel by channel),
/// each a known speaker, with no diarization. The job resumes from its checkpoint after a cancel or a crash.
/// Temporary audio is deleted when the job ends.
pub fn transcribe(dir: &Path, id: &str, settings: &Settings, progress: &mut dyn FnMut(Progress) -> bool) -> Result<Transcript> {
    // A locked project could not save the result; say so before the long work, not after.
    if !Project::open(dir)?.is_unlocked() {
        return Err(ProjectError::Locked.into());
    }
    let model_dir = settings.model_dir();
    let m0 = Project::open(dir)?.media(id)?.clone();
    let pack = choose_pack(&m0.fields);
    let status = pack_status(settings, pack);
    if !status.installed {
        return Err(EngineError::ModelsMissing(if pack.id == HEAVY_PACK.id && m0.fields.pack.is_none() {
            format!("{} is in a language the light pack does not cover, so it needs the heavy pack: {}", m0.fields.label, status.message)
        } else {
            status.message
        }));
    }
    let started = std::time::Instant::now();
    let (m, src, wav16, checkpoint, temp) = Project::update(dir, |p| {
        let m = p.media(id)?.clone();
        let src = p.media_path(&m);
        p.media_mut(id)?.status = MediaStatus::Transcribing;
        Ok((m, src, wav16_path(p, id), p.temp_dir().join(format!("{id}.job.json")), p.temp_dir()))
    })?;
    let mut leftovers: Vec<PathBuf> = vec![wav16.clone()];
    let result = (|| -> Result<Transcript> {
        let threads = settings.threads();
        let vad = SileroVad::new(&model_dir, 0.2, 20.0)?;
        let (asr, engine_name, batch, drop) = recognizer(pack, settings, &model_dir, &temp)?;
        let mut opts = JobOptions { language: m.fields.language.clone(), expected_speakers: m.fields.expected_speakers, batch, drop_outside_speech: drop, ..Default::default() };
        if batch > 1 {
            // whisper.cpp: no Parakeet re-run, and chunks that fit one 30-second Whisper window with their padding
            // (a longer chunk spills a second or two into a second window, whose words Whisper can drop).
            opts.regap_seconds = f64::INFINITY;
            opts.chunk_seconds = 25.0;
        }
        let key = |extra: &str| format!("{}|{}|{:?}|{:?}|{}|{}|{extra}", m.sha256, pack.id, opts.expected_speakers, opts.language, opts.chunk_seconds, opts.regap_seconds);
        let mut t = if m.fields.tracks == TrackMode::Mixed {
            if !wav16.exists() {
                media::to_16k_wav(&src, &wav16, settings.ffmpeg().as_deref(), &mut |_| true)?;
            }
            let audio = wav::read_wav(&wav16)?;
            let labeller = PauseLabeller::new(&model_dir, threads, 0.6, 1.5, None)?;
            let out = job::run(&audio.samples, &vad, asr.as_ref(), &labeller, &opts, &checkpoint, &key("mixed"), progress)?;
            to_transcript(id, out.turns, None)
        } else {
            let (split, word) = if m.fields.tracks == TrackMode::Tracks { (media::Split::Tracks, "Track") } else { (media::Split::Channels, "Channel") };
            let part = |k: usize| temp.join(format!("{id}.{}{k}.16k.wav", word.to_lowercase()));
            let mut parts: Vec<PathBuf> = (0..).map(part).take_while(|p| p.exists()).collect();
            if parts.len() < 2 {
                parts = media::split_16k(&src, split, &part, settings.ffmpeg().as_deref(), &mut |_| true)?;
            }
            leftovers.extend(parts.iter().cloned());
            let audio: Vec<Vec<f32>> = parts.iter().map(|p| wav::read_wav(p).map(|a| a.samples)).collect::<std::result::Result<_, _>>()?;
            let slices: Vec<&[f32]> = audio.iter().map(|a| a.as_slice()).collect();
            let masks = pipeline::dominance_masks(&slices, 10.0);
            opts.label_speakers = false;
            let n = audio.len();
            let mut words = Vec::with_capacity(n);
            // Each microphone is silenced outside its own speaker's frames before anything listens to it, and any
            // word outside its own speech is dropped, so a voice heard through another microphone is not written twice.
            opts.drop_outside_speech = true;
            for (k, (samples, mask)) in audio.iter().zip(masks).enumerate() {
                let samples = &pipeline::apply_mask(samples, &mask);
                let mvad = MaskedVad { inner: &vad, mask };
                let cp = temp.join(format!("{id}.job.{}{k}.json", word.to_lowercase()));
                // One progress bar for all the tracks: each track is an equal share.
                let mut each = |p: Progress| match p {
                    Progress::Recognize { done, total, seconds } => progress(Progress::Recognize { done: k * total.max(1) + done, total: n * total.max(1), seconds }),
                    Progress::Finished => true,
                    other => progress(other),
                };
                let out = job::run(samples, &mvad, asr.as_ref(), &NoSpeakers, &opts, &cp, &key(&format!("{word}{k}")), &mut each)?;
                words.push(out.turns.into_iter().flat_map(|t| t.words).collect::<Vec<_>>());
            }
            // Utterances end at pauses of 0.3 s; one speaker's pieces join again unless someone else spoke between them.
            let turns = pipeline::turns_from_tracks(words, 0.3, 0.3);
            let labels: Vec<String> = (1..=n).map(|k| format!("{word} {k}")).collect();
            progress(Progress::Finished);
            to_transcript(id, turns, Some(&labels))
        };
        prism_transcribe_core::edit::refresh_notation(&mut t);
        let created = now();
        t.snapshots.push(Snapshot { name: "Version 0 (what the recognizer heard)".into(), created: created.clone(), turns: t.turns.clone() });
        t.provenance = Provenance { engine: engine_name, pack: pack.id.into(), created, seconds: started.elapsed().as_secs_f64(), expected_speakers: m.fields.expected_speakers };
        Ok(t)
    })();
    match result {
        Ok(t) => {
            Project::open(dir)?.save_transcript(&t)?;
            set_status(dir, id, MediaStatus::Transcribed, None)?;
            for p in leftovers {
                let _ = std::fs::remove_file(p);
            }
            Ok(t)
        }
        Err(EngineError::Speech(SpeechError::Cancelled)) => {
            // Keep the 16 kHz copies and the checkpoints: the next run carries on.
            set_status(dir, id, MediaStatus::Imported, None)?;
            Err(EngineError::Speech(SpeechError::Cancelled))
        }
        Err(e) => {
            set_status(dir, id, MediaStatus::Failed, Some(e.to_string()))?;
            Err(e)
        }
    }
}

/// Find the speakers again with another expected count (T3), keeping every word as it is now (edits included):
/// voice detection and speaker labelling run again on the recording, and each word goes to the speaker heard at
/// its time. Speakers come back as "Speaker 1" ... and turns are rebuilt. For a recording transcribed in one mix.
pub fn find_speakers_again(dir: &Path, id: &str, turns: &[Turn], expected: Option<u32>, settings: &Settings) -> Result<(Vec<Speaker>, Vec<Turn>)> {
    let model_dir = settings.model_dir();
    models::verify(&LIGHT_PACK, &model_dir, false).map_err(|e| EngineError::ModelsMissing(e.to_string()))?;
    let p = Project::open(dir)?;
    let m = p.media(id)?.clone();
    let wav16 = wav16_path(&p, id);
    let made_here = !wav16.exists();
    if made_here {
        media::to_16k_wav(&p.media_path(&m), &wav16, settings.ffmpeg().as_deref(), &mut |_| true)?;
    }
    let audio = wav::read_wav(&wav16);
    if made_here {
        let _ = std::fs::remove_file(&wav16);
    }
    let audio = audio?;
    let threads = settings.threads();
    let vad = SileroVad::new(&model_dir, 0.2, 20.0)?;
    let speech = prism_transcribe_speech::VoiceDetector::detect(&vad, &audio.samples)?;
    let labeller = PauseLabeller::new(&model_dir, threads, 0.6, 1.5, None)?;
    let spans = prism_transcribe_speech::SpeakerLabeler::label_speech(&labeller, &audio.samples, &speech, expected)?;
    let mut words: Vec<prism_transcribe_speech::Word> = turns.iter().flat_map(|t| &t.words).map(|w| prism_transcribe_speech::Word { text: w.text.clone(), start: w.start, end: w.end, confidence: w.confidence }).collect();
    words.sort_by(|a, b| a.start.total_cmp(&b.start));
    let labels = pipeline::speakers_for_words(&words, &spans);
    let t = to_transcript(id, pipeline::turns(words, &labels), None);
    Ok((t.speakers, t.turns))
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

/// Export, and when `deidentified` is asked for, refuse unless the file passes the final check against the key
/// file's name list (T6): no name on the list anywhere in what the export carries.
pub fn export_checked(p: &Project, id: &str, format: ExportFormat, out: &Path, opts: &prism_transcribe_core::export::ExportOptions, deidentified: bool) -> Result<()> {
    if deidentified {
        let c = deid::check_one(&p.dir, id)?;
        if !c.passes() {
            let mut list: Vec<String> = c.leftovers.iter().map(|l| format!("{} ({})", l.text, l.place)).collect();
            list.dedup();
            let more = if list.len() > 5 { format!(" and {} more", list.len() - 5) } else { String::new() };
            list.truncate(5);
            return Err(EngineError::NotDeidentified(format!(
                "{} still has names from the list: {}{more}. Replace them in De-identify, or export without the de-identified mark.",
                c.label,
                list.join(", ")
            )));
        }
    }
    export(p, id, format, out, opts)
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

// ---------------------------------------------------------------------------------------------------------
// The editor (phase 2)
// ---------------------------------------------------------------------------------------------------------

const PEAKS_PER_SECOND: u32 = 50;

fn peaks_path(p: &Project, id: &str) -> PathBuf {
    p.cache_dir().join(format!("{id}.peaks"))
}

/// The waveform for a recording, 50 (min, max) pairs a second. Cached in the project; made from the 16 kHz copy if
/// it is still in `temp`, or by decoding the recording again.
pub fn peaks(dir: &Path, id: &str, settings: &Settings) -> Result<Vec<i8>> {
    let p = Project::open(dir)?;
    let cached = peaks_path(&p, id);
    if let Ok(b) = std::fs::read(&cached) {
        return Ok(b.into_iter().map(|x| x as i8).collect());
    }
    let wav16 = wav16_path(&p, id);
    let made_here = !wav16.exists();
    if made_here {
        media::to_16k_wav(&p.media_path(p.media(id)?), &wav16, settings.ffmpeg().as_deref(), &mut |_| true)?;
    }
    let pk = media::peaks_from_wav(&wav16, PEAKS_PER_SECOND)?;
    if made_here {
        let _ = std::fs::remove_file(&wav16);
    }
    std::fs::create_dir_all(p.cache_dir())?;
    std::fs::write(&cached, pk.iter().map(|x| *x as u8).collect::<Vec<u8>>())?;
    Ok(pk)
}

pub fn peaks_per_second() -> u32 {
    PEAKS_PER_SECOND
}

/// The file the editor plays: the original when WebView2 can play it (T1: playback uses the original), otherwise a
/// WAV copy in `cache` made the first time. Returns the path and whether it is a copy.
pub fn playback_file(dir: &Path, id: &str, settings: &Settings) -> Result<(PathBuf, bool)> {
    let p = Project::open(dir)?;
    let m = p.media(id)?;
    let original = p.media_path(m);
    if media::playable(&original) {
        return Ok((original, false));
    }
    let copy = p.cache_dir().join(format!("{id}.play.wav"));
    if !copy.exists() {
        std::fs::create_dir_all(p.cache_dir())?;
        media::to_16k_wav(&original, &copy, settings.ffmpeg().as_deref(), &mut |_| true)?;
    }
    Ok((copy, true))
}

/// Save a transcript from the editor: the notation list is rebuilt from the bracketed words first.
pub fn save_transcript(dir: &Path, mut t: Transcript) -> Result<()> {
    prism_transcribe_core::edit::refresh_notation(&mut t);
    Project::open(dir)?.save_transcript(&t)?;
    Ok(())
}

/// Find and replace in every transcript of the project (T4), keeping word times. Returns (media id, count) for
/// each transcript that changed.
pub fn replace_in_project(dir: &Path, find: &str, replace: &str, opts: &prism_transcribe_core::edit::FindOptions) -> Result<Vec<(String, usize)>> {
    let p = Project::open(dir)?;
    let mut out = Vec::new();
    for m in &p.file.files {
        if let Some(mut t) = p.load_transcript(&m.id)? {
            let n = prism_transcribe_core::edit::replace_all(&mut t, find, replace, opts);
            if n > 0 {
                save_transcript(dir, t)?;
                out.push((m.id.clone(), n));
            }
        }
    }
    Ok(out)
}
