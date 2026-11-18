//! A project is one folder (T1):
//!
//! ```text
//! <folder>/project.prismtranscribe   the project file (JSON)
//! <folder>/transcripts/<id>.json     one transcript per media file (<id>.locked when password protected)
//! <folder>/media/                    copies of recordings, only when "Copy media into project" is chosen
//! <folder>/temp/                     16 kHz copies, job checkpoints; emptied when jobs end ("Clean up")
//! <folder>/cache/                    waveform peaks and playback copies; can always be rebuilt
//! ```
//!
//! By default the project links to the original recordings (open decision 6). The original is never modified; its
//! SHA-256 is stored at import and checked when the project opens.

use crate::crypt::{self, CryptError, Lock};
use crate::model::{ModelError, Transcript};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = "project.prismtranscribe";
pub const PROJECT_FORMAT: &str = "Prism Transcribe project";
pub const PROJECT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{path} is not a Prism Transcribe project file")]
    NotAProject { path: String },
    #[error("this project was written by a newer Prism Transcribe (schema {0}); update the app to open it")]
    NewerSchema(u32),
    #[error("the project file is damaged: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Transcript(#[from] ModelError),
    #[error("there is no media file with id {0} in this project")]
    NoSuchMedia(String),
    #[error("a project already exists in {0}")]
    Exists(String),
    #[error("this project is password protected; unlock it with its password first")]
    Locked,
    #[error("{0}")]
    Crypt(#[from] CryptError),
}

pub type Result<T> = std::result::Result<T, ProjectError>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaStatus {
    #[default]
    Imported,
    Transcribing,
    Transcribed,
    Failed,
}

/// The optional fields asked at import (T1), plus what the import measured.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportFields {
    /// A short label such as INT-07.
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interviewer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_speakers: Option<u32>,
    /// ISO 639-1, or None for automatic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The Advanced panel (T2): which model pack to use, `light-1` or `heavy-1`; `None` picks the light pack, or the
    /// heavy one when the language is outside the light pack's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    /// One microphone per person (T1): transcribe track by track, or channel by channel, each a known speaker.
    #[serde(default, skip_serializing_if = "TrackMode::is_mixed")]
    pub tracks: TrackMode,
}

/// How the people in a recording are separated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackMode {
    /// Everyone in one mix: speakers are found by diarization.
    #[default]
    Mixed,
    /// One audio track per person.
    Tracks,
    /// One channel per person (a two-microphone recorder's left and right, a multichannel WAV).
    Channels,
}

impl TrackMode {
    pub fn is_mixed(&self) -> bool {
        *self == TrackMode::Mixed
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaFile {
    pub id: String,
    #[serde(flatten)]
    pub fields: ImportFields,
    /// Where the recording is: absolute for a link, relative to the project folder for a copy.
    pub source: String,
    pub copied: bool,
    pub sha256: String,
    pub bytes: u64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub has_video: bool,
    /// How many audio tracks the file has, and the first track's channels (0 when not known), so the Advanced panel
    /// can offer to transcribe one microphone at a time.
    #[serde(default)]
    pub audio_tracks: u32,
    #[serde(default)]
    pub channels: u32,
    pub imported: String,
    #[serde(default)]
    pub status: MediaStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The notation menu's entries when a project starts (T4); the list is editable per project. `hh:mm:ss` is
/// replaced by the playback time when inserted, and `…` marks where the cursor goes.
pub fn default_notation() -> Vec<String> {
    ["[inaudible hh:mm:ss]", "[unclear: …]", "[crosstalk]", "[laughter]", "[pause]", "[redacted]"].map(String::from).to_vec()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub format: String,
    pub schema_version: u32,
    /// A random id, so a key file (T6) can tell which project it belongs to. Given on create, or on the first open
    /// of a project made before phase 4.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub created: String,
    #[serde(default)]
    pub files: Vec<MediaFile>,
    #[serde(default = "default_notation")]
    pub notation: Vec<String>,
    /// The project word list (T4, T6): pseudonyms, field terms and words the suggestions pass was told are not
    /// names. Never suggested as names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub word_list: Vec<String>,
    /// Set when the transcripts are password protected (open decision 7); see [`crate::crypt`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<Lock>,
    /// The batch queue (T8).
    #[serde(default, skip_serializing_if = "crate::queue::Queue::is_unused")]
    pub queue: crate::queue::Queue,
}

/// A new random project id (hex).
pub fn new_id() -> String {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).expect("the operating system's random number source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// What opening a project found about one recording.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MediaCheck {
    Unchanged,
    /// The file is there but its SHA-256 differs from the one stored at import.
    Changed { sha256: String },
    Missing,
}

pub struct Project {
    pub dir: PathBuf,
    pub file: ProjectFile,
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Today's date on this computer's clock, for documents a person reads (YYYY-MM-DD).
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Write a file so that a crash leaves either the old or the new contents, never half of one.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("{}.saving", path.extension().and_then(|e| e.to_str()).unwrap_or("tmp")));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

fn remove_if_there(p: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(p) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

pub fn sha256_file(path: &Path) -> std::io::Result<(String, u64)> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut n_total = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        n_total += n as u64;
        h.update(&buf[..n]);
    }
    Ok((h.finalize().iter().map(|b| format!("{b:02x}")).collect(), n_total))
}

impl Project {
    pub fn create(dir: &Path, name: &str) -> Result<Self> {
        if dir.join(PROJECT_FILE).exists() {
            return Err(ProjectError::Exists(dir.display().to_string()));
        }
        std::fs::create_dir_all(dir.join("transcripts"))?;
        std::fs::create_dir_all(dir.join("temp"))?;
        let p = Project {
            dir: dir.to_path_buf(),
            file: ProjectFile {
                format: PROJECT_FORMAT.into(),
                schema_version: PROJECT_SCHEMA_VERSION,
                id: new_id(),
                name: name.into(),
                created: now(),
                files: Vec::new(),
                notation: default_notation(),
                word_list: Vec::new(),
                lock: None,
                queue: Default::default(),
            },
        };
        p.save()?;
        Ok(p)
    }

    /// Open a project from its folder or its project file.
    pub fn open(path: &Path) -> Result<Self> {
        let p = Project::open_file(path)?;
        if p.file.id.is_empty() {
            // A project from before phase 4: give it its id once, under the lock that guards the project file.
            return Project::update(&p.dir, |q| {
                if q.file.id.is_empty() {
                    q.file.id = new_id();
                }
                Ok(Project { dir: q.dir.clone(), file: q.file.clone() })
            });
        }
        Ok(p)
    }

    fn open_file(path: &Path) -> Result<Self> {
        let (dir, file) = if path.is_dir() { (path.to_path_buf(), path.join(PROJECT_FILE)) } else { (path.parent().unwrap_or(Path::new(".")).to_path_buf(), path.to_path_buf()) };
        let text = std::fs::read_to_string(&file)?;
        let v: serde_json::Value = serde_json::from_str(&text)?;
        if v.get("format").and_then(|f| f.as_str()) != Some(PROJECT_FORMAT) {
            return Err(ProjectError::NotAProject { path: file.display().to_string() });
        }
        let version = v.get("schema_version").and_then(|n| n.as_u64()).unwrap_or(0) as u32;
        if version > PROJECT_SCHEMA_VERSION {
            return Err(ProjectError::NewerSchema(version));
        }
        let pf: ProjectFile = serde_json::from_value(v)?;
        std::fs::create_dir_all(dir.join("transcripts"))?;
        std::fs::create_dir_all(dir.join("temp"))?;
        Ok(Project { dir, file: pf })
    }

    pub fn save(&self) -> Result<()> {
        write_atomic(&self.dir.join(PROJECT_FILE), (serde_json::to_string_pretty(&self.file)? + "\n").as_bytes())?;
        Ok(())
    }

    pub fn temp_dir(&self) -> PathBuf {
        self.dir.join("temp")
    }

    /// Things kept between sessions that can always be rebuilt: waveform peaks, playback copies.
    pub fn cache_dir(&self) -> PathBuf {
        self.dir.join("cache")
    }

    pub fn media(&self, id: &str) -> Result<&MediaFile> {
        self.file.files.iter().find(|m| m.id == id).ok_or_else(|| ProjectError::NoSuchMedia(id.into()))
    }

    pub fn media_mut(&mut self, id: &str) -> Result<&mut MediaFile> {
        self.file.files.iter_mut().find(|m| m.id == id).ok_or_else(|| ProjectError::NoSuchMedia(id.into()))
    }

    /// The recording's path on disk.
    pub fn media_path(&self, m: &MediaFile) -> PathBuf {
        let p = PathBuf::from(&m.source);
        if p.is_absolute() {
            p
        } else {
            self.dir.join(p)
        }
    }

    fn next_id(&self) -> String {
        let n = self.file.files.iter().filter_map(|m| m.id.strip_prefix('F')?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
        format!("F{n:03}")
    }

    /// Add a recording: hash it (and copy it into `media/` when `copy` is set). Decoding and the 16 kHz copy are
    /// the media crate's job; the caller fills `duration` and `has_video` afterwards.
    pub fn add_media(&mut self, original: &Path, mut fields: ImportFields, copy: bool) -> Result<String> {
        let original = std::fs::canonicalize(original)?;
        let (sha256, bytes) = sha256_file(&original)?;
        let id = self.next_id();
        let source = if copy {
            let dir = self.dir.join("media");
            std::fs::create_dir_all(&dir)?;
            let name = format!("{id}-{}", original.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            std::fs::copy(&original, dir.join(&name))?;
            format!("media/{name}")
        } else {
            original.to_string_lossy().trim_start_matches(r"\\?\").to_string()
        };
        if fields.label.trim().is_empty() {
            fields.label = original.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| id.clone());
        }
        self.file.files.push(MediaFile { id: id.clone(), fields, source, copied: copy, sha256, bytes, duration: 0.0, has_video: false, audio_tracks: 0, channels: 0, imported: now(), status: MediaStatus::Imported, error: None });
        self.save()?;
        Ok(id)
    }

    /// Compare every recording with the SHA-256 stored at import (T1: "a file whose checksum changes after import
    /// shows a warning on open").
    pub fn check_media(&self) -> Vec<(String, MediaCheck)> {
        self.file
            .files
            .iter()
            .map(|m| {
                let p = self.media_path(m);
                let state = match sha256_file(&p) {
                    Err(_) => MediaCheck::Missing,
                    Ok((h, _)) if h == m.sha256 => MediaCheck::Unchanged,
                    Ok((h, _)) => MediaCheck::Changed { sha256: h },
                };
                (m.id.clone(), state)
            })
            .collect()
    }

    /// The plain transcript file.
    pub fn transcript_path(&self, id: &str) -> PathBuf {
        self.dir.join("transcripts").join(format!("{id}.json"))
    }

    /// The sealed transcript file, when the project is password protected.
    pub fn locked_path(&self, id: &str) -> PathBuf {
        self.dir.join("transcripts").join(format!("{id}.locked"))
    }

    pub fn is_protected(&self) -> bool {
        self.file.lock.is_some()
    }

    /// Whether the transcripts can be read now: not protected, or unlocked in this session.
    pub fn is_unlocked(&self) -> bool {
        !self.is_protected() || crypt::key_for(&self.dir).is_some()
    }

    /// Read bytes that may be sealed, opening them with the project's key.
    pub fn read_maybe_sealed(&self, path: &Path) -> Result<Vec<u8>> {
        let data = std::fs::read(path)?;
        if !crypt::is_sealed(&data) {
            return Ok(data);
        }
        let key = crypt::key_for(&self.dir).ok_or(ProjectError::Locked)?;
        Ok(crypt::unseal(&key, &data)?)
    }

    /// Bytes as the project stores them: sealed when it is password protected.
    pub fn seal_if_protected(&self, data: &[u8]) -> Result<Vec<u8>> {
        if !self.is_protected() {
            return Ok(data.to_vec());
        }
        let key = crypt::key_for(&self.dir).ok_or(ProjectError::Locked)?;
        Ok(crypt::seal(&key, data))
    }

    pub fn load_transcript(&self, id: &str) -> Result<Option<Transcript>> {
        // Turning protection on or off can be cut short by a crash and leave both files; the one in the project's
        // current mode was written last.
        let (sealed, plain) = (self.locked_path(id), self.transcript_path(id));
        let p = match (self.is_protected(), sealed.exists(), plain.exists()) {
            (_, false, false) => return Ok(None),
            (true, true, _) | (false, true, false) => sealed,
            _ => plain,
        };
        let bytes = self.read_maybe_sealed(&p)?;
        Ok(Some(Transcript::from_json(&String::from_utf8_lossy(&bytes))?))
    }

    pub fn save_transcript(&self, t: &Transcript) -> Result<()> {
        let json = t.to_json();
        if self.is_protected() {
            write_atomic(&self.locked_path(&t.media), &self.seal_if_protected(json.as_bytes())?)?;
            remove_if_there(&self.transcript_path(&t.media))?;
        } else {
            write_atomic(&self.transcript_path(&t.media), json.as_bytes())?;
            remove_if_there(&self.locked_path(&t.media))?;
        }
        Ok(())
    }

    /// Unlock a password-protected project for this session.
    pub fn unlock(&self, password: &str) -> Result<()> {
        let lock = self.file.lock.as_ref().ok_or(ProjectError::Locked)?;
        crypt::remember(&self.dir, crypt::open_lock(lock, password)?);
        Ok(())
    }

    /// Turn password protection on (`Some`), change the password, or turn it off (`None`). Every transcript is
    /// read with the old key and written again with the new one, or in plain text. The project must be unlocked.
    pub fn set_password(dir: &Path, password: Option<&str>) -> Result<()> {
        Project::update(dir, |p| {
            if !p.is_unlocked() {
                return Err(ProjectError::Locked);
            }
            let ids: Vec<String> = p.file.files.iter().map(|m| m.id.clone()).collect();
            let all: Vec<Transcript> = ids.iter().filter_map(|id| p.load_transcript(id).transpose()).collect::<Result<_>>()?;
            match password {
                Some(pw) => {
                    let (lock, key) = crypt::new_lock(pw)?;
                    p.file.lock = Some(lock);
                    crypt::remember(&p.dir, key);
                }
                None => {
                    p.file.lock = None;
                    crypt::forget(&p.dir);
                }
            }
            // The project file first, so a crash part way leaves files the project can still read.
            p.save()?;
            for t in &all {
                p.save_transcript(t)?;
            }
            Ok(())
        })
    }

    /// Change the project on disk safely while other threads may be changing it too: open it fresh, apply `f`, save,
    /// all under one lock for the whole program. Long work (decoding, transcribing) happens outside, between two
    /// short updates, so an import finishing during a transcription is never overwritten.
    pub fn update<R>(dir: &Path, f: impl FnOnce(&mut Project) -> Result<R>) -> Result<R> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut p = Project::open_file(dir)?;
        let r = f(&mut p)?;
        p.save()?;
        Ok(r)
    }

    /// Remove temporary audio and job files left behind (the "Clean up" button). Returns how many were removed.
    pub fn clean_up(&self) -> Result<usize> {
        let mut n = 0;
        if let Ok(rd) = std::fs::read_dir(self.temp_dir()) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    std::fs::remove_dir_all(&p)?;
                } else {
                    std::fs::remove_file(&p)?;
                }
                n += 1;
            }
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_import_check_and_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let rec = tmp.path().join("interview one.wav");
        std::fs::write(&rec, b"RIFF....pretend audio").unwrap();
        let dir = tmp.path().join("proj");
        let mut p = Project::create(&dir, "Study A").unwrap();
        assert!(matches!(Project::create(&dir, "again"), Err(ProjectError::Exists(_))));
        let id = p.add_media(&rec, ImportFields { expected_speakers: Some(2), ..Default::default() }, false).unwrap();
        assert_eq!(id, "F001");
        let id2 = p.add_media(&rec, ImportFields { label: "INT-02".into(), ..Default::default() }, true).unwrap();
        assert_eq!(id2, "F002");
        assert!(dir.join("media").join("F002-interview one.wav").exists());

        let p = Project::open(&dir).unwrap();
        assert_eq!(p.file.name, "Study A");
        assert_eq!(p.media("F001").unwrap().fields.label, "interview one", "label defaults to the file name");
        assert_eq!(p.media("F001").unwrap().fields.expected_speakers, Some(2));
        assert!(p.check_media().iter().all(|(_, c)| *c == MediaCheck::Unchanged));

        std::fs::write(&rec, b"RIFF....changed audio").unwrap();
        let checks = p.check_media();
        assert!(matches!(checks[0].1, MediaCheck::Changed { .. }), "the linked original changed");
        assert_eq!(checks[1].1, MediaCheck::Unchanged, "the copy did not");
        std::fs::remove_file(&rec).unwrap();
        assert_eq!(p.check_media()[0].1, MediaCheck::Missing);
    }

    #[test]
    fn transcripts_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Project::create(tmp.path(), "x").unwrap();
        assert!(p.load_transcript("F001").unwrap().is_none());
        let t = crate::model::tests::sample();
        p.save_transcript(&t).unwrap();
        assert_eq!(p.load_transcript("F1").unwrap().unwrap(), t);
        std::fs::write(p.temp_dir().join("chunk.wav"), b"x").unwrap();
        assert_eq!(p.clean_up().unwrap(), 1);
    }

    #[test]
    fn password_protection_on_change_and_off() {
        let tmp = tempfile::tempdir().unwrap();
        let mut p = Project::create(tmp.path(), "x").unwrap();
        assert_eq!(p.file.id.len(), 16);
        let rec = tmp.path().join("a.wav");
        std::fs::write(&rec, b"RIFF").unwrap();
        let id = p.add_media(&rec, ImportFields::default(), false).unwrap();
        let mut t = crate::model::tests::sample();
        t.media = id.clone();
        p.save_transcript(&t).unwrap();

        Project::set_password(tmp.path(), Some("open sesame")).unwrap();
        let p = Project::open(tmp.path()).unwrap();
        assert!(p.is_protected() && p.is_unlocked());
        assert!(!p.transcript_path(&id).exists(), "the plain copy is gone");
        let raw = std::fs::read(p.locked_path(&id)).unwrap();
        assert!(crate::crypt::is_sealed(&raw) && !raw.windows(5).any(|w| w == b"Maria"));
        assert_eq!(p.load_transcript(&id).unwrap().unwrap(), t);

        // Locked: nothing can be read or written; a wrong password is refused; the right one opens it.
        crate::crypt::forget(tmp.path());
        assert!(matches!(p.load_transcript(&id), Err(ProjectError::Locked)));
        assert!(matches!(p.save_transcript(&t), Err(ProjectError::Locked)));
        assert!(matches!(Project::set_password(tmp.path(), None), Err(ProjectError::Locked)));
        assert!(matches!(p.unlock("open says me"), Err(ProjectError::Crypt(CryptError::WrongPassword))));
        p.unlock("open sesame").unwrap();
        assert_eq!(p.load_transcript(&id).unwrap().unwrap(), t);

        Project::set_password(tmp.path(), Some("new one")).unwrap();
        crate::crypt::forget(tmp.path());
        let p = Project::open(tmp.path()).unwrap();
        assert!(p.unlock("open sesame").is_err());
        p.unlock("new one").unwrap();
        Project::set_password(tmp.path(), None).unwrap();
        let p = Project::open(tmp.path()).unwrap();
        assert!(!p.is_protected() && !p.locked_path(&id).exists());
        assert_eq!(p.load_transcript(&id).unwrap().unwrap(), t);
    }

    #[test]
    fn an_old_project_gets_an_id() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(PROJECT_FILE), r#"{"format":"Prism Transcribe project","schema_version":1,"name":"old","created":"2026-11-01T00:00:00Z"}"#).unwrap();
        let p = Project::open(tmp.path()).unwrap();
        assert_eq!(p.file.id.len(), 16);
        assert_eq!(Project::open(tmp.path()).unwrap().file.id, p.file.id, "kept");
    }

    #[test]
    fn other_json_is_not_a_project() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(PROJECT_FILE), r#"{"format":"something else"}"#).unwrap();
        assert!(matches!(Project::open(tmp.path()), Err(ProjectError::NotAProject { .. })));
    }
}
