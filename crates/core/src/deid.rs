//! De-identification (T6): the name map, the pattern and suggestion passes, the audit log, the key file and the
//! final check.
//!
//! **The name map** lists real names and places, each with its other spellings and a replacement (P07,
//! [Participant 3], [Employer], [City]). Replacing is whole-word, ignores case, and is possessive aware: "Maria's"
//! becomes "P07's", because the apostrophe ends the word. Longer names go first, so "Maria Lopez" is replaced
//! whole before "Maria" alone. Word times are kept by the editing rule (T5): a replacement takes the time span of
//! the words it replaced.
//!
//! **The key file** holds the name map, the log of every change (original, replacement, file, time in the recording,
//! when, and who accepted it) and the flags the user skipped. It is saved where the user chooses, apart from the
//! transcripts, and nothing in it is ever exported.
//!
//! **The final check** looks for every name and spelling on the list in everything an export carries: the turns,
//! speaker names and attributes, and the file's label, interviewer and note.

use crate::edit::{find_in, set_turn_text, FindOptions};
use crate::model::{hms, ReviewStatus, Transcript};
use crate::project::{now, MediaFile, Project, ProjectError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

// ---------------------------------------------------------------------------------------------------------
// The name map
// ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameKind {
    #[default]
    Person,
    Place,
    Organisation,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameSource {
    #[default]
    Typed,
    Roster,
    Suggestion,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NameEntry {
    /// The name as the user wrote it: "Maria Lopez".
    pub real: String,
    /// Other ways it is said or spelled: "Maria", "Ms Lopez", "Mari".
    #[serde(default)]
    pub variants: Vec<String>,
    pub replacement: String,
    #[serde(default)]
    pub kind: NameKind,
    #[serde(default)]
    pub source: NameSource,
}

/// Name parts that are not names on their own ("de" in "Ana de Souza").
const PARTICLES: &[&str] = &["de", "da", "das", "do", "dos", "del", "della", "di", "du", "la", "le", "van", "von", "der", "den", "ter", "bin", "binti", "al", "el", "mc", "st", "jr", "sr"];

/// The spellings a person's full name is usually heard as: each part on its own ("Maria Lopez" gives "Maria" and
/// "Lopez"). Places and organisations get none; the user adds what they need.
pub fn auto_variants(real: &str, kind: NameKind) -> Vec<String> {
    let parts: Vec<&str> = real.split_whitespace().collect();
    if kind != NameKind::Person || parts.len() < 2 {
        return Vec::new();
    }
    parts
        .iter()
        .map(|p| p.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|p| p.chars().count() >= 2 && !PARTICLES.contains(&p.to_lowercase().as_str()))
        .map(String::from)
        .collect()
}

/// What a new entry is replaced with when the user gives nothing: [Person n], [Place], [Organisation], [redacted].
pub fn default_replacement(kind: NameKind, names: &[NameEntry]) -> String {
    match kind {
        NameKind::Person => format!("[Person {}]", names.iter().filter(|n| n.kind == NameKind::Person).count() + 1),
        NameKind::Place => "[Place]".into(),
        NameKind::Organisation => "[Organisation]".into(),
        NameKind::Other => "[redacted]".into(),
    }
}

fn kind_from(word: &str) -> Option<NameKind> {
    match word.trim().to_lowercase().as_str() {
        "person" | "people" | "participant" | "name" => Some(NameKind::Person),
        "place" | "city" | "town" | "location" | "site" => Some(NameKind::Place),
        "organisation" | "organization" | "org" | "employer" | "company" | "school" => Some(NameKind::Organisation),
        "other" => Some(NameKind::Other),
        _ => None,
    }
}

/// Looks like a replacement rather than a name: P07, [Participant 3], INT-07.
fn looks_like_replacement(s: &str) -> bool {
    let s = s.trim();
    s.starts_with('[') || (s.chars().any(|c| c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
}

/// "Lopez, Maria" is Maria Lopez; a name without a comma stays as it is.
fn surname_last(name: &str) -> String {
    match name.split_once(',') {
        Some((last, first)) if !first.trim().is_empty() && !first.contains(',') => format!("{} {}", first.trim(), last.trim()),
        _ => name.trim().to_string(),
    }
}

/// What follows a name: "P07", "P07, person", "[College] organisation", "person". A kind written after the
/// replacement, with or without a comma, is the kind, not part of the replacement.
fn split_rest(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in s.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        match part.rsplit_once(char::is_whitespace) {
            Some((head, last)) if kind_from(last).is_some() && !head.trim().is_empty() => {
                out.push(head.trim().to_string());
                out.push(last.to_string());
            }
            _ => out.push(part.to_string()),
        }
    }
    out
}

/// Entries from a pasted roster, one per line: "Maria Lopez", "Maria Lopez<TAB>P07", "Maria Lopez = P07",
/// "Maria Lopez -> P07", "Maria Lopez, P07", "Lopez, Maria", with an optional third column naming the kind
/// (person, place, organisation). Names already in `existing` (ignoring case) are left out.
pub fn parse_roster(text: &str, existing: &[NameEntry]) -> Vec<NameEntry> {
    let mut out: Vec<NameEntry> = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        // The name, and what follows it (the replacement and the kind, in either order).
        let (left, rest): (String, Vec<String>) = if line.contains('\t') {
            let mut it = line.split('\t').map(|c| c.trim().to_string());
            (it.next().unwrap_or_default(), it.flat_map(|c| split_rest(&c)).collect())
        } else if let Some(sep) = ["=>", "->", "\u{2192}", "="].iter().find(|s| line.contains(**s)) {
            let mut it = line.splitn(2, *sep);
            (it.next().unwrap_or("").trim().to_string(), split_rest(it.next().unwrap_or("")))
        } else if line.contains(',') || line.contains(';') {
            let parts: Vec<String> = line.split([',', ';']).map(|c| c.trim().to_string()).collect();
            let is_name = |s: &String| !looks_like_replacement(s) && kind_from(s).is_none();
            if parts.len() >= 2 && is_name(&parts[1]) {
                // "Lopez, Maria" and "Lopez, Maria, P07": surname first.
                (format!("{}, {}", parts[0], parts[1]), parts[2..].iter().flat_map(|c| split_rest(c)).collect())
            } else {
                (parts[0].clone(), parts[1..].iter().flat_map(|c| split_rest(c)).collect())
            }
        } else {
            (line.to_string(), Vec::new())
        };
        let cols: Vec<String> = std::iter::once(surname_last(&left)).chain(rest.into_iter().filter(|c| !c.is_empty())).collect();
        let real = cols[0].trim().to_string();
        if real.is_empty() || existing.iter().chain(&out).any(|n| n.real.eq_ignore_ascii_case(&real)) {
            continue;
        }
        let mut replacement = cols.get(1).filter(|r| !r.is_empty() && kind_from(r).is_none()).cloned();
        // The kind: a third column, or what a bracketed replacement says ([City], [Employer]), else a person.
        let kind = cols
            .iter()
            .skip(1)
            .find_map(|c| kind_from(c).or_else(|| c.strip_prefix('[').and_then(|r| kind_from(r.split([']', ' ']).next().unwrap_or("")))))
            .unwrap_or_default();
        if replacement.is_none() {
            let all: Vec<NameEntry> = existing.iter().chain(&out).cloned().collect();
            replacement = Some(default_replacement(kind, &all));
        }
        out.push(NameEntry { variants: auto_variants(&real, kind), real, replacement: replacement.unwrap(), kind, source: NameSource::Roster });
    }
    out
}

/// Every spelling on the list with its entry, longest first.
fn terms(names: &[NameEntry]) -> Vec<(String, usize)> {
    let mut t: Vec<(String, usize)> = names
        .iter()
        .enumerate()
        .flat_map(|(i, n)| std::iter::once(n.real.clone()).chain(n.variants.iter().cloned()).map(move |s| (s.trim().to_string(), i)))
        .filter(|(s, _)| s.chars().count() >= 2)
        .collect();
    t.sort_by(|a, b| b.0.chars().count().cmp(&a.0.chars().count()).then_with(|| a.0.cmp(&b.0)));
    t.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
    t
}

/// A name that is part of an address ("Riverside.edu", "maria.lopez@...", "riverside.org/staff"): the whole address,
/// without the sentence's own punctuation around it, so no half-replaced address is left behind.
fn address_around(text: &str, s: usize, e: usize) -> Option<(usize, usize)> {
    let joins = |mut it: std::str::Chars| matches!((it.next(), it.next()), (Some('.' | '@' | '/'), Some(c)) if c.is_alphanumeric());
    let joined_after = joins(text[e..].chars());
    let mut back = text[..s].chars().rev();
    let joined_before = matches!((back.next(), back.next()), (Some('.' | '@' | '/'), Some(c)) if c.is_alphanumeric());
    if !joined_after && !joined_before {
        return None;
    }
    let mut ts = text[..s].char_indices().rev().find(|(_, c)| c.is_whitespace()).map(|(i, c)| i + c.len_utf8()).unwrap_or(0);
    let mut te = text[e..].char_indices().find(|(_, c)| c.is_whitespace()).map(|(i, _)| e + i).unwrap_or(text.len());
    while let Some(c) = text[..te].chars().last().filter(|c| te > e && ".,;:!?)\"'\u{201d}\u{2019}".contains(*c)) {
        te -= c.len_utf8();
    }
    while let Some(c) = text[ts..].chars().next().filter(|c| ts < s && "(\"'\u{201c}\u{2018}".contains(*c)) {
        ts += c.len_utf8();
    }
    Some((ts, te))
}

/// A person's initial just before the name ("D Okafor", "D. Okafor"): where it starts, so it goes with the name.
fn initial_before(text: &str, s: usize) -> Option<usize> {
    let b = text[..s].strip_suffix(' ')?;
    let b = b.strip_suffix('.').unwrap_or(b);
    let c = b.chars().last().filter(|c| c.is_uppercase() && *c != 'I')?;
    let start = b.len() - c.len_utf8();
    (start == 0 || text[..start].ends_with(char::is_whitespace)).then_some(start)
}

/// Where names on the list occur in `text`: (start, end, entry), not overlapping, in order. A name inside one of
/// the list's own replacements ("College" in "[College]") is not a name left; a name that is part of an address
/// takes the whole address; a person's name takes an initial written before it.
pub fn find_names(text: &str, names: &[NameEntry]) -> Vec<(usize, usize, usize)> {
    let o = FindOptions { whole_word: true, match_case: false };
    let mut placed: Vec<(usize, usize)> = Vec::new();
    for n in names {
        let r = n.replacement.trim();
        if !r.is_empty() {
            placed.extend(text.match_indices(r).map(|(i, m)| (i, i + m.len())));
        }
    }
    let mut hits: Vec<(usize, usize, usize)> = Vec::new();
    for (term, i) in terms(names) {
        for (s, e) in find_in(text, &term, &o) {
            if placed.iter().any(|&(a, b)| s < b && a < e) {
                continue;
            }
            if !hits.iter().any(|&(a, b, _)| s < b && a < e) {
                hits.push((s, e, i));
            }
        }
    }
    // (start, end, entry, where the name itself starts)
    let mut grown: Vec<(usize, usize, usize, usize)> = hits
        .into_iter()
        .map(|(s, e, i)| match address_around(text, s, e) {
            Some((a, b)) => (a, b, i, s),
            None if names[i].kind == NameKind::Person => (initial_before(text, s).unwrap_or(s), e, i, s),
            None => (s, e, i, s),
        })
        .collect();
    grown.sort_by_key(|g| (g.0, g.3));
    // An address holding two names is one replacement: the first name's.
    let mut out: Vec<(usize, usize, usize)> = Vec::with_capacity(grown.len());
    for (s, e, i, _) in grown {
        let h = (s, e, i);
        match out.last_mut() {
            Some(last) if h.0 < last.1 => last.1 = last.1.max(h.1),
            _ => out.push(h),
        }
    }
    out
}

fn replace_ranges(text: &str, hits: &[(usize, usize, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (s, e, r) in hits {
        out.push_str(&text[at..*s]);
        out.push_str(r);
        at = *e;
    }
    out.push_str(&text[at..]);
    out
}

// ---------------------------------------------------------------------------------------------------------
// The audit log
// ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// When the change was made.
    pub when: String,
    /// The media file's id and label.
    pub file: String,
    pub label: String,
    /// Where in the file: "00:12:03" for a turn, or "speaker name", "attribute site".
    pub place: String,
    /// Seconds into the recording, for a change in a turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
    pub original: String,
    pub replacement: String,
    /// "name list", "pattern: phone number" ...
    pub reason: String,
    /// The Windows user who accepted it.
    pub by: String,
}

/// Who is accepting changes: the Windows user name.
pub fn current_user() -> String {
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "unknown".into())
}

/// Byte offset of each word in the turn's text (words joined by single spaces, as `Turn::text` does).
fn word_offsets(t: &crate::model::Turn) -> Vec<usize> {
    let mut at = 0;
    t.words
        .iter()
        .map(|w| {
            let s = at;
            at += w.text.len() + 1;
            s
        })
        .collect()
}

/// The time of the word holding byte `pos` of a turn's text.
fn time_at(t: &crate::model::Turn, pos: usize) -> f64 {
    let offs = word_offsets(t);
    let i = offs.iter().rposition(|&o| o <= pos).unwrap_or(0);
    t.words.get(i).map(|w| w.start).unwrap_or(t.start)
}

/// Replace every name on the list in one transcript: its turns, its speakers' names and attribute values. Returns
/// one log entry per replacement.
pub fn apply_names(t: &mut Transcript, m: &MediaFile, names: &[NameEntry], by: &str) -> Vec<LogEntry> {
    let when = now();
    let mut log = Vec::new();
    let entry = |place: String, at: Option<f64>, original: &str, replacement: &str| LogEntry {
        when: when.clone(),
        file: m.id.clone(),
        label: m.fields.label.clone(),
        place,
        at,
        original: original.to_string(),
        replacement: replacement.to_string(),
        reason: "name list".into(),
        by: by.to_string(),
    };
    for i in 0..t.turns.len() {
        let text = t.turns[i].text();
        let hits = find_names(&text, names);
        if hits.is_empty() {
            continue;
        }
        let mut reps = Vec::with_capacity(hits.len());
        for &(s, e, n) in &hits {
            let at = time_at(&t.turns[i], s);
            log.push(entry(hms(at), Some(at), &text[s..e], &names[n].replacement));
            reps.push((s, e, names[n].replacement.clone()));
        }
        set_turn_text(&mut t.turns, i, &replace_ranges(&text, &reps));
    }
    for sp in &mut t.speakers {
        if let Some(name) = sp.name.clone() {
            let hits = find_names(&name, names);
            if !hits.is_empty() {
                let reps: Vec<_> = hits.iter().map(|&(s, e, n)| (s, e, names[n].replacement.clone())).collect();
                for &(s, e, n) in &hits {
                    log.push(entry("speaker name".into(), None, &name[s..e], &names[n].replacement));
                }
                sp.name = Some(replace_ranges(&name, &reps));
            }
        }
        for (k, v) in sp.attributes.iter_mut() {
            let hits = find_names(v, names);
            if !hits.is_empty() {
                let reps: Vec<_> = hits.iter().map(|&(s, e, n)| (s, e, names[n].replacement.clone())).collect();
                for &(s, e, n) in &hits {
                    log.push(entry(format!("attribute {k}"), None, &v[s..e], &names[n].replacement));
                }
                *v = replace_ranges(v, &reps);
            }
        }
    }
    log
}

/// How many replacements [`apply_names`] would make in a transcript, without changing it.
pub fn count_names(t: &Transcript, names: &[NameEntry]) -> usize {
    t.turns.iter().map(|x| find_names(&x.text(), names).len()).sum::<usize>()
        + t.speakers.iter().map(|s| s.name.as_deref().map(|n| find_names(n, names).len()).unwrap_or(0) + s.attributes.values().map(|v| find_names(v, names).len()).sum::<usize>()).sum::<usize>()
}

// ---------------------------------------------------------------------------------------------------------
// The pattern pass
// ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    Email,
    WebAddress,
    Phone,
    Address,
    Date,
    Birth,
    IdNumber,
}

impl PatternKind {
    pub fn words(self) -> &'static str {
        match self {
            PatternKind::Email => "email address",
            PatternKind::WebAddress => "web address",
            PatternKind::Phone => "phone number",
            PatternKind::Address => "street address or postcode",
            PatternKind::Date => "date",
            PatternKind::Birth => "birth date or place",
            PatternKind::IdNumber => "ID-like number",
        }
    }

    pub fn replacement(self) -> &'static str {
        match self {
            PatternKind::Email => "[email address]",
            PatternKind::WebAddress => "[web address]",
            PatternKind::Phone => "[phone number]",
            PatternKind::Address => "[address]",
            PatternKind::Date => "[date]",
            PatternKind::Birth => "[birth details]",
            PatternKind::IdNumber => "[ID number]",
        }
    }
}

/// Something the pattern pass found, for the user to accept (replace) or skip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flag {
    pub file: String,
    pub label: String,
    pub turn: usize,
    /// Byte range in the turn's text.
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub kind: PatternKind,
    pub at: f64,
    /// A little of the turn either side, for the review list.
    pub before: String,
    pub after: String,
}

struct Patterns {
    list: Vec<(PatternKind, regex::Regex)>,
    born: regex::Regex,
}

const BORN_STOPS: &[&str] = &["and", "but", "so", "when", "then", "because", "where", "which", "with", "to", "or"];

const NUMBER_WORDS: &str = "zero|oh|one|two|three|four|five|six|seven|eight|nine";
const MONTHS: &str = "jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?";
const TLDS: &str = "com|org|net|edu|gov|io|info|me|ca|au|nz|ie|uk|us|de|fr";
const STREETS: &str = "Street|St|Road|Rd|Avenue|Ave|Lane|Ln|Drive|Dr|Boulevard|Blvd|Court|Ct|Close|Way|Place|Pl|Crescent|Terrace|Square|Sq|Parkway|Highway|Hwy|Row|Grove|Gardens|Mews|Hill";
const NUMBER_NAMES: &str = "one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|twenty|thirty|forty|fifty|sixty|seventy|eighty|ninety|hundred";

impl Patterns {
    fn new() -> Self {
        let r = |p: String| regex::Regex::new(&p).expect("pattern");
        use PatternKind::*;
        let list = vec![
            (Email, r(r"(?i)\b[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}\b".into())),
            // Said aloud: "maria dot lopez at example dot com".
            (Email, r(format!(r"(?i)\b[a-z0-9_-]+(?: (?:dot|underscore) [a-z0-9_-]+)* at [a-z0-9-]+(?: dot [a-z0-9-]+)* dot (?:{TLDS})\b"))),
            (WebAddress, r(r"(?i)\b(?:https?://|www\.)[^\s]+".into())),
            (WebAddress, r(format!(r"(?i)\b[a-z0-9-]+(?:\.[a-z0-9-]+)*\.(?:{TLDS})\b(?:/[^\s]*)?"))),
            (WebAddress, r(format!(r"(?i)\b(?:www dot )?[a-z0-9-]+(?: dot [a-z0-9-]+)* dot (?:{TLDS})\b"))),
            (Date, r(r"\b\d{1,2}[/.-]\d{1,2}[/.-](?:\d{4}|\d{2})\b".into())),
            (Date, r(r"\b(?:19|20)\d{2}-\d{1,2}-\d{1,2}\b".into())),
            (Date, r(format!(r"(?i)\b\d{{1,2}}(?:st|nd|rd|th)?(?: of)? (?:{MONTHS}),? (?:19|20)\d{{2}}\b"))),
            (Date, r(format!(r"(?i)\b(?:{MONTHS}) (?:the )?\d{{1,2}}(?:st|nd|rd|th)?,? (?:19|20)\d{{2}}\b"))),
            (IdNumber, r(r"\b\d{3}-\d{2}-\d{4}\b".into())),
            (Phone, r(r"\+?\(?\d[\d().\- ]{5,}\d".into())),
            (Phone, r(format!(r"(?i)\b(?:(?:{NUMBER_WORDS}|double|triple)[ ,-]+){{6,}}(?:{NUMBER_WORDS})\b"))),
            (Address, r(format!(r"\b(?:\d{{1,5}}[A-Za-z]?|(?:(?i:{NUMBER_NAMES})[ -]){{0,2}}(?i:{NUMBER_NAMES})),? (?:[A-Z][A-Za-z'-]+ ){{1,3}}(?:{STREETS})\b\.?"))),
            (Address, r(r"\b[A-Z]{1,2}\d[A-Z\d]? ?\d[A-Z]{2}\b".into())),
            (Address, r(r"\b[A-Z]{2},? \d{5}(?:-\d{4})?\b".into())),
            (Address, r(r"\b[A-Z]\d[A-Z] ?\d[A-Z]\d\b".into())),
            (Address, r(r"(?i)\bP\.? ?O\.? box \d+".into())),
        ];
        Patterns { list, born: r(r"(?i)\bborn (?:in|on|at) (?:the )?".into()) }
    }
}

fn plausible_phone(m: &str) -> bool {
    let digits = m.chars().filter(|c| c.is_ascii_digit()).count();
    if !(7..=15).contains(&digits) {
        return false;
    }
    // Years in a row ("1998 2004 2010") are not a phone number.
    let groups: Vec<&str> = m.split_whitespace().collect();
    if groups.len() > 1 && groups.iter().all(|g| g.len() == 4 && (g.starts_with("19") || g.starts_with("20"))) {
        return false;
    }
    m.contains(['-', '.', '(', '+']) || m.starts_with('0') || (digits >= 10 && !m.contains(' '))
}

/// Tokens that look like an ID: six or more digits, or letters and four or more digits mixed (AB123456).
fn id_tokens(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    for tok in text.split(' ') {
        let s = at;
        at += tok.len() + 1;
        if tok.starts_with(['$', '\u{a3}', '\u{20ac}']) {
            continue;
        }
        let lead = tok.len() - tok.trim_start_matches(|c: char| !c.is_alphanumeric()).len();
        let core = tok.trim_matches(|c: char| !c.is_alphanumeric());
        if core.is_empty() || core.contains(',') {
            continue;
        }
        let digits = core.chars().filter(|c| c.is_ascii_digit()).count();
        let letters = core.chars().filter(|c| c.is_alphabetic()).count();
        let only = core.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        if only && ((letters == 0 && digits >= 6) || (letters >= 1 && digits >= 4 && core.len() >= 6)) {
            out.push((s + lead, s + lead + core.len()));
        }
    }
    out
}

/// Where personal details may be in `text`: (start, end, kind), not overlapping, in order.
pub fn find_patterns(text: &str) -> Vec<(usize, usize, PatternKind)> {
    static P: std::sync::OnceLock<Patterns> = std::sync::OnceLock::new();
    let p = P.get_or_init(Patterns::new);
    let mut hits: Vec<(usize, usize, PatternKind)> = Vec::new();
    let push = |s: usize, e: usize, k: PatternKind, hits: &mut Vec<(usize, usize, PatternKind)>| {
        // Sentence punctuation after a web address or a number is not part of it.
        let e = s + text[s..e].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ' ']).len();
        if e > s && !hits.iter().any(|&(a, b, _)| s < b && a < e) {
            hits.push((s, e, k));
        }
    };
    // "born in 1985", "born on the 3rd of March", "born at St Mary's": the phrase and up to four words after it,
    // stopping at a joining word or at punctuation.
    for m in p.born.find_iter(text) {
        let mut end = m.end();
        let mut at = m.end();
        for (k, tok) in text[m.end()..].split(' ').enumerate() {
            let core = tok.trim_end_matches(|c: char| !c.is_alphanumeric());
            if k >= 4 || core.is_empty() || BORN_STOPS.contains(&core.to_lowercase().as_str()) {
                break;
            }
            end = at + core.len();
            at += tok.len() + 1;
            if core.len() < tok.len() {
                break;
            }
        }
        if end > m.end() {
            push(m.start(), end, PatternKind::Birth, &mut hits);
        }
    }
    for (kind, re) in &p.list {
        for m in re.find_iter(text) {
            if *kind == PatternKind::Phone && m.as_str().chars().next().is_some_and(|c| c.is_ascii_digit() || c == '+' || c == '(') && !plausible_phone(m.as_str()) {
                continue;
            }
            if *kind == PatternKind::WebAddress && text[..m.start()].ends_with('@') {
                continue;
            }
            push(m.start(), m.end(), *kind, &mut hits);
        }
    }
    for (s, e) in id_tokens(text) {
        push(s, e, PatternKind::IdNumber, &mut hits);
    }
    hits.sort_by_key(|h| h.0);
    hits
}

fn context(text: &str, s: usize, e: usize) -> (String, String) {
    const N: usize = 60;
    let before = &text[..s];
    let after = &text[e..];
    // Cut at a space, so no word is shown in part.
    let b = if before.len() > N {
        let from = before.len() - N;
        let cut = before[from..].find(' ').map(|i| from + i + 1).unwrap_or(from);
        format!("\u{2026}{}", &before[before.char_indices().map(|(i, _)| i).find(|&i| i >= cut).unwrap_or(0)..])
    } else {
        before.to_string()
    };
    let a = if after.len() > N {
        let end = after.char_indices().map(|(i, _)| i).take_while(|&i| i <= N).last().unwrap_or(0);
        let cut = after[..end].rfind(' ').filter(|&i| i > 0).unwrap_or(end);
        format!("{}\u{2026}", &after[..cut])
    } else {
        after.to_string()
    };
    (b, a)
}

/// The pattern pass over one transcript. Texts the user skipped before (`ignored`, lower case) are left out.
pub fn flags(t: &Transcript, m: &MediaFile, ignored: &HashSet<String>) -> Vec<Flag> {
    let mut out = Vec::new();
    for (i, turn) in t.turns.iter().enumerate() {
        let text = turn.text();
        for (s, e, kind) in find_patterns(&text) {
            let found = &text[s..e];
            if ignored.contains(&found.to_lowercase()) || crate::edit::is_notation(found) {
                continue;
            }
            let (before, after) = context(&text, s, e);
            out.push(Flag { file: m.id.clone(), label: m.fields.label.clone(), turn: i, start: s, end: e, text: found.to_string(), kind, at: time_at(turn, s), before, after });
        }
    }
    out
}

/// Replace one flagged span. The flag must still match the transcript (the turn's text at that range is the flagged
/// text); otherwise nothing changes and `None` comes back.
pub fn apply_flag(t: &mut Transcript, f: &Flag, replacement: &str, by: &str) -> Option<LogEntry> {
    let turn = t.turns.get(f.turn)?;
    let text = turn.text();
    if text.get(f.start..f.end) != Some(f.text.as_str()) {
        return None;
    }
    set_turn_text(&mut t.turns, f.turn, &replace_ranges(&text, &[(f.start, f.end, replacement.to_string())]));
    Some(LogEntry {
        when: now(),
        file: f.file.clone(),
        label: f.label.clone(),
        place: hms(f.at),
        at: Some(f.at),
        original: f.text.clone(),
        replacement: replacement.to_string(),
        reason: format!("pattern: {}", f.kind.words()),
        by: by.to_string(),
    })
}

// ---------------------------------------------------------------------------------------------------------
// The suggestions pass
// ---------------------------------------------------------------------------------------------------------

/// Capitalised words that are seldom names, so the suggestions list stays short.
const NOT_NAMES: &[&str] = &[
    "i", "i'm", "i've", "i'll", "i'd", "ok", "okay", "mr", "mrs", "ms", "miss", "dr", "doctor", "professor", "prof", "sir", "madam", "god", "christmas", "easter",
    "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday", "january", "february", "march", "april", "may", "june", "july", "august",
    "september", "october", "november", "december", "english", "british", "american", "canadian", "australian", "french", "spanish", "german", "italian",
    "chinese", "japanese", "indian", "irish", "scottish", "welsh", "european", "african", "asian", "tv", "uk", "us", "usa", "eu", "covid", "internet",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Likelihood {
    /// A common word that is also a name ("Mark", "Will"), capitalised mid-sentence.
    Possible,
    /// Capitalised mid-sentence and not a dictionary word, or never seen in lower case.
    Likely,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub word: String,
    pub likelihood: Likelihood,
    pub count: usize,
    /// Labels of the files it is in.
    pub files: Vec<String>,
    /// Up to three places it was said: (file label, time, before, after).
    pub examples: Vec<(String, f64, String, String)>,
}

/// The word with its punctuation and a possessive taken off: "Maria's," gives "Maria".
fn bare_word(token: &str) -> &str {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric());
    t.strip_suffix("'s").or_else(|| t.strip_suffix("\u{2019}s")).unwrap_or(t)
}

/// The suggestions pass: capitalised words that may be names, across every transcript. `known` says whether a
/// lower-case word is in the dictionary (Windows' spell checker in the app); words on the project word list, on the
/// name list, or already replaced, are left out.
pub fn suggestions(files: &[(&MediaFile, &Transcript)], names: &[NameEntry], word_list: &[String], known: &dyn Fn(&str) -> bool) -> Vec<Suggestion> {
    let mut skip: HashSet<String> = NOT_NAMES.iter().map(|s| s.to_string()).collect();
    skip.extend(word_list.iter().map(|w| w.trim().to_lowercase()));
    for n in names {
        for s in std::iter::once(&n.real).chain(&n.variants).chain(std::iter::once(&n.replacement)) {
            skip.extend(s.split_whitespace().map(|w| bare_word(w).to_lowercase()));
        }
    }
    // Words also said in lower case somewhere are ordinary words more often than names.
    let mut lower_seen: HashSet<String> = HashSet::new();
    for (_, t) in files {
        for turn in &t.turns {
            for w in &turn.words {
                let b = bare_word(&w.text);
                if b.chars().next().is_some_and(|c| c.is_lowercase()) {
                    lower_seen.insert(b.to_string());
                }
            }
        }
    }
    let mut found: BTreeMap<String, Suggestion> = BTreeMap::new();
    let mut dict: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    for (m, t) in files {
        for turn in &t.turns {
            let text = turn.text();
            let offs = word_offsets(turn);
            for (i, w) in turn.words.iter().enumerate() {
                // Notation and replacements ("[College]", "[College].edu") are not names.
                if crate::edit::is_notation(&w.text) || w.text.contains(['[', ']']) {
                    continue;
                }
                let b = bare_word(&w.text);
                let lower = b.to_lowercase();
                if b.chars().count() < 2 || !b.chars().next().is_some_and(|c| c.is_uppercase()) || b.chars().any(|c| c.is_ascii_digit()) || skip.contains(&lower) {
                    continue;
                }
                let sentence_start = i == 0 || turn.words[i - 1].text.ends_with(['.', '?', '!', ':']) || crate::edit::is_notation(&turn.words[i - 1].text);
                let in_dict = *dict.entry(lower.clone()).or_insert_with(|| known(&lower));
                let likelihood = match (sentence_start, in_dict || lower_seen.contains(&lower)) {
                    (true, true) => continue,
                    (_, false) => Likelihood::Likely,
                    (false, true) => Likelihood::Possible,
                };
                let s = found.entry(lower).or_insert_with(|| Suggestion { word: b.to_string(), likelihood, count: 0, files: Vec::new(), examples: Vec::new() });
                s.likelihood = s.likelihood.max(likelihood);
                s.count += 1;
                if !s.files.contains(&m.fields.label) {
                    s.files.push(m.fields.label.clone());
                }
                if s.examples.len() < 3 {
                    // The word itself inside its token, so "Keith's," shows as before + Keith + "'s, ...".
                    let start = offs[i] + w.text.find(b).unwrap_or(0);
                    let (before, after) = context(&text, start, start + b.len());
                    s.examples.push((m.fields.label.clone(), w.start, before, after));
                }
            }
        }
    }
    let mut out: Vec<Suggestion> = found.into_values().collect();
    out.sort_by(|a, b| b.likelihood.cmp(&a.likelihood).then(b.count.cmp(&a.count)).then_with(|| a.word.cmp(&b.word)));
    out
}

// ---------------------------------------------------------------------------------------------------------
// The final check
// ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Leftover {
    /// "00:12:03", "speaker name", "attribute site", "file label", "interviewer", "note".
    pub place: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileCheck {
    pub file: String,
    pub label: String,
    pub review: ReviewStatus,
    pub transcribed: bool,
    /// Names on the list still found in what an export carries. Empty means the file passes.
    pub leftovers: Vec<Leftover>,
    /// Snapshots (never exported) that still hold names on the list: the transcript as it was before.
    pub snapshots_with_names: usize,
}

impl FileCheck {
    pub fn passes(&self) -> bool {
        self.leftovers.is_empty()
    }
}

/// The final check for one file (T6): every name and spelling on the list, everywhere an export reads.
pub fn check_file(m: &MediaFile, t: Option<&Transcript>, names: &[NameEntry]) -> FileCheck {
    let mut left = Vec::new();
    let look = |place: &str, s: &str, at: Option<f64>, left: &mut Vec<Leftover>| {
        for (a, b, _) in find_names(s, names) {
            left.push(Leftover { place: place.to_string(), text: s[a..b].to_string(), at });
        }
    };
    look("file label", &m.fields.label, None, &mut left);
    for (place, v) in [("interviewer", &m.fields.interviewer), ("note", &m.fields.note)] {
        if let Some(v) = v {
            look(place, v, None, &mut left);
        }
    }
    let mut snapshots = 0;
    if let Some(t) = t {
        for turn in &t.turns {
            let text = turn.text();
            for (a, b, _) in find_names(&text, names) {
                let at = time_at(turn, a);
                left.push(Leftover { place: hms(at), text: text[a..b].to_string(), at: Some(at) });
            }
        }
        for s in &t.speakers {
            if let Some(n) = &s.name {
                look("speaker name", n, None, &mut left);
            }
            for (k, v) in &s.attributes {
                look(&format!("attribute {k}"), v, None, &mut left);
            }
        }
        snapshots = t.snapshots.iter().filter(|s| s.turns.iter().any(|x| !find_names(&x.text(), names).is_empty())).count();
    }
    FileCheck { file: m.id.clone(), label: m.fields.label.clone(), review: t.map(|t| t.review).unwrap_or_default(), transcribed: t.is_some(), leftovers: left, snapshots_with_names: snapshots }
}

/// Whether a file's review status says it was de-identified.
pub fn is_deidentified(r: ReviewStatus) -> bool {
    matches!(r, ReviewStatus::DeIdentified | ReviewStatus::Final)
}

// ---------------------------------------------------------------------------------------------------------
// The key file
// ---------------------------------------------------------------------------------------------------------

pub const KEY_FORMAT: &str = "Prism Transcribe key file";
pub const KEY_SCHEMA_VERSION: u32 = 1;
/// The suggested file ending.
pub const KEY_EXTENSION: &str = "prismkey";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyFile {
    pub format: String,
    pub schema_version: u32,
    /// The project it belongs to (`ProjectFile::id`), and its name when the key was made.
    pub project_id: String,
    pub project_name: String,
    pub created: String,
    #[serde(default)]
    pub names: Vec<NameEntry>,
    /// Flagged texts the user skipped (lower case), so the pattern pass does not raise them again.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub ignored: BTreeSet<String>,
    #[serde(default)]
    pub log: Vec<LogEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("{0} is not a Prism Transcribe key file")]
    NotAKey(String),
    #[error("this key file belongs to another project ({0})")]
    OtherProject(String),
    #[error("this key file was written by a newer Prism Transcribe (schema {0}); update the app to open it")]
    NewerSchema(u32),
    #[error("{0}")]
    Project(#[from] ProjectError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("the key file is damaged: {0}")]
    Json(#[from] serde_json::Error),
}

impl KeyFile {
    pub fn new(p: &Project) -> Self {
        KeyFile { format: KEY_FORMAT.into(), schema_version: KEY_SCHEMA_VERSION, project_id: p.file.id.clone(), project_name: p.file.name.clone(), created: now(), names: Vec::new(), ignored: BTreeSet::new(), log: Vec::new() }
    }

    /// Read a project's key file; it is sealed with the project's password when the project has one.
    pub fn load(path: &Path, p: &Project) -> Result<Self, KeyError> {
        let bytes = p.read_maybe_sealed(path)?;
        let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| KeyError::NotAKey(path.display().to_string()))?;
        if v.get("format").and_then(|f| f.as_str()) != Some(KEY_FORMAT) {
            return Err(KeyError::NotAKey(path.display().to_string()));
        }
        let version = v.get("schema_version").and_then(|n| n.as_u64()).unwrap_or(0) as u32;
        if version > KEY_SCHEMA_VERSION {
            return Err(KeyError::NewerSchema(version));
        }
        let k: KeyFile = serde_json::from_value(v)?;
        if k.project_id != p.file.id {
            return Err(KeyError::OtherProject(k.project_name));
        }
        Ok(k)
    }

    /// Write it, sealed with the project's password when the project has one.
    pub fn save(&self, path: &Path, p: &Project) -> Result<(), KeyError> {
        let json = serde_json::to_string_pretty(self)? + "\n";
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        crate::project::write_atomic(path, &p.seal_if_protected(json.as_bytes())?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Speaker, Turn, Word};

    fn media(id: &str, label: &str) -> MediaFile {
        serde_json::from_value(serde_json::json!({"id": id, "label": label, "source": "x.wav", "copied": false, "sha256": "", "bytes": 0, "imported": ""})).unwrap()
    }

    fn transcript(lines: &[(&str, &str)]) -> Transcript {
        let mut t = Transcript::new("F001");
        let mut at = 0.0;
        t.speakers = vec![
            Speaker { id: "S1".into(), label: "Speaker 1".into(), name: Some("Interviewer".into()), role: Default::default(), attributes: Default::default() },
            Speaker { id: "S2".into(), label: "Speaker 2".into(), name: Some("Maria".into()), role: Default::default(), attributes: [("site".to_string(), "Riverside clinic".to_string())].into() },
        ];
        for (sp, text) in lines {
            let words: Vec<Word> = text
                .split_whitespace()
                .map(|w| {
                    let x = Word { text: w.into(), start: at, end: at + 0.4, confidence: 0.9, speaker: sp.to_string() };
                    at += 0.5;
                    x
                })
                .collect();
            t.turns.push(Turn { speaker: sp.to_string(), start: words[0].start, end: words.last().unwrap().end, words });
            at += 1.0;
        }
        t
    }

    fn entry(real: &str, rep: &str) -> NameEntry {
        NameEntry { real: real.into(), variants: auto_variants(real, NameKind::Person), replacement: rep.into(), kind: NameKind::Person, source: NameSource::Typed }
    }

    #[test]
    fn a_roster_in_its_usual_shapes() {
        let r = parse_roster("Maria Lopez\tP07\nJohn Smith = P08\nLopez, Ana\nRiverside, place\n\nmaria lopez\nAcme Ltd -> [Employer]\nKeith", &[]);
        let got: Vec<(&str, &str, NameKind)> = r.iter().map(|n| (n.real.as_str(), n.replacement.as_str(), n.kind)).collect();
        assert_eq!(
            got,
            [
                ("Maria Lopez", "P07", NameKind::Person),
                ("John Smith", "P08", NameKind::Person),
                ("Ana Lopez", "[Person 3]", NameKind::Person),
                ("Riverside", "[Place]", NameKind::Place),
                ("Acme Ltd", "[Employer]", NameKind::Organisation),
                ("Keith", "[Person 4]", NameKind::Person),
            ]
        );
        assert_eq!(r[0].variants, ["Maria", "Lopez"]);
        assert!(r[3].variants.is_empty(), "places get no automatic spellings");
        assert_eq!(auto_variants("Ana de Souza", NameKind::Person), ["Ana", "Souza"]);
        assert!(parse_roster("MARIA LOPEZ", &r).is_empty(), "already on the list");
    }

    /// The manual's cases (2026-11-18): surname first with a replacement, and a kind after an arrow.
    #[test]
    fn a_roster_with_the_surname_first_and_a_kind_after_the_replacement() {
        let r = parse_roster("Okafor, Dana = P08\nGarcia, Ana = P21\nGarcia, Luis\tP22\nRiverside College -> [College] organisation\nNorth Clinic -> [Site], place\nKim, Jo, P23", &[]);
        let got: Vec<(&str, &str, NameKind)> = r.iter().map(|n| (n.real.as_str(), n.replacement.as_str(), n.kind)).collect();
        assert_eq!(
            got,
            [
                ("Dana Okafor", "P08", NameKind::Person),
                ("Ana Garcia", "P21", NameKind::Person),
                ("Luis Garcia", "P22", NameKind::Person),
                ("Riverside College", "[College]", NameKind::Organisation),
                ("North Clinic", "[Site]", NameKind::Place),
                ("Jo Kim", "P23", NameKind::Person),
            ]
        );
        assert_eq!(r[0].variants, ["Dana", "Okafor"]);
    }

    /// The manual's cases: a replacement holding a word of the name is not a name left, an address goes whole, and
    /// an initial goes with the name.
    #[test]
    fn names_in_addresses_initials_and_replacements() {
        let college = NameEntry { real: "Riverside College".into(), variants: vec!["Riverside".into(), "College".into()], replacement: "[College]".into(), kind: NameKind::Organisation, source: NameSource::Typed };
        let dana = NameEntry { real: "Dana Okafor".into(), variants: vec!["Dana".into(), "Okafor".into(), "Akafer".into()], replacement: "P08".into(), kind: NameKind::Person, source: NameSource::Typed };
        let names = vec![college, dana];
        let mut t = transcript(&[("S2", "Write to D Akafer at Riverside.edu, or d.okafor@riverside.edu. I teach at Riverside College."), ("S1", "Thanks, D. Okafor.")]);
        let m = media("F001", "INT-08");
        apply_names(&mut t, &m, &names, "jo");
        assert_eq!(t.turns[0].text(), "Write to P08 at [College], or P08. I teach at [College].");
        assert_eq!(t.turns[1].text(), "Thanks, P08.");
        let c = check_file(&m, Some(&t), &names);
        assert!(c.passes(), "[College] is the replacement, not a name left: {:?}", c.leftovers);
        assert_eq!(find_names("I asked A Okafor and I Okafor.", &names).len(), 2);
        assert_eq!(find_names("Ask I Okafor", &names)[0].0, 6, "I is a word, not an initial");
    }

    #[test]
    fn names_are_replaced_whole_word_any_case_and_possessive() {
        let names = vec![entry("Maria Lopez", "P07"), NameEntry { real: "Riverside".into(), variants: vec![], replacement: "[Place]".into(), kind: NameKind::Place, source: NameSource::Typed }];
        let mut t = transcript(&[("S1", "Hello Maria Lopez, welcome."), ("S2", "Thanks. maria's fine, Mariana too. I work at Riverside clinic, ask MARIA.")]);
        let m = media("F001", "INT-07");
        assert_eq!(count_names(&t, &names), 6);
        let log = apply_names(&mut t, &m, &names, "jason");
        assert_eq!(t.turns[0].text(), "Hello P07, welcome.");
        assert_eq!(t.turns[1].text(), "Thanks. P07's fine, Mariana too. I work at [Place] clinic, ask P07.");
        assert_eq!(t.speakers[1].name.as_deref(), Some("P07"));
        assert_eq!(t.speakers[1].attributes["site"], "[Place] clinic");
        assert_eq!(log.len(), 6);
        assert_eq!((log[0].original.as_str(), log[0].replacement.as_str(), log[0].place.as_str(), log[0].by.as_str()), ("Maria Lopez", "P07", "00:00:00", "jason"));
        assert_eq!(log[1].original, "maria");
        assert!(log.iter().any(|l| l.place == "attribute site"));
        // The replacement keeps the time span of the two words it replaced.
        assert_eq!((t.turns[0].words[1].start, t.turns[0].words[1].end), (0.5, 1.4));
        assert!(check_file(&m, Some(&t), &names).passes());
        assert_eq!(count_names(&t, &names), 0);
    }

    #[test]
    fn the_final_check_looks_everywhere_an_export_reads() {
        let names = vec![entry("Maria Lopez", "P07")];
        let mut m = media("F001", "Lopez interview");
        m.fields.interviewer = Some("Jo".into());
        let mut t = transcript(&[("S1", "So Maria, tell me.")]);
        t.snapshots.push(crate::model::Snapshot { name: "Version 0".into(), created: String::new(), turns: t.turns.clone() });
        let c = check_file(&m, Some(&t), &names);
        let places: Vec<&str> = c.leftovers.iter().map(|l| l.place.as_str()).collect();
        assert_eq!(places, ["file label", "00:00:00", "speaker name"]);
        assert!(!c.passes());
        apply_names(&mut t, &m, &names, "x");
        m.fields.label = "INT-07".into();
        let c = check_file(&m, Some(&t), &names);
        assert!(c.passes(), "{:?}", c.leftovers);
        assert_eq!(c.snapshots_with_names, 1, "the earlier version still has it, and is never exported");
    }

    #[test]
    fn the_pattern_pass() {
        let kinds = |s: &str| find_patterns(s).into_iter().map(|(a, b, k)| (s[a..b].to_string(), k)).collect::<Vec<_>>();
        use PatternKind::*;
        assert_eq!(kinds("Email me at maria.lopez@example.org, or call 07700 900123."), [("maria.lopez@example.org".to_string(), Email), ("07700 900123".to_string(), Phone)]);
        assert_eq!(kinds("It's maria dot lopez at gmail dot com."), [("maria dot lopez at gmail dot com".to_string(), Email)]);
        assert_eq!(kinds("See www.example.org/about or riverside-clinic.co.uk."), [("www.example.org/about".to_string(), WebAddress), ("riverside-clinic.co.uk".to_string(), WebAddress)]);
        assert_eq!(kinds("Call +1 (555) 010-4477 today"), [("+1 (555) 010-4477".to_string(), Phone)]);
        assert_eq!(kinds("It is five five five zero one zero four four."), [("five five five zero one zero four four".to_string(), Phone)]);
        assert_eq!(kinds("I live at 12 Elm Street, SW1A 1AA."), [("12 Elm Street".to_string(), Address), ("SW1A 1AA".to_string(), Address)]);
        assert_eq!(kinds("I was born on 3 March 1985 and moved 04/07/1999."), [("born on 3 March 1985".to_string(), Birth), ("04/07/1999".to_string(), Date)]);
        assert_eq!(kinds("She was born in Leeds, I think."), [("born in Leeds".to_string(), Birth)]);
        assert_eq!(kinds("It was 3rd of March 1985 and 2001-09-11."), [("3rd of March 1985".to_string(), Date), ("2001-09-11".to_string(), Date)]);
        assert_eq!(kinds("My number is 123-45-6789 and card AB123456C."), [("123-45-6789".to_string(), IdNumber), ("AB123456C".to_string(), IdNumber)]);
        // Ordinary numbers stay quiet: years, money, counts, times.
        assert!(kinds("From 1998 2004 2010, about 1,000,000 people, $250000, at 10.30 or 4 times.").is_empty(), "{:?}", kinds("From 1998 2004 2010, about 1,000,000 people, $250000, at 10.30 or 4 times."));
    }

    #[test]
    fn a_flag_is_applied_only_where_it_still_matches() {
        let mut t = transcript(&[("S2", "Call me on 07700 900123 tonight.")]);
        let m = media("F001", "INT-07");
        let f = flags(&t, &m, &HashSet::new());
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].before.as_str(), f[0].after.as_str()), ("Call me on ", " tonight."));
        let mut stale = f[0].clone();
        stale.text = "07700 900124".into();
        assert!(apply_flag(&mut t, &stale, "[phone number]", "x").is_none());
        let log = apply_flag(&mut t, &f[0], "[phone number]", "x").unwrap();
        assert_eq!(t.turns[0].text(), "Call me on [phone number] tonight.");
        assert_eq!(log.reason, "pattern: phone number");
        assert!(flags(&t, &m, &HashSet::new()).is_empty());
        let t2 = transcript(&[("S2", "Call me on 07700 900123 tonight.")]);
        assert!(flags(&t2, &m, &["07700 900123".to_string()].into()).is_empty(), "skipped before");
    }

    #[test]
    fn suggestions_list_capitalised_words_that_may_be_names() {
        let t = transcript(&[("S1", "So how did Keith and Mark get on at Brookfield? Brookfield is far."), ("S2", "Keith said it will mark a change. Sunday we met Dr Patel. Patel's lovely. I think so.")]);
        let m = media("F001", "INT-07");
        let dict = ["so", "how", "did", "and", "mark", "get", "on", "at", "is", "far", "said", "it", "will", "a", "change", "sunday", "we", "met", "think", "lovely"];
        let known = |w: &str| dict.contains(&w);
        let s = suggestions(&[(&m, &t)], &[entry("Ana Lopez", "P07")], &["Brookfield".into()], &known);
        let got: Vec<(&str, Likelihood, usize)> = s.iter().map(|x| (x.word.as_str(), x.likelihood, x.count)).collect();
        assert_eq!(got, [("Keith", Likelihood::Likely, 2), ("Patel", Likelihood::Likely, 2), ("Mark", Likelihood::Possible, 1)]);
        assert_eq!(s[0].examples[0].2, "So how did ");
    }

    #[test]
    fn the_key_file_round_trips_and_knows_its_project() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Project::create(&tmp.path().join("proj"), "Study A").unwrap();
        let mut k = KeyFile::new(&p);
        k.names.push(entry("Maria Lopez", "P07"));
        k.ignored.insert("07700 900123".into());
        let path = tmp.path().join("keys").join("Study A.prismkey");
        k.save(&path, &p).unwrap();
        assert_eq!(KeyFile::load(&path, &p).unwrap(), k);
        let other = Project::create(&tmp.path().join("other"), "Study B").unwrap();
        assert!(matches!(KeyFile::load(&path, &other), Err(KeyError::OtherProject(n)) if n == "Study A"));
        std::fs::write(&path, b"{}").unwrap();
        assert!(matches!(KeyFile::load(&path, &p), Err(KeyError::NotAKey(_))));
    }
}
