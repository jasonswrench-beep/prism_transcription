//! Editing rules shared by the editor and project-wide find and replace.
//!
//! **Word times survive edits (T5).** When a turn's text changes, the new words are lined up against the old ones
//! (longest common subsequence on the exact text): a word that is unchanged keeps its times and confidence; words
//! that replace other words share the replaced words' time span evenly; newly typed words with nothing replaced are
//! spread across the gap between their neighbours. Edited and typed words get confidence 1.0, because a person
//! wrote them.
//!
//! **Notation** ([laughter], [inaudible 00:12:03], [unclear: best guess]) is typed inline and kept as one token,
//! brackets and spaces included.
//!
//! **The clean view** hides fillers (um, uh, you know, repeated words) without deleting them (T4); clean exports
//! use the same rule.

use crate::model::{NotationTag, Transcript, Turn, Word};

/// Split turn text into tokens: a bracketed span (notation) is one token, anything else splits on white space.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '[' => {
                if depth == 0 && !cur.is_empty() && !cur.ends_with(char::is_whitespace) {
                    // "word[note]" stays one token only if the user wrote it that way; keep it simple: split.
                    out.push(std::mem::take(&mut cur));
                }
                depth += 1;
                cur.push(c);
            }
            ']' if depth > 0 => {
                depth -= 1;
                cur.push(c);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(if c.is_whitespace() { ' ' } else { c }),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// A notation token: starts with '[' and ends with ']' (trailing punctuation allowed: "[laughter].").
pub fn is_notation(token: &str) -> bool {
    let t = token.trim_end_matches(|c: char| c.is_ascii_punctuation() && c != ']');
    t.starts_with('[') && t.ends_with(']')
}

/// The notation kind: "laughter" from "[laughter]", "inaudible" from "[inaudible 00:12:03]", "unclear" from
/// "[unclear: best guess]", "timestamp" from "[00:12:03]".
pub fn notation_kind(token: &str) -> String {
    let inner = token.trim_end_matches(|c: char| c.is_ascii_punctuation() && c != ']').trim_start_matches('[').trim_end_matches(']').trim();
    if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit() || c == ':' || c == '.') {
        return "timestamp".into();
    }
    inner.split(|c: char| c == ':' || c.is_whitespace()).next().unwrap_or("").to_lowercase()
}

fn lcs_pairs(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![0u32; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * (m + 1) + j] = if a[i] == b[j] { dp[(i + 1) * (m + 1) + j + 1] + 1 } else { dp[(i + 1) * (m + 1) + j].max(dp[i * (m + 1) + j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if dp[(i + 1) * (m + 1) + j] >= dp[i * (m + 1) + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

fn spread(tokens: &[String], from: f64, to: f64, speaker: &str, out: &mut Vec<Word>) {
    let n = tokens.len().max(1) as f64;
    let to = to.max(from);
    let step = (to - from) / n;
    for (k, t) in tokens.iter().enumerate() {
        out.push(Word { text: t.clone(), start: from + step * k as f64, end: from + step * (k + 1) as f64, confidence: 1.0, speaker: speaker.into() });
    }
}

/// New words for a turn whose text became `text`. `lo` and `hi` bound typed words that have no neighbour on one side
/// (normally the end of the turn before and the start of the turn after).
pub fn retime(old: &[Word], text: &str, speaker: &str, lo: f64, hi: f64) -> Vec<Word> {
    let new = tokenize(text);
    let a: Vec<&str> = old.iter().map(|w| w.text.as_str()).collect();
    let b: Vec<&str> = new.iter().map(String::as_str).collect();
    let anchors = lcs_pairs(&a, &b);
    let mut out = Vec::with_capacity(new.len());
    let (mut oi, mut ni) = (0usize, 0usize);
    let mut prev_end = lo;
    for &(ai, bi) in anchors.iter().chain(std::iter::once(&(old.len(), new.len()))) {
        let old_run = &old[oi..ai];
        let new_run = &new[ni..bi];
        if !new_run.is_empty() {
            if let (Some(f), Some(l)) = (old_run.first(), old_run.last()) {
                spread(new_run, f.start, l.end, speaker, &mut out);
            } else {
                let next_start = if ai < old.len() { old[ai].start } else { hi.max(prev_end) };
                spread(new_run, prev_end, next_start, speaker, &mut out);
            }
        }
        if ai < old.len() {
            let mut w = old[ai].clone();
            w.speaker = speaker.into();
            prev_end = w.end;
            out.push(w);
        } else if let Some(l) = old_run.last() {
            prev_end = prev_end.max(l.end);
        }
        oi = ai + 1;
        ni = bi + 1;
    }
    out
}

/// Replace a turn's text, keeping times by [`retime`]. The turn's start and end follow its words.
pub fn set_turn_text(turns: &mut [Turn], index: usize, text: &str) {
    let lo = if index > 0 { turns[index - 1].end.min(turns[index].start) } else { 0.0f64.min(turns[index].start) };
    let hi = turns.get(index + 1).map(|t| t.start.max(turns[index].end)).unwrap_or(turns[index].end + 2.0);
    let t = &mut turns[index];
    let (lo, hi) = if t.words.is_empty() { (t.start.max(lo), t.end.max(t.start)) } else { (lo, hi) };
    t.words = retime(&t.words, text, &t.speaker, lo, hi);
    if let (Some(f), Some(l)) = (t.words.first(), t.words.last()) {
        t.start = f.start;
        t.end = l.end;
    }
}

/// Rebuild the notation list from the bracketed words, for exports that want it apart from the text.
pub fn refresh_notation(t: &mut Transcript) {
    t.notation = t
        .turns
        .iter()
        .enumerate()
        .flat_map(|(ti, turn)| {
            turn.words.iter().enumerate().filter(|(_, w)| is_notation(&w.text)).map(move |(wi, w)| NotationTag { kind: notation_kind(&w.text), time: w.start, turn: ti, word: wi, note: Some(w.text.clone()) })
        })
        .collect();
}

// ---------------------------------------------------------------------------------------------------------
// Find and replace
// ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct FindOptions {
    #[serde(default)]
    pub whole_word: bool,
    #[serde(default)]
    pub match_case: bool,
}

fn pattern(find: &str, o: &FindOptions) -> Option<regex::Regex> {
    if find.is_empty() {
        return None;
    }
    // A literal search; whole words are checked by hand in `find_in`, which also handles "maria's".
    regex::RegexBuilder::new(&regex::escape(find)).case_insensitive(!o.match_case).build().ok()
}

/// For whole-word matching an apostrophe is a boundary, so "Maria" matches in "Maria's" (T6: possessive aware).
fn is_word_char(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// Where `find` occurs in `text` (byte ranges).
pub fn find_in(text: &str, find: &str, o: &FindOptions) -> Vec<(usize, usize)> {
    let Some(re) = pattern(find, o) else { return Vec::new() };
    re.find_iter(text)
        .filter(|m| {
            !o.whole_word || {
                let before = text[..m.start()].chars().next_back();
                let after = text[m.end()..].chars().next();
                let first = m.as_str().chars().next();
                let last = m.as_str().chars().next_back();
                (!is_word_char(first) || !is_word_char(before)) && (!is_word_char(last) || !is_word_char(after))
            }
        })
        .map(|m| (m.start(), m.end()))
        .collect()
}

/// Replace every occurrence in every turn, keeping word times (T5). Returns how many were replaced.
pub fn replace_all(t: &mut Transcript, find: &str, replace: &str, o: &FindOptions) -> usize {
    let mut count = 0;
    for i in 0..t.turns.len() {
        let text = t.turns[i].text();
        let hits = find_in(&text, find, o);
        if hits.is_empty() {
            continue;
        }
        count += hits.len();
        let mut new = String::with_capacity(text.len());
        let mut at = 0;
        for (s, e) in hits {
            new.push_str(&text[at..s]);
            new.push_str(replace);
            at = e;
        }
        new.push_str(&text[at..]);
        set_turn_text(&mut t.turns, i, &new);
    }
    count
}

// ---------------------------------------------------------------------------------------------------------
// Clean view
// ---------------------------------------------------------------------------------------------------------

const FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "uhm", "er", "erm", "ah", "hmm", "mm"];

fn bare(t: &str) -> String {
    t.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect::<String>().to_lowercase()
}

/// For each word, whether the clean view shows it. Hidden: fillers (um, uh ...), "you know" set off as an aside
/// (followed by a comma, or ending the turn), and the first of a word said twice in a row ("I I think"). Notation
/// is always shown.
pub fn clean_mask(words: &[Word]) -> Vec<bool> {
    let n = words.len();
    let mut keep = vec![true; n];
    for i in 0..n {
        let w = &words[i].text;
        if is_notation(w) {
            continue;
        }
        let b = bare(w);
        if FILLERS.contains(&b.as_str()) {
            keep[i] = false;
            continue;
        }
        if b == "you" && i + 1 < n && bare(&words[i + 1].text) == "know" {
            let know = &words[i + 1].text;
            if know.ends_with(',') || know.ends_with('.') || i + 2 == n || words[i].text.ends_with(',') || (i > 0 && words[i - 1].text.ends_with(',')) {
                keep[i] = false;
                keep[i + 1] = false;
            }
        }
        if i + 1 < n && !b.is_empty() && bare(&words[i + 1].text) == b && !w.ends_with(['.', '?', '!']) {
            keep[i] = false;
        }
    }
    keep
}

/// A turn's clean text.
pub fn clean_text(words: &[Word]) -> String {
    let keep = clean_mask(words);
    let shown: Vec<&str> = words.iter().zip(&keep).filter(|(_, k)| **k).map(|(w, _)| w.text.as_str()).collect();
    let mut s = shown.join(" ");
    // "Um, I think" loses its filler; capitalise what now starts the turn.
    if let Some(first) = s.chars().next() {
        if keep.first() == Some(&false) && first.is_lowercase() {
            s = first.to_uppercase().collect::<String>() + &s[first.len_utf8()..];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(spec: &[(&str, f64, f64)]) -> Vec<Word> {
        spec.iter().map(|(t, a, b)| Word { text: t.to_string(), start: *a, end: *b, confidence: 0.8, speaker: "S1".into() }).collect()
    }

    fn texts(w: &[Word]) -> Vec<&str> {
        w.iter().map(|x| x.text.as_str()).collect()
    }

    #[test]
    fn tokens_keep_notation_whole() {
        assert_eq!(tokenize("  so [unclear: best guess] went [laughter]. ok "), ["so", "[unclear: best guess]", "went", "[laughter].", "ok"]);
        assert!(is_notation("[laughter]."));
        assert_eq!(notation_kind("[inaudible 00:12:03]"), "inaudible");
        assert_eq!(notation_kind("[unclear: best guess]"), "unclear");
        assert_eq!(notation_kind("[00:12:03]"), "timestamp");
    }

    #[test]
    fn unchanged_words_keep_their_times_and_confidence() {
        let old = words(&[("I", 1.0, 1.2), ("went", 1.2, 1.5), ("home", 1.6, 2.0)]);
        let new = retime(&old, "I went home", "S1", 0.0, 3.0);
        assert_eq!(new, old);
    }

    #[test]
    fn a_replaced_word_keeps_the_span_of_what_it_replaced() {
        let old = words(&[("I", 1.0, 1.2), ("wnt", 1.2, 1.5), ("home", 1.6, 2.0)]);
        let new = retime(&old, "I went back home", "S1", 0.0, 3.0);
        assert_eq!(texts(&new), ["I", "went", "back", "home"]);
        // "went back" shares 1.2..1.5, evenly; typed words are fully confident.
        assert!((new[1].start - 1.2).abs() < 1e-9 && (new[1].end - 1.35).abs() < 1e-9);
        assert!((new[2].start - 1.35).abs() < 1e-9 && (new[2].end - 1.5).abs() < 1e-9);
        assert_eq!(new[1].confidence, 1.0);
        assert_eq!(new[3], old[2]);
    }

    #[test]
    fn typed_words_spread_across_the_gap() {
        let old = words(&[("Yes", 1.0, 1.3), ("now", 2.3, 2.6)]);
        let new = retime(&old, "Yes [pause] now", "S1", 0.0, 3.0);
        assert_eq!(texts(&new), ["Yes", "[pause]", "now"]);
        assert!((new[1].start - 1.3).abs() < 1e-9 && (new[1].end - 2.3).abs() < 1e-9, "fills the silence between");
        // At the ends, the bounds given are used.
        let new = retime(&old, "Well Yes now", "S1", 0.4, 3.0);
        assert!((new[0].start - 0.4).abs() < 1e-9 && (new[0].end - 1.0).abs() < 1e-9);
        let new = retime(&old, "Yes now indeed", "S1", 0.0, 3.0);
        assert!((new[2].start - 2.6).abs() < 1e-9 && (new[2].end - 3.0).abs() < 1e-9);
    }

    #[test]
    fn deleting_and_retyping_from_empty() {
        let old = words(&[("a", 1.0, 1.1), ("b", 1.1, 1.2)]);
        assert!(retime(&old, "   ", "S1", 0.0, 3.0).is_empty());
        let new = retime(&[], "one two", "S2", 5.0, 6.0);
        assert_eq!(texts(&new), ["one", "two"]);
        assert_eq!((new[0].start, new[1].end), (5.0, 6.0));
        assert!(new.iter().all(|w| w.speaker == "S2"));
    }

    #[test]
    fn times_always_go_forward_after_edits() {
        let old = words(&[("the", 0.0, 0.2), ("cat", 0.2, 0.5), ("sat", 0.5, 0.8), ("on", 0.8, 0.9), ("mat", 1.0, 1.4)]);
        for text in ["a cat sat on the mat", "the dog sat", "sat on cat the mat", "x y z", "the cat [laughter] sat on a mat."] {
            let new = retime(&old, text, "S1", 0.0, 2.0);
            assert!(new.windows(2).all(|p| p[0].start <= p[1].start && p[0].end <= p[1].end + 1e-9), "{text}: {new:?}");
            assert!(new.iter().all(|w| w.end >= w.start));
        }
    }

    #[test]
    fn find_and_replace() {
        let o = FindOptions::default();
        assert_eq!(find_in("Maria met maria's friend, Mariana.", "maria", &o).len(), 3);
        let ww = FindOptions { whole_word: true, ..Default::default() };
        assert_eq!(find_in("Maria met maria's friend, Mariana.", "maria", &ww), [(0, 5), (10, 15)], "possessive counts, Mariana does not");
        let mc = FindOptions { whole_word: true, match_case: true };
        assert_eq!(find_in("Maria met maria's friend.", "Maria", &mc), [(0, 5)]);

        let mut t = crate::model::tests::sample();
        let n = replace_all(&mut t, "Maria", "P07", &ww);
        assert_eq!(n, 1);
        assert_eq!(t.turns[0].text(), "Hello, P07.");
        assert_eq!((t.turns[0].words[1].start, t.turns[0].words[1].end), (0.9, 1.4), "the replacement keeps the name's time");
    }

    #[test]
    fn clean_view_hides_fillers_without_deleting() {
        let w = words(&[("Um,", 0.0, 0.1), ("I", 0.1, 0.2), ("I", 0.2, 0.3), ("think,", 0.3, 0.4), ("you", 0.4, 0.5), ("know,", 0.5, 0.6), ("it", 0.6, 0.7), ("uh", 0.7, 0.8), ("works", 0.8, 0.9), ("[laughter]", 0.9, 1.0)]);
        assert_eq!(clean_text(&w), "I think, it works [laughter]");
        assert_eq!(w.len(), 10, "nothing was deleted");
        // "you know" as ordinary words stays.
        let w = words(&[("Do", 0.0, 0.1), ("you", 0.1, 0.2), ("know", 0.2, 0.3), ("her?", 0.3, 0.4)]);
        assert_eq!(clean_text(&w), "Do you know her?");
    }
}
