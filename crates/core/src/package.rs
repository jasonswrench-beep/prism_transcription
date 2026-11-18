//! The two hand-off packages of T7: the Prism transcript package (.prismtx) for PrismQDA, and a REFI-QDA project
//! (.qdpx) for PrismQDA today and other QDA programs.
//!
//! **.prismtx** (version 1, specified in docs/PRISMTX.md) is a zip:
//!
//! ```text
//! manifest.json                 format "prism-transcribe-package", version "1.0", producer, created, documents
//! <id>/transcript.json          the transcript model the editor writes (schema 1), without its snapshots
//! <id>/document.json            label, date, language, the media reference, and the speakers as cases
//! <id>/transcript.vtt           the same turns as WebVTT with voice tags, for readers that know only captions
//! ```
//!
//! Snapshots are left out because they keep the words from before de-identification. The media is referenced, not
//! copied: its absolute path, size, duration, SHA-256, and PrismQDA's own fingerprint (SHA-256 of the size as 8
//! little-endian bytes, the first MiB and the last MiB), so the recording can be found again if it moves.
//!
//! **.qdpx** follows REFI-QDA 1.0: `project.qde` with one `AudioSource` or `VideoSource` per recording, its
//! `Transcript` (plain text in `sources/`, a `SyncPoint` at the start and end of every turn, a `TranscriptSelection`
//! per turn), and one `Case` per speaker holding that speaker's turns, with role and attributes as `Variables`.

use crate::model::{Transcript, Turn};
use crate::project::MediaFile;
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const PRISMTX_FORMAT: &str = "prism-transcribe-package";
pub const PRISMTX_VERSION: &str = "1.0";

/// One recording to package: its project entry, its transcript, and where the recording is on disk.
pub struct Doc<'a> {
    pub media: &'a MediaFile,
    pub transcript: &'a Transcript,
    pub path: PathBuf,
}

/// PrismQDA's media fingerprint: SHA-256 of the file size (8 bytes, little endian), the first MiB and the last MiB.
pub fn fingerprint(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    const MIB: u64 = 1 << 20;
    let mut f = std::fs::File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let mut h = Sha256::new();
    h.update(size.to_le_bytes());
    let mut buf = vec![0u8; MIB.min(size) as usize];
    f.read_exact(&mut buf).ok()?;
    h.update(&buf);
    let tail = MIB.min(size);
    f.seek(SeekFrom::Start(size - tail)).ok()?;
    let mut buf = vec![0u8; tail as usize];
    f.read_exact(&mut buf).ok()?;
    h.update(&buf);
    Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn guid() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("the operating system's random number source");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

fn speaker_name(t: &Transcript, id: &str) -> String {
    t.speaker(id).map(|s| s.display_name().to_string()).unwrap_or_else(|| id.to_string())
}

fn kind_of(m: &MediaFile) -> &'static str {
    if m.has_video {
        "video"
    } else {
        "audio"
    }
}

// ---------------------------------------------------------------------------------------------------------
// .prismtx
// ---------------------------------------------------------------------------------------------------------

#[derive(Serialize)]
struct Manifest<'a> {
    format: &'a str,
    version: &'a str,
    producer: Producer<'a>,
    created: String,
    /// Every document passed the final check against the project's name list (T6).
    deidentified: bool,
    documents: Vec<ManifestDoc>,
}

#[derive(Serialize)]
struct Producer<'a> {
    app: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct ManifestDoc {
    id: String,
    label: String,
    transcript: String,
    document: String,
    captions: String,
}

#[derive(Serialize)]
struct MediaRef {
    kind: &'static str,
    file_name: String,
    /// Absolute path when it was packaged.
    path: String,
    size_bytes: u64,
    duration_ms: i64,
    sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fingerprint: Option<String>,
}

#[derive(Serialize)]
struct CaseOut {
    /// The speaker id in transcript.json ("S1").
    speaker: String,
    /// The case's name: the speaker's shown name.
    name: String,
    role: &'static str,
    /// Interviewers and moderators: their turns are usually left out of coding (PrismQDA's `excluded`).
    interviewer: bool,
    /// Case attributes, all text (nominal) values: role first, then the speaker's own.
    attributes: Vec<AttributeOut>,
}

#[derive(Serialize)]
struct AttributeOut {
    name: String,
    level: &'static str,
    value: String,
}

#[derive(Serialize)]
struct DocumentOut<'a> {
    label: &'a str,
    source_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    media: MediaRef,
    cases: Vec<CaseOut>,
    /// Turns in milliseconds, in the order of transcript.json: (speaker, start_ms, end_ms).
    turns_ms: Vec<(String, i64, i64)>,
}

fn ms(s: f64) -> i64 {
    (s * 1000.0).round() as i64
}

fn cases(t: &Transcript) -> Vec<CaseOut> {
    use crate::model::Role;
    t.speakers
        .iter()
        .filter(|s| t.turns.iter().any(|x| x.speaker == s.id))
        .map(|s| {
            let role = crate::formats::role_word(s.role);
            let mut attributes = vec![AttributeOut { name: "role".into(), level: "nominal", value: role.into() }];
            attributes.extend(s.attributes.iter().map(|(k, v)| AttributeOut { name: k.clone(), level: "nominal", value: v.clone() }));
            CaseOut { speaker: s.id.clone(), name: s.display_name().to_string(), role, interviewer: matches!(s.role, Role::Interviewer | Role::Moderator), attributes }
        })
        .collect()
}

/// A .prismtx package of one or more recordings.
pub fn to_prismtx(docs: &[Doc], deidentified: bool, app_version: &str) -> std::io::Result<Vec<u8>> {
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut listed = Vec::new();
    for d in docs {
        let id = &d.media.id;
        let mut t = d.transcript.clone();
        t.snapshots.clear();
        let size = std::fs::metadata(&d.path).map(|m| m.len()).unwrap_or(d.media.bytes);
        let doc = DocumentOut {
            label: &d.media.fields.label,
            source_type: "transcript",
            date: d.media.fields.date.as_deref(),
            language: d.media.fields.language.as_deref(),
            media: MediaRef {
                kind: kind_of(d.media),
                file_name: d.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                path: d.path.display().to_string().trim_start_matches(r"\\?\").to_string(),
                size_bytes: size,
                duration_ms: ms(d.media.duration),
                sha256: d.media.sha256.clone(),
                fingerprint: fingerprint(&d.path),
            },
            cases: cases(&t),
            turns_ms: t.turns.iter().map(|x| (x.speaker.clone(), ms(x.start), ms(x.end))).collect(),
        };
        let vtt = crate::formats::to_vtt(&t, &crate::formats::CaptionOptions { speaker_names: true, clean: false });
        for (name, data) in [(format!("{id}/transcript.json"), t.to_json()), (format!("{id}/document.json"), serde_json::to_string_pretty(&doc)? + "\n"), (format!("{id}/transcript.vtt"), vtt)] {
            zw.start_file(name, o).map_err(std::io::Error::other)?;
            zw.write_all(data.as_bytes())?;
        }
        listed.push(ManifestDoc { id: id.clone(), label: d.media.fields.label.clone(), transcript: format!("{id}/transcript.json"), document: format!("{id}/document.json"), captions: format!("{id}/transcript.vtt") });
    }
    let manifest = Manifest {
        format: PRISMTX_FORMAT,
        version: PRISMTX_VERSION,
        producer: Producer { app: "Prism Transcribe", version: app_version },
        created: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        deidentified,
        documents: listed,
    };
    zw.start_file("manifest.json", o).map_err(std::io::Error::other)?;
    zw.write_all((serde_json::to_string_pretty(&manifest)? + "\n").as_bytes())?;
    Ok(zw.finish().map_err(std::io::Error::other)?.into_inner())
}

// ---------------------------------------------------------------------------------------------------------
// .qdpx (REFI-QDA 1.0)
// ---------------------------------------------------------------------------------------------------------

fn x(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => o.push(c),
        }
    }
    o
}

/// A turn's paragraph in the plain text: "Name: words". Returns the text and, per turn, its start and end in
/// characters (Unicode code points, end exclusive).
fn plain_text(t: &Transcript) -> (String, Vec<(usize, usize)>) {
    let mut s = String::new();
    let mut spans = Vec::new();
    for (k, turn) in t.turns.iter().enumerate() {
        if k > 0 {
            s.push_str("\n\n");
        }
        let start = s.chars().count();
        s.push_str(&speaker_name(t, &turn.speaker));
        s.push_str(": ");
        s.push_str(&turn.text());
        spans.push((start, s.chars().count()));
    }
    (s, spans)
}

/// The REFI-QDA URI for a linked recording: absolute://C:/path/file.wav
fn absolute_uri(p: &Path) -> String {
    format!("absolute://{}", p.display().to_string().trim_start_matches(r"\\?\").replace('\\', "/"))
}

pub fn to_qdpx(project_name: &str, docs: &[Doc], app_version: &str) -> std::io::Result<Vec<u8>> {
    let user = guid();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut texts: Vec<(String, String)> = Vec::new();
    let mut sources = String::new();
    // Variables: role, and every attribute name used by any speaker.
    let mut var_names: Vec<String> = vec!["role".into()];
    for d in docs {
        for s in &d.transcript.speakers {
            for k in s.attributes.keys() {
                if !var_names.contains(k) {
                    var_names.push(k.clone());
                }
            }
        }
    }
    let vars: Vec<(String, String)> = var_names.iter().map(|n| (n.clone(), guid())).collect();
    let mut cases = String::new();
    for d in docs {
        let t = d.transcript;
        let (text, spans) = plain_text(t);
        let tguid = guid();
        let sguid = guid();
        let mut sync = String::new();
        let mut selections = String::new();
        let mut turn_sel: Vec<(String, String)> = Vec::new(); // (speaker id, selection guid)
        // One sync point where each turn starts and one at the end of the text: a reader cuts the text at sync
        // points, so a separate end point per turn would make the blank line between turns a line of its own
        // (PrismQDA's importer does). A turn's stretch runs to the next turn's start.
        let points: Vec<(String, i64, usize)> = t
            .turns
            .iter()
            .zip(&spans)
            .map(|(turn, (a, _))| (guid(), ms(turn.start), *a))
            .chain(std::iter::once((guid(), t.turns.last().map(|l| ms(l.end)).unwrap_or(0), text.chars().count())))
            .collect();
        for (g, at, pos) in &points {
            sync.push_str(&format!(r#"<SyncPoint guid="{g}" timeStamp="{at}" position="{pos}"/>"#));
        }
        for (k, turn) in t.turns.iter().enumerate() {
            let gs = guid();
            selections.push_str(&format!(
                r#"<TranscriptSelection guid="{gs}" name="{}" fromSyncPoint="{}" toSyncPoint="{}"/>"#,
                x(&format!("{} {}", speaker_name(t, &turn.speaker), crate::model::hms(turn.start))),
                points[k].0,
                points[k + 1].0
            ));
            turn_sel.push((turn.speaker.clone(), gs));
        }
        let element = if d.media.has_video { "VideoSource" } else { "AudioSource" };
        sources.push_str(&format!(
            r#"<{element} guid="{sguid}" name="{}" path="{}" creatingUser="{user}" creationDateTime="{now}"><Transcript guid="{tguid}" name="{}" plainTextPath="internal://{tguid}.txt" creatingUser="{user}" creationDateTime="{now}">{sync}{selections}</Transcript></{element}>"#,
            x(&d.media.fields.label),
            x(&absolute_uri(&d.path)),
            x(&format!("{} transcript", d.media.fields.label)),
        ));
        texts.push((format!("sources/{tguid}.txt"), text));
        // One case per speaker who speaks, holding that speaker's turns. A recording with one participant also
        // gives that participant's case the whole source, which is how one-to-one interviews are usually coded.
        let participants: Vec<&str> = t.speakers.iter().filter(|s| !matches!(s.role, crate::model::Role::Interviewer | crate::model::Role::Moderator)).map(|s| s.id.as_str()).filter(|id| t.turns.iter().any(|x| x.speaker == *id)).collect();
        for s in t.speakers.iter().filter(|s| t.turns.iter().any(|x| x.speaker == s.id)) {
            let mut c = format!(r#"<Case guid="{}" name="{}">"#, guid(), x(&format!("{} ({})", s.display_name(), d.media.fields.label)));
            let mut values = vec![("role".to_string(), crate::formats::role_word(s.role).to_string())];
            values.extend(s.attributes.iter().map(|(k, v)| (k.clone(), v.clone())));
            for (k, v) in values {
                let g = &vars.iter().find(|(n, _)| *n == k).unwrap().1;
                c.push_str(&format!(r#"<VariableValue><VariableRef targetGUID="{g}"/><TextValue>{}</TextValue></VariableValue>"#, x(&v)));
            }
            if participants.len() == 1 && participants[0] == s.id {
                c.push_str(&format!(r#"<SourceRef targetGUID="{sguid}"/>"#));
            }
            for (_, gs) in turn_sel.iter().filter(|(sp, _)| *sp == s.id) {
                c.push_str(&format!(r#"<SelectionRef targetGUID="{gs}"/>"#));
            }
            c.push_str("</Case>");
            cases.push_str(&c);
        }
    }
    let variables: String = vars.iter().map(|(n, g)| format!(r#"<Variable guid="{g}" name="{}" typeOfVariable="Text"/>"#, x(n))).collect();
    let qde = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><Project xmlns="urn:QDA-XML:project:1.0" name="{}" origin="Prism Transcribe {}" creatingUserGUID="{user}" creationDateTime="{now}"><Users><User guid="{user}" name="Prism Transcribe"/></Users><Variables>{variables}</Variables><Cases>{cases}</Cases><Sources>{sources}</Sources></Project>"#,
        x(project_name),
        x(app_version)
    );
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zw.start_file("project.qde", o).map_err(std::io::Error::other)?;
    zw.write_all(qde.as_bytes())?;
    for (name, text) in texts {
        zw.start_file(name, o).map_err(std::io::Error::other)?;
        zw.write_all(text.as_bytes())?;
    }
    Ok(zw.finish().map_err(std::io::Error::other)?.into_inner())
}

/// A turn's text as it appears in the .qdpx plain text, for tests and readers.
pub fn qdpx_paragraph(t: &Transcript, turn: &Turn) -> String {
    format!("{}: {}", speaker_name(t, &turn.speaker), turn.text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Role, Snapshot, Speaker, Word};

    fn doc_parts() -> (MediaFile, Transcript, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let rec = tmp.path().join("int 07.wav");
        std::fs::write(&rec, vec![7u8; 3 << 20]).unwrap();
        let m: MediaFile = serde_json::from_value(serde_json::json!({"id": "F001", "label": "INT-07", "date": "2026-03-04", "source": rec.display().to_string(), "copied": false, "sha256": "abc", "bytes": 3145728, "duration": 61.5, "imported": ""})).unwrap();
        let w = |t: &str, a: f64, s: &str| Word { text: t.into(), start: a, end: a + 0.4, confidence: 0.9, speaker: s.into() };
        let mut t = Transcript::new("F001");
        t.speakers = vec![
            Speaker { id: "S1".into(), label: "Speaker 1".into(), name: Some("Interviewer".into()), role: Role::Interviewer, attributes: Default::default() },
            Speaker { id: "S2".into(), label: "Speaker 2".into(), name: Some("P07".into()), role: Role::Participant, attributes: [("site".to_string(), "North & co".to_string())].into() },
        ];
        t.turns = vec![
            Turn { speaker: "S1".into(), start: 1.0, end: 1.9, words: vec![w("Hello", 1.0, "S1"), w("there.", 1.5, "S1")] },
            Turn { speaker: "S2".into(), start: 2.5, end: 3.4, words: vec![w("Hi", 2.5, "S2"), w("café.", 3.0, "S2")] },
        ];
        t.snapshots.push(Snapshot { name: "Before de-identification".into(), created: String::new(), turns: vec![Turn { speaker: "S2".into(), start: 0.0, end: 1.0, words: vec![w("Maria", 0.0, "S2")] }] });
        (m, t, tmp)
    }

    fn read(zip_bytes: &[u8], name: &str) -> String {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();
        let mut s = String::new();
        z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
        s
    }

    #[test]
    fn fingerprint_is_prismqdas() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("small.bin");
        std::fs::write(&p, b"abc").unwrap();
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(3u64.to_le_bytes());
        h.update(b"abc");
        h.update(b"abc");
        let want: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(fingerprint(&p).unwrap(), want);
    }

    #[test]
    fn prismtx_holds_the_model_without_snapshots() {
        let (m, t, _tmp) = doc_parts();
        let bytes = to_prismtx(&[Doc { media: &m, transcript: &t, path: PathBuf::from(&m.source) }], true, "0.1.0").unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&read(&bytes, "manifest.json")).unwrap();
        assert_eq!(manifest["format"], PRISMTX_FORMAT);
        assert_eq!(manifest["version"], "1.0");
        assert_eq!(manifest["deidentified"], true);
        assert_eq!(manifest["documents"][0]["transcript"], "F001/transcript.json");
        let back = Transcript::from_json(&read(&bytes, "F001/transcript.json")).unwrap();
        assert!(back.snapshots.is_empty(), "snapshots hold the words from before de-identification");
        assert_eq!(back.turns, t.turns);
        assert!(!read(&bytes, "F001/transcript.json").contains("Maria"));
        let doc: serde_json::Value = serde_json::from_str(&read(&bytes, "F001/document.json")).unwrap();
        assert_eq!(doc["media"]["duration_ms"], 61500);
        assert_eq!(doc["media"]["size_bytes"], 3 << 20);
        assert_eq!(doc["media"]["fingerprint"].as_str().unwrap().len(), 64);
        assert_eq!(doc["cases"][0]["interviewer"], true);
        assert_eq!(doc["cases"][1]["attributes"][1]["value"], "North & co");
        assert_eq!(doc["turns_ms"][1], serde_json::json!(["S2", 2500, 3400]));
        assert!(read(&bytes, "F001/transcript.vtt").contains("<v P07>Hi café."));
    }

    #[test]
    fn qdpx_follows_refi_qda() {
        let (m, t, _tmp) = doc_parts();
        let bytes = to_qdpx("Study A", &[Doc { media: &m, transcript: &t, path: PathBuf::from(&m.source) }], "0.1.0").unwrap();
        let qde = read(&bytes, "project.qde");
        assert!(qde.starts_with(r#"<?xml version="1.0" encoding="utf-8"?><Project xmlns="urn:QDA-XML:project:1.0" name="Study A""#));
        assert!(qde.contains("<AudioSource ") && qde.contains("path=\"absolute://"));
        assert!(qde.contains(r#"timeStamp="2500" position="#), "{qde}");
        assert!(qde.contains("<TextValue>North &amp; co</TextValue>"));
        assert_eq!(qde.matches("<Case ").count(), 2);
        assert_eq!(qde.matches("<SourceRef ").count(), 1, "the one participant's case holds the whole source");
        assert_eq!(qde.matches("<SelectionRef ").count(), 2);
        // The plain text, and sync point positions in characters (code points), end exclusive.
        let tg = qde.split("plainTextPath=\"internal://").nth(1).unwrap().split(".txt").next().unwrap();
        let text = read(&bytes, &format!("sources/{tg}.txt"));
        assert_eq!(text, "Interviewer: Hello there.\n\nP07: Hi café.");
        let second = text.chars().count();
        assert!(qde.contains(&format!(r#"timeStamp="3400" position="{second}""#)), "the end of the last turn is the end of the text");
        assert_eq!(qdpx_paragraph(&t, &t.turns[1]), "P07: Hi café.");
    }
}
