//! The transcript data model: one JSON document per media file, `transcripts/<file id>.json` in the project.
//!
//! The `.prismtx` export (phase 5) is this model plus a manifest, so PrismQDA reads the structure the editor writes.
//! Times are seconds from the start of the recording. `schema_version` starts at 1 and is raised only together with
//! a reader for the version before it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

/// One recognized or typed word. `speaker` repeats the turn's speaker so a word can be moved on its own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
    /// 0..1, the recognizer's probability; 1.0 for words a person typed.
    pub confidence: f32,
    pub speaker: SpeakerId,
}

/// Speaker ids are stable across renames: "S1", "S2" ... in order of first appearance.
pub type SpeakerId = String;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Interviewer,
    Moderator,
    Participant,
    #[default]
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Speaker {
    pub id: SpeakerId,
    /// What the transcript shows until the user names the speaker: "Speaker 1".
    pub label: String,
    /// The name the user gave (a pseudonym, usually). Shown instead of the label when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Role,
    /// Case attributes for PrismQDA: pseudonym, age band, site ...
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

impl Speaker {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().filter(|n| !n.trim().is_empty()).unwrap_or(&self.label)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub speaker: SpeakerId,
    pub start: f64,
    pub end: f64,
    pub words: Vec<Word>,
}

impl Turn {
    pub fn text(&self) -> String {
        self.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
    }
}

/// A standard notation mark in the transcript: [inaudible 00:12:03], [crosstalk], [laughter] ... (T4; the editor
/// inserts them in phase 2). `turn` and `word` say where it sits: before word `word` of turn `turn`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotationTag {
    pub kind: String,
    pub time: f64,
    pub turn: usize,
    pub word: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A named copy of the turns ("after second listen"); version 0 is the recognizer's own output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub name: String,
    pub created: String,
    pub turns: Vec<Turn>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    #[default]
    NotStarted,
    FirstPass,
    Checked,
    DeIdentified,
    Final,
}

/// What made the transcript: which pack and models, when, and how long it took.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub engine: String,
    pub pack: String,
    pub created: String,
    #[serde(default)]
    pub seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_speakers: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub schema_version: u32,
    /// The media file's id in the project.
    pub media: String,
    pub speakers: Vec<Speaker>,
    pub turns: Vec<Turn>,
    #[serde(default)]
    pub notation: Vec<NotationTag>,
    /// Version 0, what the machine heard, then any named snapshots.
    #[serde(default)]
    pub snapshots: Vec<Snapshot>,
    #[serde(default)]
    pub review: ReviewStatus,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("this transcript was written by a newer Prism Transcribe (schema {0}); update the app to open it")]
    NewerSchema(u32),
    #[error("the transcript file is damaged: {0}")]
    Json(#[from] serde_json::Error),
}

impl Transcript {
    pub fn new(media: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            media: media.into(),
            speakers: Vec::new(),
            turns: Vec::new(),
            notation: Vec::new(),
            snapshots: Vec::new(),
            review: ReviewStatus::NotStarted,
            provenance: Provenance::default(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a transcript always serialises") + "\n"
    }

    pub fn from_json(text: &str) -> Result<Self, ModelError> {
        // Read the version first, so a newer file is refused with a clear message rather than a parse error.
        #[derive(Deserialize)]
        struct Head {
            schema_version: u32,
        }
        let head: Head = serde_json::from_str(text)?;
        if head.schema_version > SCHEMA_VERSION {
            return Err(ModelError::NewerSchema(head.schema_version));
        }
        Ok(serde_json::from_str(text)?)
    }

    pub fn speaker(&self, id: &str) -> Option<&Speaker> {
        self.speakers.iter().find(|s| s.id == id)
    }

    pub fn duration_covered(&self) -> f64 {
        self.turns.last().map(|t| t.end).unwrap_or(0.0)
    }
}

/// hh:mm:ss, the transcript's default time format (T5).
pub fn hms(t: f64) -> String {
    let s = t.max(0.0).floor() as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// hh:mm:ss.mmm, for captions.
pub fn hms_ms(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn sample() -> Transcript {
        let w = |t: &str, a: f64, b: f64, s: &str| Word { text: t.into(), start: a, end: b, confidence: 0.9, speaker: s.into() };
        let mut t = Transcript::new("F1");
        t.speakers = vec![
            Speaker { id: "S1".into(), label: "Speaker 1".into(), name: Some("Interviewer".into()), role: Role::Interviewer, attributes: BTreeMap::new() },
            Speaker { id: "S2".into(), label: "Speaker 2".into(), name: None, role: Role::Participant, attributes: [("site".to_string(), "North".to_string())].into() },
        ];
        t.turns = vec![
            Turn { speaker: "S1".into(), start: 0.5, end: 1.4, words: vec![w("Hello,", 0.5, 0.8, "S1"), w("Maria.", 0.9, 1.4, "S1")] },
            Turn { speaker: "S2".into(), start: 2.0, end: 3.1, words: vec![w("Hi", 2.0, 2.3, "S2"), w("there.", 2.4, 3.1, "S2")] },
        ];
        t.notation.push(NotationTag { kind: "laughter".into(), time: 3.2, turn: 1, word: 2, note: None });
        t.snapshots.push(Snapshot { name: "Version 0".into(), created: "2026-11-17T12:00:00Z".into(), turns: t.turns.clone() });
        t.provenance = Provenance { engine: "sherpa-onnx 2.0.1".into(), pack: "light-1".into(), created: "2026-11-17T12:00:00Z".into(), seconds: 12.5, expected_speakers: Some(2) };
        t
    }

    #[test]
    fn round_trips_through_json() {
        let t = sample();
        let back = Transcript::from_json(&t.to_json()).unwrap();
        assert_eq!(back, t);
        assert_eq!(back.speaker("S1").unwrap().display_name(), "Interviewer");
        assert_eq!(back.speaker("S2").unwrap().display_name(), "Speaker 2");
    }

    #[test]
    fn a_newer_schema_is_refused_clearly() {
        let text = sample().to_json().replace("\"schema_version\": 1", "\"schema_version\": 2");
        assert!(matches!(Transcript::from_json(&text), Err(ModelError::NewerSchema(2))));
    }

    #[test]
    fn times_format() {
        assert_eq!(hms(3725.9), "01:02:05");
        assert_eq!(hms_ms(3725.9), "01:02:05.900");
        assert_eq!(hms(-1.0), "00:00:00");
    }
}
