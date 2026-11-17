//! A project is one folder (T1):
//!
//! ```text
//! <folder>/project.prismtranscribe   the project file (JSON)
//! <folder>/transcripts/<id>.json     one transcript per media file
//! <folder>/media/                    copies of recordings, only when "Copy media into project" is chosen
//! <folder>/temp/                     16 kHz copies, job checkpoints; emptied when jobs end ("Clean up")
//! <folder>/cache/                    waveform peaks and playback copies; can always be rebuilt
//! ```
//!
//! By default the project links to the original recordings (open decision 6). The original is never modified; its
//! SHA-256 is stored at import and checked when the project opens.

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
    pub name: String,
    pub created: String,
    #[serde(default)]
    pub files: Vec<MediaFile>,
    #[serde(default = "default_notation")]
    pub notation: Vec<String>,
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
            file: ProjectFile { format: PROJECT_FORMAT.into(), schema_version: PROJECT_SCHEMA_VERSION, name: name.into(), created: now(), files: Vec::new(), notation: default_notation() },
        };
        p.save()?;
        Ok(p)
    }

    /// Open a project from its folder or its project file.
    pub fn open(path: &Path) -> Result<Self> {
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
        self.file.files.push(MediaFile { id: id.clone(), fields, source, copied: copy, sha256, bytes, duration: 0.0, has_video: false, imported: now(), status: MediaStatus::Imported, error: None });
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

    pub fn transcript_path(&self, id: &str) -> PathBuf {
        self.dir.join("transcripts").join(format!("{id}.json"))
    }

    pub fn load_transcript(&self, id: &str) -> Result<Option<Transcript>> {
        let p = self.transcript_path(id);
        if !p.exists() {
            return Ok(None);
        }
        Ok(Some(Transcript::from_json(&std::fs::read_to_string(p)?)?))
    }

    pub fn save_transcript(&self, t: &Transcript) -> Result<()> {
        write_atomic(&self.transcript_path(&t.media), t.to_json().as_bytes())?;
        Ok(())
    }

    /// Change the project on disk safely while other threads may be changing it too: open it fresh, apply `f`, save,
    /// all under one lock for the whole program. Long work (decoding, transcribing) happens outside, between two
    /// short updates, so an import finishing during a transcription is never overwritten.
    pub fn update<R>(dir: &Path, f: impl FnOnce(&mut Project) -> Result<R>) -> Result<R> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut p = Project::open(dir)?;
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
    fn other_json_is_not_a_project() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(PROJECT_FILE), r#"{"format":"something else"}"#).unwrap();
        assert!(matches!(Project::open(tmp.path()), Err(ProjectError::NotAProject { .. })));
    }
}
