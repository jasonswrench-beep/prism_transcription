//! De-identification (T6) for the app and the command line: where the key file is, the name pass, the pattern and
//! suggestion passes, the final check, and marking files de-identified. Every change goes into the key file's log
//! before the transcript is written, so a crash can leave a logged change not made, never a change not logged.

use crate::spell::Dictionary;
use crate::{save_transcript, EngineError, Result, Settings};
use prism_transcribe_core::deid::{self, FileCheck, Flag, KeyError, KeyFile, LogEntry, NameEntry, NameKind, NameSource, Suggestion};
use prism_transcribe_core::model::{ReviewStatus, Snapshot, Transcript};
use prism_transcribe_core::project::{now, MediaFile, Project};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The snapshot taken before a file's names are first replaced, so the change can be looked at or undone.
pub const BEFORE_SNAPSHOT: &str = "Before de-identification";

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum KeyState {
    /// No key file chosen for this project yet.
    None,
    /// Chosen, but not there (a removable drive that is not plugged in, a moved file).
    Missing { path: String },
    /// There, but it cannot be read: another project's key, damaged, or the project is locked.
    Unreadable { path: String, message: String },
    Ready { path: String, names: usize, changes: usize, inside_project: bool },
}

fn key_error(e: KeyError) -> EngineError {
    EngineError::Key(e.to_string())
}

pub fn key_path(settings: &Settings, p: &Project) -> Option<PathBuf> {
    settings.key_files.get(&p.file.id).cloned()
}

fn inside(dir: &Path, path: &Path) -> bool {
    let canon = |x: &Path| std::fs::canonicalize(x).unwrap_or_else(|_| x.to_path_buf());
    let file = path.parent().map(canon).unwrap_or_default();
    file.starts_with(canon(dir))
}

pub fn key_state(dir: &Path) -> Result<KeyState> {
    let p = Project::open(dir)?;
    let Some(path) = key_path(&Settings::load(), &p) else { return Ok(KeyState::None) };
    let shown = path.display().to_string();
    if !path.exists() {
        return Ok(KeyState::Missing { path: shown });
    }
    Ok(match KeyFile::load(&path, &p) {
        Ok(k) => KeyState::Ready { path: shown, names: k.names.len(), changes: k.log.len(), inside_project: inside(&p.dir, &path) },
        Err(e) => KeyState::Unreadable { path: shown, message: e.to_string() },
    })
}

/// Use a key file at `path` for the project: open it if it is there (it must be this project's), or start a new
/// one. The choice is kept in Settings, outside the project folder.
pub fn key_use(dir: &Path, path: &Path) -> Result<KeyState> {
    let p = Project::open(dir)?;
    if path.exists() {
        KeyFile::load(path, &p).map_err(key_error)?;
    } else {
        KeyFile::new(&p).save(path, &p).map_err(key_error)?;
    }
    let mut s = Settings::load();
    s.key_files.insert(p.file.id.clone(), path.to_path_buf());
    s.save()?;
    key_state(dir)
}

/// The project and its key file, ready to change.
pub fn key_open(dir: &Path) -> Result<(Project, PathBuf, KeyFile)> {
    let p = Project::open(dir)?;
    let path = key_path(&Settings::load(), &p).ok_or_else(|| EngineError::Key("choose where the key file goes first".into()))?;
    if !path.exists() {
        return Err(EngineError::Key(format!("the key file is not at {} (is its drive plugged in?)", path.display())));
    }
    let k = KeyFile::load(&path, &p).map_err(key_error)?;
    Ok((p, path, k))
}

fn transcripts(p: &Project) -> Result<Vec<(MediaFile, Transcript)>> {
    let mut out = Vec::new();
    for m in &p.file.files {
        if let Some(t) = p.load_transcript(&m.id)? {
            out.push((m.clone(), t));
        }
    }
    Ok(out)
}

/// Replace the name list (the table the user edits).
pub fn names_set(dir: &Path, names: Vec<NameEntry>) -> Result<Vec<NameEntry>> {
    let (p, path, mut k) = key_open(dir)?;
    k.names = names.into_iter().filter(|n| !n.real.trim().is_empty()).collect();
    k.save(&path, &p).map_err(key_error)?;
    Ok(k.names)
}

/// Add entries from a pasted roster; returns the whole list.
pub fn names_paste(dir: &Path, text: &str) -> Result<Vec<NameEntry>> {
    let (p, path, mut k) = key_open(dir)?;
    let new = deid::parse_roster(text, &k.names);
    k.names.extend(new);
    k.save(&path, &p).map_err(key_error)?;
    Ok(k.names)
}

#[derive(Clone, Debug, Serialize)]
pub struct FileCount {
    pub file: String,
    pub label: String,
    pub count: usize,
}

/// How many replacements the name list would make in each transcript.
pub fn names_preview(dir: &Path) -> Result<Vec<FileCount>> {
    let (p, _, k) = key_open(dir)?;
    Ok(transcripts(&p)?.iter().map(|(m, t)| FileCount { file: m.id.clone(), label: m.fields.label.clone(), count: deid::count_names(t, &k.names) }).collect())
}

/// Write the log first, then the transcript.
fn record(p: &Project, path: &Path, k: &mut KeyFile, entries: Vec<LogEntry>, t: Transcript) -> Result<()> {
    k.log.extend(entries);
    k.save(path, p).map_err(key_error)?;
    save_transcript(&p.dir, t)
}

fn apply(dir: &Path, only: Option<&NameEntry>) -> Result<Vec<FileCount>> {
    let (p, path, mut k) = key_open(dir)?;
    let list: Vec<NameEntry> = only.map(|n| vec![n.clone()]).unwrap_or_else(|| k.names.clone());
    let by = deid::current_user();
    let mut out = Vec::new();
    for (m, mut t) in transcripts(&p)? {
        if deid::count_names(&t, &list) == 0 {
            continue;
        }
        if !t.snapshots.iter().any(|s| s.name == BEFORE_SNAPSHOT) {
            t.snapshots.push(Snapshot { name: BEFORE_SNAPSHOT.into(), created: now(), turns: t.turns.clone() });
        }
        let log = deid::apply_names(&mut t, &m, &list, &by);
        out.push(FileCount { file: m.id.clone(), label: m.fields.label.clone(), count: log.len() });
        record(&p, &path, &mut k, log, t)?;
    }
    Ok(out)
}

/// Replace every name on the list in every transcript (logged). Returns the count per changed file.
pub fn names_apply(dir: &Path) -> Result<Vec<FileCount>> {
    apply(dir, None)
}

/// The pattern pass over every transcript.
pub fn flags_find(dir: &Path) -> Result<Vec<Flag>> {
    let (p, _, k) = key_open(dir)?;
    let ignored: HashSet<String> = k.ignored.iter().cloned().collect();
    Ok(transcripts(&p)?.iter().flat_map(|(m, t)| deid::flags(t, m, &ignored)).collect())
}

/// Accept a flag: replace it (logged). False when the transcript changed since the flag was found.
pub fn flag_apply(dir: &Path, flag: &Flag, replacement: &str) -> Result<bool> {
    let (p, path, mut k) = key_open(dir)?;
    let Some(mut t) = p.load_transcript(&flag.file)? else { return Ok(false) };
    let Some(entry) = deid::apply_flag(&mut t, flag, replacement, &deid::current_user()) else { return Ok(false) };
    record(&p, &path, &mut k, vec![entry], t)?;
    Ok(true)
}

/// Skip a flag: the same text is not flagged again in this project.
pub fn flag_skip(dir: &Path, text: &str) -> Result<()> {
    let (p, path, mut k) = key_open(dir)?;
    k.ignored.insert(text.to_lowercase());
    k.save(&path, &p).map_err(key_error)
}

/// The suggestions pass, with the dictionary of the project's language (the first file's, else the user's).
pub fn suggestions(dir: &Path) -> Result<(Vec<Suggestion>, Option<String>)> {
    let (p, _, k) = key_open(dir)?;
    let all = transcripts(&p)?;
    let lang = p.file.files.iter().find_map(|m| m.fields.language.clone());
    let dict = Dictionary::for_language(lang.as_deref());
    let files: Vec<(&MediaFile, &Transcript)> = all.iter().map(|(m, t)| (m, t)).collect();
    Ok((deid::suggestions(&files, &k.names, &p.file.word_list, &|w| dict.known(w)), dict.language.clone()))
}

/// Accept a suggestion: the word joins the name list and is replaced everywhere now (logged).
pub fn suggestion_accept(dir: &Path, word: &str, replacement: &str, kind: NameKind) -> Result<Vec<FileCount>> {
    let entry = {
        let (p, path, mut k) = key_open(dir)?;
        let replacement = if replacement.trim().is_empty() { deid::default_replacement(kind, &k.names) } else { replacement.trim().to_string() };
        let e = NameEntry { real: word.trim().to_string(), variants: Vec::new(), replacement, kind, source: NameSource::Suggestion };
        if !k.names.iter().any(|n| n.real.eq_ignore_ascii_case(&e.real)) {
            k.names.push(e.clone());
            k.save(&path, &p).map_err(key_error)?;
        }
        e
    };
    apply(dir, Some(&entry))
}

/// Skip a suggestion: the word goes on the project word list, so it is not suggested again.
pub fn suggestion_skip(dir: &Path, word: &str) -> Result<Vec<String>> {
    Project::update(dir, |p| {
        if !p.file.word_list.iter().any(|w| w.eq_ignore_ascii_case(word.trim())) {
            p.file.word_list.push(word.trim().to_string());
        }
        Ok(p.file.word_list.clone())
    })
    .map_err(Into::into)
}

/// The project word list, set whole.
pub fn word_list_set(dir: &Path, words: Vec<String>) -> Result<Vec<String>> {
    Project::update(dir, |p| {
        let mut seen = HashSet::new();
        p.file.word_list = words.into_iter().map(|w| w.trim().to_string()).filter(|w| !w.is_empty() && seen.insert(w.to_lowercase())).collect();
        Ok(p.file.word_list.clone())
    })
    .map_err(Into::into)
}

/// The final check over every file. Without a key file there is no list to check against, which is an error: an
/// export that should be de-identified cannot be shown to be.
pub fn final_check(dir: &Path) -> Result<Vec<FileCheck>> {
    let (p, _, k) = key_open(dir)?;
    let mut out = Vec::new();
    for m in &p.file.files {
        let t = p.load_transcript(&m.id)?;
        out.push(deid::check_file(m, t.as_ref(), &k.names));
    }
    Ok(out)
}

pub fn check_one(dir: &Path, id: &str) -> Result<FileCheck> {
    let (p, _, k) = key_open(dir)?;
    let m = p.media(id)?;
    let t = p.load_transcript(id)?;
    Ok(deid::check_file(m, t.as_ref(), &k.names))
}

/// Mark a file de-identified: only when it passes the final check.
pub fn mark_deidentified(dir: &Path, id: &str) -> Result<FileCheck> {
    let c = check_one(dir, id)?;
    if !c.passes() {
        return Err(EngineError::Key(format!("{} still has {} name(s) from the list; replace them first", c.label, c.leftovers.len())));
    }
    let p = Project::open(dir)?;
    let mut t = p.load_transcript(id)?.ok_or_else(|| EngineError::NoTranscript(c.label.clone()))?;
    if !deid::is_deidentified(t.review) {
        t.review = ReviewStatus::DeIdentified;
        save_transcript(dir, t)?;
    }
    check_one(dir, id)
}

/// The change log, newest first.
pub fn log(dir: &Path) -> Result<Vec<LogEntry>> {
    let (_, _, k) = key_open(dir)?;
    Ok(k.log.into_iter().rev().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_transcribe_core::model::{Speaker, Turn, Word};

    /// The phase's own test: a de-identified export passes the final check with zero list names left.
    #[test]
    fn names_patterns_suggestions_check_and_export() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("APPDATA", tmp.path().join("appdata"));
        let dir = tmp.path().join("proj");
        let mut p = Project::create(&dir, "Study A").unwrap();
        let rec = tmp.path().join("int.wav");
        std::fs::write(&rec, b"RIFF").unwrap();
        let id = p.add_media(&rec, Default::default(), false).unwrap();
        let mut t = Transcript::new(&id);
        t.speakers = vec![Speaker { id: "S1".into(), label: "Speaker 1".into(), name: Some("Maria".into()), role: Default::default(), attributes: Default::default() }];
        let words: Vec<Word> = "So Maria, you said Keith called you on 07700 900123 about Riverside?"
            .split(' ')
            .enumerate()
            .map(|(i, w)| Word { text: w.into(), start: i as f64, end: i as f64 + 0.8, confidence: 0.9, speaker: "S1".into() })
            .collect();
        t.turns = vec![Turn { speaker: "S1".into(), start: 0.0, end: 11.8, words }];
        p.save_transcript(&t).unwrap();

        assert!(matches!(key_state(&dir).unwrap(), KeyState::None));
        assert!(names_apply(&dir).is_err(), "no key file yet");
        let key = tmp.path().join("safe").join("Study A.prismkey");
        assert!(matches!(key_use(&dir, &key).unwrap(), KeyState::Ready { inside_project: false, .. }));
        names_paste(&dir, "Maria Lopez\tP07\nRiverside\t[Place]").unwrap();
        assert_eq!(names_preview(&dir).unwrap()[0].count, 3);
        assert!(!final_check(&dir).unwrap()[0].passes());
        assert!(mark_deidentified(&dir, &id).is_err(), "names left");
        assert_eq!(names_apply(&dir).unwrap()[0].count, 3);

        let f = flags_find(&dir).unwrap();
        assert_eq!(f.len(), 1);
        assert!(flag_apply(&dir, &f[0], "[phone number]").unwrap());
        let (s, _) = suggestions(&dir).unwrap();
        assert!(s.iter().any(|x| x.word == "Keith"), "{s:?}");
        suggestion_accept(&dir, "Keith", "", NameKind::Person).unwrap();
        let after = Project::open(&dir).unwrap().load_transcript(&id).unwrap().unwrap();
        assert_eq!(after.turns[0].text(), "So P07, you said [Person 2] called you on [phone number] about [Place]?");
        assert_eq!(after.speakers[0].name.as_deref(), Some("P07"));
        assert!(after.snapshots.iter().any(|s| s.name == BEFORE_SNAPSHOT));

        let c = mark_deidentified(&dir, &id).unwrap();
        assert!(c.passes() && c.review == ReviewStatus::DeIdentified);
        let log = log(&dir).unwrap();
        assert_eq!(log.len(), 5, "Maria, Riverside, the speaker name, the phone number, Keith: {log:?}");
        assert!(log.iter().all(|l| !l.by.is_empty() && !l.when.is_empty()));

        // The de-identified export has no list name in it, and the key file never goes into it.
        let out = tmp.path().join("out.txt");
        crate::export_checked(&Project::open(&dir).unwrap(), &id, crate::ExportFormat::Text, &out, &Default::default(), true).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        for name in ["Maria", "Lopez", "Keith", "Riverside", "07700"] {
            assert!(!text.contains(name), "{name} in {text}");
        }
        // A name added to the list later blocks the de-identified export until it is replaced.
        names_paste(&dir, "you").unwrap();
        assert!(crate::export_checked(&Project::open(&dir).unwrap(), &id, crate::ExportFormat::Text, &out, &Default::default(), true).is_err());
    }
}
