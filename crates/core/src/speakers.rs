//! Speakers (T3): rename, merge, split a turn, reassign a turn or a stretch of it, and the counts the editor shows.
//!
//! Rules every operation keeps:
//! - a word's `speaker` is always its turn's speaker;
//! - a turn's `start` and `end` are its first word's start and its last word's end;
//! - two turns of the same speaker that end up side by side because of an operation are joined into one (turns
//!   the user split on purpose and did not touch again stay apart);
//! - a speaker who no longer has any turn stays in the list (with a count of 0) until removed, so a name and its
//!   attributes are not lost by an undoable mistake.

use crate::model::{Role, Speaker, SpeakerId, Turn};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::Range;

/// Make a turn agree with its words: their speaker, its start and end.
fn settle(t: &mut Turn) {
    for w in &mut t.words {
        w.speaker = t.speaker.clone();
    }
    if let (Some(a), Some(b)) = (t.words.first(), t.words.last()) {
        t.start = a.start;
        t.end = t.words.iter().map(|w| w.end).fold(b.end, f64::max);
    }
}

/// Join neighbours of the same speaker across the boundaries between turns `lo..=hi` (indices before joining).
fn tidy(turns: &mut Vec<Turn>, lo: usize, hi: usize) {
    let mut hi = hi.min(turns.len().saturating_sub(1));
    let mut k = lo + 1;
    while k <= hi && k < turns.len() {
        if turns[k - 1].speaker == turns[k].speaker {
            let t = turns.remove(k);
            turns[k - 1].words.extend(t.words);
            settle(&mut turns[k - 1]);
            hi -= 1;
        } else {
            k += 1;
        }
    }
}

/// Split turn `i` before word `at`, keeping the speaker. Nothing happens at either end of the turn.
pub fn split_turn(turns: &mut Vec<Turn>, i: usize, at: usize) -> bool {
    let Some(t) = turns.get_mut(i) else { return false };
    if at == 0 || at >= t.words.len() {
        return false;
    }
    let tail = t.words.split_off(at);
    settle(t);
    let mut second = Turn { speaker: t.speaker.clone(), start: 0.0, end: 0.0, words: tail };
    settle(&mut second);
    turns.insert(i + 1, second);
    true
}

/// Give turn `i`, or only its words `words` (a selection), to `speaker`. A selection in the middle of a turn
/// splits it in three. The result is joined with neighbours of the same speaker. Returns the index of the turn
/// that now holds the reassigned words.
pub fn reassign(turns: &mut Vec<Turn>, i: usize, words: Option<Range<usize>>, speaker: &str) -> Option<usize> {
    let n = turns.get(i)?.words.len();
    let r = words.map(|r| r.start.min(n)..r.end.min(n)).filter(|r| !r.is_empty()).unwrap_or(0..n);
    let mut at = i;
    if r.end < n {
        split_turn(turns, i, r.end);
    }
    if r.start > 0 {
        split_turn(turns, i, r.start);
        at = i + 1;
    }
    turns[at].speaker = speaker.to_string();
    settle(&mut turns[at]);
    // Join with the turn before if it is now the same speaker, and with the one after.
    let joined_before = at > 0 && turns[at - 1].speaker == turns[at].speaker;
    tidy(turns, at.saturating_sub(1), at + 1);
    Some(if joined_before { at - 1 } else { at })
}

/// Join turn `i` onto the turn before it (Backspace at the start of a turn). Its words take that turn's speaker.
pub fn join_with_previous(turns: &mut Vec<Turn>, i: usize) -> bool {
    if i == 0 || i >= turns.len() {
        return false;
    }
    let t = turns.remove(i);
    turns[i - 1].words.extend(t.words);
    settle(&mut turns[i - 1]);
    tidy(turns, i - 1, i);
    true
}

/// Merge speaker `from` into `into`: every turn of `from` becomes `into`'s, neighbours are joined, and `from`
/// leaves the list. `into` keeps its own name, role and attributes; any attribute only `from` had is kept too.
pub fn merge_speakers(speakers: &mut Vec<Speaker>, turns: &mut Vec<Turn>, from: &str, into: &str) -> bool {
    if from == into || !speakers.iter().any(|s| s.id == into) {
        return false;
    }
    let Some(pos) = speakers.iter().position(|s| s.id == from) else { return false };
    let gone = speakers.remove(pos);
    let target = speakers.iter_mut().find(|s| s.id == into).unwrap();
    if target.name.as_deref().map(str::trim).unwrap_or("").is_empty() {
        target.name = gone.name.clone();
    }
    for (k, v) in gone.attributes {
        target.attributes.entry(k).or_insert(v);
    }
    let mut touched = false;
    for t in turns.iter_mut() {
        if t.speaker == from {
            t.speaker = into.to_string();
            settle(t);
            touched = true;
        }
    }
    if touched {
        // Join every place where two `into` turns now meet. Only boundaries the merge created can meet: before the
        // merge, two neighbours of `into` were already joined by the pipeline or kept apart by the user's split,
        // and a user's split of `into` next to `from` is lost here; that is what merging means.
        let mut i = 1;
        while i < turns.len() {
            if turns[i].speaker == into && turns[i - 1].speaker == into {
                let t = turns.remove(i);
                turns[i - 1].words.extend(t.words);
                settle(&mut turns[i - 1]);
            } else {
                i += 1;
            }
        }
    }
    true
}

/// A new speaker, "Speaker n" with the next free number, at the end of the list.
pub fn add_speaker(speakers: &mut Vec<Speaker>) -> SpeakerId {
    let mut n = speakers.len() + 1;
    while speakers.iter().any(|s| s.id == format!("S{n}") || s.label == format!("Speaker {n}")) {
        n += 1;
    }
    let id = format!("S{n}");
    speakers.push(Speaker { id: id.clone(), label: format!("Speaker {n}"), name: None, role: Role::Other, attributes: BTreeMap::new() });
    id
}

/// Remove a speaker who has no turns left. Refused (false) while any turn is theirs.
pub fn remove_speaker(speakers: &mut Vec<Speaker>, turns: &[Turn], id: &str) -> bool {
    if turns.iter().any(|t| t.speaker == id) {
        return false;
    }
    let before = speakers.len();
    speakers.retain(|s| s.id != id);
    speakers.len() != before
}

/// How much each speaker says, for the speakers panel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpeakerStats {
    pub id: SpeakerId,
    pub turns: usize,
    pub words: usize,
    /// Seconds of their turns, start to end.
    pub seconds: f64,
}

pub fn stats(speakers: &[Speaker], turns: &[Turn]) -> Vec<SpeakerStats> {
    speakers
        .iter()
        .map(|s| {
            let mine = turns.iter().filter(|t| t.speaker == s.id);
            let (mut n, mut words, mut seconds) = (0, 0, 0.0);
            for t in mine {
                n += 1;
                words += t.words.iter().filter(|w| !crate::edit::is_notation(&w.text)).count();
                seconds += (t.end - t.start).max(0.0);
            }
            SpeakerStats { id: s.id.clone(), turns: n, words, seconds }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Word;

    fn turn(s: &str, words: &[(&str, f64)]) -> Turn {
        let mut t = Turn { speaker: s.into(), start: 0.0, end: 0.0, words: words.iter().map(|(w, a)| Word { text: (*w).into(), start: *a, end: a + 0.3, confidence: 0.9, speaker: s.into() }).collect() };
        settle(&mut t);
        t
    }

    fn three() -> Vec<Turn> {
        vec![
            turn("S1", &[("How", 0.0), ("are", 0.4), ("you?", 0.8)]),
            turn("S2", &[("Fine,", 1.5), ("thanks.", 1.9), ("And", 2.4), ("you?", 2.8)]),
            turn("S1", &[("Good.", 3.5)]),
        ]
    }

    fn speakers(n: usize) -> Vec<Speaker> {
        let mut v = Vec::new();
        for _ in 0..n {
            add_speaker(&mut v);
        }
        v
    }

    fn consistent(turns: &[Turn]) {
        for t in turns {
            assert!(!t.words.is_empty());
            assert!(t.words.iter().all(|w| w.speaker == t.speaker), "words carry their turn's speaker");
            assert_eq!(t.start, t.words[0].start);
            assert_eq!(t.end, t.words.last().unwrap().end);
        }
        assert!(turns.windows(2).all(|p| p[0].speaker != p[1].speaker || p[0].end <= p[1].start));
    }

    #[test]
    fn split_keeps_the_speaker_and_times() {
        let mut t = three();
        assert!(split_turn(&mut t, 1, 2));
        assert_eq!(t.len(), 4);
        assert_eq!(t[1].words.len(), 2);
        assert_eq!(t[2].words[0].text, "And");
        assert_eq!(t[2].start, 2.4);
        assert_eq!(t[2].speaker, "S2");
        assert!(!split_turn(&mut t, 0, 0), "nothing to split at the start");
        assert!(!split_turn(&mut t, 0, 3), "nor at the end");
        consistent(&t);
    }

    #[test]
    fn reassigning_a_whole_turn_joins_its_neighbours() {
        let mut t = three();
        let at = reassign(&mut t, 1, None, "S1").unwrap();
        assert_eq!(t.len(), 1, "S1, S1, S1 become one turn");
        assert_eq!(at, 0);
        assert_eq!(t[0].words.len(), 8);
        consistent(&t);
    }

    #[test]
    fn reassigning_a_selection_splits_in_three() {
        let mut t = three();
        // "thanks. And" is really a third person.
        let at = reassign(&mut t, 1, Some(1..3), "S3").unwrap();
        let who: Vec<&str> = t.iter().map(|x| x.speaker.as_str()).collect();
        assert_eq!(who, ["S1", "S2", "S3", "S2", "S1"]);
        assert_eq!(at, 2);
        assert_eq!(t[2].words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["thanks.", "And"]);
        consistent(&t);

        // The first words of a turn given to the speaker before it join that turn.
        let mut t = three();
        let at = reassign(&mut t, 1, Some(0..1), "S1").unwrap();
        assert_eq!(at, 0);
        assert_eq!(t[0].words.last().unwrap().text, "Fine,");
        assert_eq!(t[1].words[0].text, "thanks.");
        consistent(&t);

        // The last words given to the speaker after it join that turn.
        let mut t = three();
        let at = reassign(&mut t, 1, Some(2..4), "S1").unwrap();
        assert_eq!(at, 2);
        assert_eq!(t.len(), 3);
        assert_eq!(t[2].words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["And", "you?", "Good."]);
        consistent(&t);
    }

    #[test]
    fn join_with_previous_takes_that_speaker() {
        let mut t = three();
        assert!(join_with_previous(&mut t, 1));
        assert_eq!(t.len(), 1, "joined onto S1, then S1's next turn meets it too");
        assert!(!join_with_previous(&mut t, 0));
        consistent(&t);
    }

    #[test]
    fn merging_speakers_moves_turns_and_keeps_names() {
        let mut s = speakers(3);
        s[1].name = Some("Dana".into());
        s[1].attributes.insert("site".into(), "North".into());
        s[0].attributes.insert("site".into(), "South".into());
        let mut t = three();
        t[2].speaker = "S3".into();
        settle(&mut t[2]);
        // S3 was really S2: Dana.
        assert!(merge_speakers(&mut s, &mut t, "S3", "S2"));
        assert_eq!(s.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["S1", "S2"]);
        assert_eq!(t.len(), 2, "S2's two turns now meet and are joined");
        assert_eq!(t[1].words.last().unwrap().text, "Good.");
        consistent(&t);
        // S2 into S1: S1 has no name, so it takes Dana's; S1's own attribute wins.
        assert!(merge_speakers(&mut s, &mut t, "S2", "S1"));
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name.as_deref(), Some("Dana"));
        assert_eq!(s[0].attributes["site"], "South");
        assert_eq!(t.len(), 1);
        consistent(&t);
        assert!(!merge_speakers(&mut s, &mut t, "S1", "S1"));
        assert!(!merge_speakers(&mut s, &mut t, "S9", "S1"));
    }

    #[test]
    fn new_speakers_get_free_numbers_and_only_unused_ones_are_removed() {
        let mut s = speakers(2);
        let t = three();
        assert_eq!(add_speaker(&mut s), "S3");
        assert!(!remove_speaker(&mut s, &t, "S1"), "S1 still has turns");
        assert!(remove_speaker(&mut s, &t, "S3"));
        s.remove(0);
        assert_eq!(add_speaker(&mut s), "S3", "S2 is taken, so the next is S3");
    }

    #[test]
    fn stats_count_words_but_not_notation() {
        let s = speakers(2);
        let mut t = three();
        t[1].words.push(Word { text: "[laughter]".into(), start: 3.2, end: 3.2, confidence: 1.0, speaker: "S2".into() });
        settle(&mut t[1]);
        let st = stats(&s, &t);
        assert_eq!((st[0].turns, st[0].words), (2, 4));
        assert_eq!((st[1].turns, st[1].words), (1, 4));
    }
}
