//! The steps between the three jobs: cutting speech into chunks at silences, turning tokens into words, and
//! putting speaker labels on words to make turns. Pure functions, so they are tested without models.

use crate::{SpeakerSpan, SpeechSpan, Word};
use serde::{Deserialize, Serialize};

/// A piece of the recording given to the recognizer in one go. Chunks start and end in silence.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub start: f64,
    pub end: f64,
}

/// Group detected speech into chunks no longer than `max_len` seconds (a single longer stretch stays whole: the
/// detector already splits those), each padded by up to `pad` seconds of the silence around it without crossing
/// the middle of a gap. Speech outside every chunk is never recognized, which is what keeps a recognizer from
/// writing words into silence.
pub fn chunks_from_speech(spans: &[SpeechSpan], max_len: f64, pad: f64, duration: f64) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = Vec::new();
    for s in spans {
        match chunks.last_mut() {
            Some(c) if s.end - c.start <= max_len => c.end = c.end.max(s.end),
            _ => chunks.push(Chunk { start: s.start, end: s.end }),
        }
    }
    let n = chunks.len();
    let bare = chunks.clone();
    for i in 0..n {
        let lo = if i == 0 { 0.0 } else { (bare[i - 1].end + bare[i].start) / 2.0 };
        let hi = if i + 1 == n { duration } else { (bare[i].end + bare[i + 1].start) / 2.0 };
        chunks[i].start = (bare[i].start - pad).max(lo).max(0.0);
        chunks[i].end = (bare[i].end + pad).min(hi).min(duration);
    }
    chunks
}

fn is_punctuation(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t.chars().all(|c| !c.is_alphanumeric())
}

/// Build words from a recognizer's tokens. A token that starts with a space (or SentencePiece's `▁`) begins a new
/// word; a token of punctuation alone is attached to the word before it. Times are `offset` plus the token's
/// time; a word ends at its last token's start plus that token's duration (TDT models give durations; otherwise
/// the next token's start, or 0.08 s). Confidence is the geometric mean of the word's token probabilities,
/// leaving out attached punctuation (an unsure comma is not an unsure word).
pub fn words_from_tokens(tokens: &[String], timestamps: &[f32], durations: &[f32], log_probs: &[f32], offset: f64) -> Vec<Word> {
    struct Acc {
        text: String,
        start: f64,
        end: f64,
        logp_sum: f64,
        logp_n: u32,
    }
    let mut words: Vec<Acc> = Vec::new();
    for (i, tok) in tokens.iter().enumerate() {
        let t = timestamps.get(i).copied().unwrap_or(0.0) as f64 + offset;
        let end = match durations.get(i) {
            Some(d) if *d > 0.0 => t + *d as f64,
            _ => timestamps.get(i + 1).map(|n| *n as f64 + offset).unwrap_or(t + 0.08),
        };
        let lp = log_probs.get(i).copied();
        let clean = tok.replace('\u{2581}', " ");
        if is_punctuation(&clean) {
            if let Some(w) = words.last_mut() {
                w.text.push_str(clean.trim());
                continue;
            }
        }
        let starts_word = clean.starts_with(' ') || words.is_empty();
        if starts_word {
            words.push(Acc { text: clean.trim_start().to_string(), start: t, end, logp_sum: 0.0, logp_n: 0 });
        } else if let Some(w) = words.last_mut() {
            w.text.push_str(&clean);
            w.end = w.end.max(end);
        }
        if let (Some(lp), Some(w)) = (lp, words.last_mut()) {
            w.logp_sum += lp as f64;
            w.logp_n += 1;
        }
    }
    words
        .into_iter()
        .filter(|w| !w.text.is_empty())
        .map(|w| Word {
            text: w.text,
            start: w.start,
            end: w.end.max(w.start),
            confidence: if w.logp_n == 0 { 1.0 } else { (w.logp_sum / w.logp_n as f64).exp().clamp(0.0, 1.0) as f32 },
        })
        .collect()
}

/// Stretches of at least `min_gap` seconds inside `chunk` that hold no word: before the first word, between
/// words, and after the last.
pub fn word_gaps(words: &[Word], chunk: Chunk, min_gap: f64) -> Vec<Chunk> {
    let mut gaps = Vec::new();
    let mut t = chunk.start;
    for w in words {
        if w.start - t >= min_gap {
            gaps.push(Chunk { start: t, end: w.start });
        }
        t = t.max(w.end);
    }
    if chunk.end - t >= min_gap {
        gaps.push(Chunk { start: t, end: chunk.end });
    }
    gaps
}

/// Recognize one chunk, then recognize again, on its own, every stretch of `min_gap` seconds or more that came
/// back with no words.
///
/// Why: Parakeet TDT sometimes stops emitting part-way through a chunk and skips the rest of it (phase 0 found
/// whole 9-second answers missing; moving the chunk's edges by half a second brought them back). A stretch that
/// really is silent comes back empty again, so the cost is a little extra time on pauses.
pub fn recognize_chunk(
    asr: &dyn crate::SpeechRecognizer,
    samples: &[f32],
    chunk: Chunk,
    language: Option<&str>,
    min_gap: f64,
) -> crate::Result<Vec<Word>> {
    let sr = crate::SAMPLE_RATE as f64;
    let slice = |a: f64, b: f64| &samples[((a * sr) as usize).min(samples.len())..((b * sr) as usize).min(samples.len())];
    let mut words = asr.recognize(slice(chunk.start, chunk.end), chunk.start, language)?;
    let mut extra = Vec::new();
    for g in word_gaps(&words, chunk, min_gap) {
        let (a, b) = ((g.start - 0.15).max(chunk.start), (g.end + 0.15).min(chunk.end));
        extra.extend(asr.recognize(slice(a, b), a, language)?.into_iter().filter(|w| w.start >= g.start - 0.05 && w.end <= g.end + 0.05));
    }
    if !extra.is_empty() {
        words.extend(extra);
        words.sort_by(|a, b| a.start.total_cmp(&b.start));
    }
    Ok(words)
}

/// A run of words by one speaker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub speaker: u32,
    pub start: f64,
    pub end: f64,
    pub words: Vec<Word>,
}

impl Turn {
    pub fn text(&self) -> String {
        self.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
    }
}

/// The speaker label for each word: the label whose span overlaps the word most, or, when none overlaps, the
/// nearest span within one second, or else the label of the word before. One speaker per word, even where the
/// labeller found two people talking at once (the editor marks crosstalk separately, T3).
pub fn speakers_for_words(words: &[Word], spans: &[SpeakerSpan]) -> Vec<u32> {
    let mut out = Vec::with_capacity(words.len());
    let mut first = 0usize; // spans are sorted by start; skip those that ended long before this word
    let mut prev = 0u32;
    for w in words {
        while first < spans.len() && spans[first].end < w.start - 30.0 {
            first += 1;
        }
        let mut best: Option<(f64, u32)> = None;
        let mut nearest: Option<(f64, u32)> = None;
        for s in &spans[first..] {
            if s.start > w.end + 1.0 {
                break;
            }
            let overlap = w.end.min(s.end) - w.start.max(s.start);
            if overlap > 0.0 {
                if best.is_none_or(|(o, _)| overlap > o) {
                    best = Some((overlap, s.speaker));
                }
            } else {
                let gap = (s.start - w.end).max(w.start - s.end);
                if gap <= 1.0 && nearest.is_none_or(|(g, _)| gap < g) {
                    nearest = Some((gap, s.speaker));
                }
            }
        }
        let spk = best.or(nearest).map(|(_, s)| s).unwrap_or(prev);
        out.push(spk);
        prev = spk;
    }
    out
}

/// Group words into turns: a new turn wherever the speaker changes.
pub fn turns(words: Vec<Word>, speakers: &[u32]) -> Vec<Turn> {
    let mut out: Vec<Turn> = Vec::new();
    for (w, &spk) in words.into_iter().zip(speakers) {
        match out.last_mut() {
            Some(t) if t.speaker == spk => {
                t.end = t.end.max(w.end);
                t.words.push(w);
            }
            _ => out.push(Turn { speaker: spk, start: w.start, end: w.end, words: vec![w] }),
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------------
// One microphone per person (T1: each track becomes a known speaker, which skips diarization)
// ---------------------------------------------------------------------------------------------------------

/// Frames of 20 ms for comparing microphones.
pub const MASK_FRAME: usize = 320;

/// For each track, which 20 ms frames are its own speaker talking rather than another voice heard through it.
///
/// Every microphone hears everyone, more quietly ("bleed"). A frame belongs to a track when that track is within
/// `margin_db` of the loudest track at that moment, after the microphones' gains are evened out (measured from the
/// bleed, below), so a hot microphone does not win every frame. Two people
/// talking at once are both within the margin on their own microphones, so both keep the frame: that is how
/// crosstalk survives. Short holes are closed and the result widened by 0.2 s so words are not clipped.
pub fn dominance_masks(tracks: &[&[f32]], margin_db: f32) -> Vec<Vec<bool>> {
    let frames = tracks.iter().map(|t| t.len() / MASK_FRAME).min().unwrap_or(0);
    let rms: Vec<Vec<f32>> = tracks
        .iter()
        .map(|t| (0..frames).map(|f| (t[f * MASK_FRAME..(f + 1) * MASK_FRAME].iter().map(|x| x * x).sum::<f32>() / MASK_FRAME as f32).sqrt()).collect())
        .collect();
    // Put the microphones on one scale by their gain, measured from the bleed itself. Where track j alone is
    // talking, track k hears it at (gain k / gain j) times the room's coupling; where k alone is talking, j hears it
    // at (gain j / gain k) times the same coupling. Half the difference of the two (in logs) is the gain difference,
    // whatever the room, and whoever talks louder or longer. Track 0 is the reference; a pair with too few frames
    // each way is left at equal gain.
    let median = |mut v: Vec<f32>| -> Option<f32> {
        if v.len() < 25 {
            return None;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        Some(v[v.len() / 2])
    };
    let alone = |j: usize, f: usize| rms[j][f] > 1e-4 && (0..tracks.len()).all(|i| i == j || rms[j][f] >= 2.0 * rms[i][f]);
    let scale: Vec<f32> = (0..tracks.len())
        .map(|k| {
            if k == 0 {
                return 1.0;
            }
            let heard_by_k = median((0..frames).filter(|&f| alone(0, f) && rms[k][f] > 0.0).map(|f| rms[k][f] / rms[0][f]).collect());
            let heard_by_0 = median((0..frames).filter(|&f| alone(k, f) && rms[0][f] > 0.0).map(|f| rms[0][f] / rms[k][f]).collect());
            match (heard_by_k, heard_by_0) {
                (Some(a), Some(b)) if a > 0.0 && b > 0.0 => (a / b).sqrt(),
                _ => 1.0,
            }
        })
        .collect();
    // Each microphone's noise floor (its quietest tenth): a frame is only someone talking on that microphone when it
    // is 12 dB above it, so the pauses between words are not "everyone at once".
    let floor: Vec<f32> = rms
        .iter()
        .map(|r| {
            let mut v = r.clone();
            v.sort_by(|a, b| a.total_cmp(b));
            v.get(v.len() / 10).copied().unwrap_or(0.0).max(1e-5)
        })
        .collect();
    let ratio = 10f32.powf(-margin_db / 20.0);
    let mut masks = vec![vec![false; frames]; tracks.len()];
    for f in 0..frames {
        let level: Vec<f32> = (0..tracks.len()).map(|k| rms[k][f] / scale[k]).collect();
        let loudest = level.iter().copied().fold(0.0f32, f32::max);
        for k in 0..tracks.len() {
            masks[k][f] = rms[k][f] > 4.0 * floor[k] && level[k] >= loudest * ratio;
        }
    }
    let close = 15; // 0.3 s
    let widen = 10; // 0.2 s
    for m in &mut masks {
        // Close holes no longer than `close` frames.
        let mut last_on: Option<usize> = None;
        for f in 0..m.len() {
            if m[f] {
                if let Some(l) = last_on {
                    if f - l > 1 && f - l <= close {
                        m[l..f].iter_mut().for_each(|x| *x = true);
                    }
                }
                last_on = Some(f);
            }
        }
        let orig = m.clone();
        for f in 0..m.len() {
            if orig[f] {
                let (a, b) = (f.saturating_sub(widen), (f + widen + 1).min(m.len()));
                m[a..b].iter_mut().for_each(|x| *x = true);
            }
        }
    }
    masks
}

/// `samples` with every frame outside `mask` silenced (with a 20 ms fade either side), so a recognizer given a chunk
/// of one microphone does not hear the other people through it. Frames past the mask's end are silenced too.
pub fn apply_mask(samples: &[f32], mask: &[bool]) -> Vec<f32> {
    let mut out = vec![0.0f32; samples.len()];
    let n = MASK_FRAME;
    for (f, on) in mask.iter().enumerate() {
        if !*on {
            continue;
        }
        let (a, b) = (f * n, ((f + 1) * n).min(samples.len()));
        let fade_in = f == 0 || !mask[f - 1];
        let fade_out = f + 1 >= mask.len() || !mask[f + 1];
        for i in a..b {
            let k = (i - a) as f32 / n as f32;
            let g = match (fade_in, fade_out) {
                (true, true) => (k * 2.0).min((1.0 - k) * 2.0).min(1.0),
                (true, false) => k,
                (false, true) => 1.0 - k,
                _ => 1.0,
            };
            out[i] = samples[i] * g;
        }
    }
    out
}

/// The parts of `spans` where `mask` is on, dropping pieces shorter than `min_len` seconds.
pub fn mask_spans(spans: &[SpeechSpan], mask: &[bool], min_len: f64) -> Vec<SpeechSpan> {
    let fs = MASK_FRAME as f64 / crate::SAMPLE_RATE as f64;
    let mut out = Vec::new();
    for s in spans {
        let a = (s.start / fs).floor().max(0.0) as usize;
        let b = ((s.end / fs).ceil() as usize).min(mask.len());
        let mut run: Option<usize> = None;
        for f in a..=b {
            let on = f < b && mask[f];
            match (on, run) {
                (true, None) => run = Some(f),
                (false, Some(r)) => {
                    let piece = SpeechSpan { start: (r as f64 * fs).max(s.start), end: (f as f64 * fs).min(s.end) };
                    if piece.end - piece.start >= min_len {
                        out.push(piece);
                    }
                    run = None;
                }
                _ => {}
            }
        }
    }
    out
}

/// The marker written where two people talk at once (T3).
pub const CROSSTALK: &str = "[crosstalk]";

fn marker(t: f64) -> Word {
    Word { text: CROSSTALK.into(), start: t, end: t, confidence: 1.0 }
}

/// Turns from one list of words per track (track `k` is speaker `k`).
///
/// Each track's words are cut into utterances at pauses of `gap` seconds and the utterances are put in order of
/// their start. Where one starts while another speaker's is still going (that speaker says another word after it
/// starts, and the two overlap by at least `min_overlap` seconds), the one already going is cut there and carries on after the newcomer, and the newcomer's overlapped words are put
/// between two [crosstalk] markers, at the overlap's start and its end:
///
/// ```text
/// Speaker 1: So what I was
/// Speaker 2: [crosstalk] Sorry, go on. [crosstalk]
/// Speaker 1: saying is that ...
/// ```
///
/// Each word keeps one speaker. Turns are in order of their start; a turn can start before the one above it ends.
pub fn turns_from_tracks(tracks: Vec<Vec<Word>>, gap: f64, min_overlap: f64) -> Vec<Turn> {
    let mut queue: Vec<Turn> = Vec::new();
    for (k, mut words) in tracks.into_iter().enumerate() {
        words.sort_by(|a, b| a.start.total_cmp(&b.start));
        for w in words {
            match queue.last_mut() {
                Some(t) if t.speaker == k as u32 && w.start - t.end < gap => {
                    t.end = t.end.max(w.end);
                    t.words.push(w);
                }
                _ => queue.push(Turn { speaker: k as u32, start: w.start, end: w.end, words: vec![w] }),
            }
        }
    }
    // Earliest start last, so `pop` takes it. The flag marks the rest of a turn that was cut: it carries on after
    // the newcomer and never cuts anything itself (or the two would cut each other for ever).
    let mut queue: Vec<(Turn, bool)> = queue.into_iter().map(|t| (t, false)).collect();
    queue.sort_by(|(a, _), (b, _)| b.start.total_cmp(&a.start).then(b.speaker.cmp(&a.speaker)));
    let mut out: Vec<Turn> = Vec::new();
    while let Some((mut u, resumed)) = queue.pop() {
        if let (false, Some(prev)) = (resumed, out.last_mut()) {
            let overlap_end = prev.end.min(u.end);
            // Crosstalk is the other person still saying words after the newcomer starts, not only a last word whose
            // end time runs on.
            let at = prev.words.iter().position(|w| w.start >= u.start).unwrap_or(prev.words.len());
            let still_talking = prev.words.get(at).is_some_and(|w| w.start < u.end);
            if prev.speaker != u.speaker && still_talking && overlap_end - u.start >= min_overlap {
                // Cut the turn already going where the newcomer starts; the rest waits in the queue.
                let rest: Vec<Word> = prev.words.split_off(at);
                if let Some(l) = prev.words.last() {
                    prev.end = prev.words.iter().map(|w| w.end).fold(l.end, f64::max);
                }
                if !rest.is_empty() {
                    let r = Turn { speaker: prev.speaker, start: rest[0].start, end: rest.iter().map(|w| w.end).fold(0.0, f64::max), words: rest };
                    // Keep the queue sorted with the earliest start last.
                    let pos = queue.iter().position(|(q, _)| q.start < r.start || (q.start == r.start && q.speaker < r.speaker)).unwrap_or(queue.len());
                    queue.insert(pos, (r, true));
                }
                let end_at = u.words.iter().position(|w| w.start >= overlap_end).unwrap_or(u.words.len());
                let start_t = u.start;
                u.words.insert(end_at, marker(overlap_end));
                u.words.insert(0, marker(start_t));
                u.end = u.end.max(overlap_end);
                if prev.words.is_empty() {
                    out.pop();
                }
            }
        }
        match out.last_mut() {
            Some(t) if t.speaker == u.speaker => {
                t.end = t.end.max(u.end);
                t.words.extend(u.words);
            }
            _ => out.push(u),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn tokens_become_words_with_punctuation_and_confidence() {
        let toks = s(&[" A", "sk", " not", ",", " what", "."]);
        let ts = [0.0, 0.24, 0.40, 0.64, 0.80, 1.04];
        let du = [0.24, 0.16, 0.24, 0.08, 0.16, 0.08];
        let lp = [-0.1, -0.1, 0.0, -2.0, -0.5, 0.0];
        let w = words_from_tokens(&toks, &ts, &du, &lp, 10.0);
        assert_eq!(w.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["Ask", "not,", "what."]);
        assert!((w[0].start - 10.0).abs() < 1e-6);
        assert!((w[0].end - 10.40).abs() < 1e-6);
        assert!((w[0].confidence - (-0.1f64).exp() as f32).abs() < 1e-5);
        // An unsure comma does not make the word before it look unsure: punctuation's probability is left out.
        assert_eq!(w[1].confidence, 1.0);
    }

    #[test]
    fn no_probabilities_means_full_confidence() {
        let w = words_from_tokens(&s(&["\u{2581}hello", "\u{2581}there"]), &[0.0, 0.5], &[], &[], 0.0);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].text, "hello");
        assert_eq!(w[0].confidence, 1.0);
        assert!((w[0].end - 0.5).abs() < 1e-6, "ends where the next token starts");
    }

    #[test]
    fn chunks_respect_length_and_pad_into_silence_only() {
        let sp = |a, b| SpeechSpan { start: a, end: b };
        let spans = [sp(1.0, 5.0), sp(6.0, 12.0), sp(13.0, 40.0), sp(41.0, 42.0)];
        let c = chunks_from_speech(&spans, 15.0, 0.3, 50.0);
        assert_eq!(c.len(), 3);
        assert!((c[0].start - 0.7).abs() < 1e-9 && (c[0].end - 12.3).abs() < 1e-9);
        assert!((c[1].start - 12.7).abs() < 1e-9, "a long stretch stays whole");
        // The pad stops at the middle of a short gap.
        assert!((c[1].end - 40.3).abs() < 1e-9 && (c[2].start - 40.7).abs() < 1e-9);
        for pair in c.windows(2) {
            assert!(pair[0].end <= pair[1].start);
        }
    }

    struct SkipsTail;
    impl crate::SpeechRecognizer for SkipsTail {
        // Hears one word per second of input, but stops after the first three: the failure seen in phase 0.
        fn recognize(&self, samples: &[f32], offset: f64, _: Option<&str>) -> crate::Result<Vec<Word>> {
            let secs = samples.len() / crate::SAMPLE_RATE as usize;
            Ok((0..secs.min(3)).map(|i| Word { text: format!("w{}", offset as usize + i), start: offset + i as f64, end: offset + i as f64 + 0.5, confidence: 1.0 }).collect())
        }
        fn languages(&self) -> &[&'static str] {
            &["en"]
        }
    }

    #[test]
    fn a_skipped_tail_is_recognized_again() {
        let samples = vec![0.0f32; 16_000 * 10];
        let w = recognize_chunk(&SkipsTail, &samples, Chunk { start: 0.0, end: 10.0 }, None, 1.5).unwrap();
        // First pass: words at 0, 1, 2. The gap 2.5..10 is tried again from 2.35; of the three words that come
        // back (2.35, 3.35, 4.35) the first starts before the gap and is dropped as already heard.
        assert_eq!(w.len(), 5);
        assert_eq!(w[3].start, 3.35);
        assert!(w.windows(2).all(|p| p[0].start <= p[1].start));
        assert_eq!(word_gaps(&w[..3], Chunk { start: 0.0, end: 10.0 }, 1.5), [Chunk { start: 2.5, end: 10.0 }]);
    }

    #[test]
    fn words_take_the_most_overlapping_speaker_and_form_turns() {
        let w = |t: &str, a, b| Word { text: t.into(), start: a, end: b, confidence: 1.0 };
        let words = vec![w("one", 0.0, 0.5), w("two", 0.5, 1.0), w("three", 1.9, 2.4), w("four", 2.5, 3.0), w("five", 9.0, 9.2)];
        let spans = [
            SpeakerSpan { start: 0.0, end: 2.0, speaker: 0 },
            SpeakerSpan { start: 2.0, end: 3.1, speaker: 1 },
        ];
        let spk = speakers_for_words(&words, &spans);
        assert_eq!(spk, [0, 0, 1, 1, 1], "'three' overlaps speaker 1 more; 'five' has no span near and keeps the last");
        let t = turns(words, &spk);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].text(), "one two");
        assert_eq!(t[1].text(), "three four five");
    }

    fn w(t: &str, a: f64, b: f64) -> Word {
        Word { text: t.into(), start: a, end: b, confidence: 0.9 }
    }

    #[test]
    fn bleed_from_another_microphone_is_not_its_speaker() {
        // Track 0 speaks for the first second, track 1 for the second; each microphone hears the other at -20 dB.
        let n = 32_000;
        let tone = |i: usize| (i as f32 * 0.05).sin() * 0.5;
        let a: Vec<f32> = (0..n).map(|i| if i < 16_000 { tone(i) } else { tone(i) * 0.1 }).collect();
        let b: Vec<f32> = (0..n).map(|i| if i >= 16_000 { tone(i) } else { tone(i) * 0.1 }).collect();
        let m = dominance_masks(&[&a, &b], 10.0);
        let on = |k: usize, t: f64| m[k][(t * 50.0) as usize];
        assert!(on(0, 0.5) && !on(1, 0.5), "the first second is track 0's");
        assert!(!on(0, 1.5) && on(1, 1.5), "the second second is track 1's");
        // Both talking at once for the first second (then a quiet room): both keep those frames.
        let c: Vec<f32> = (0..n).map(|i| if i < 16_000 { tone(i) } else { tone(i) * 0.002 }).collect();
        let m2 = dominance_masks(&[&c, &c], 10.0);
        assert!(m2[0][25] && m2[1][25]);
        assert!(!m2[0][90] && !m2[1][90], "the quiet room is nobody's");

        let gated = apply_mask(&a, &m[0]);
        assert_eq!(gated[8_000], a[8_000], "its own speech passes untouched");
        assert!(gated[28_000..].iter().all(|x| *x == 0.0), "the other voice, heard through it, is silenced");
        let spans = [SpeechSpan { start: 0.2, end: 1.8 }];
        let kept = mask_spans(&spans, &m[0], 0.25);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].start == 0.2 && (kept[0].end - 1.2).abs() < 0.05, "{kept:?}");
    }

    #[test]
    fn tracks_become_turns_with_crosstalk_marked() {
        let a = vec![w("So", 0.0, 0.3), w("what", 0.4, 0.6), w("I", 0.7, 0.8), w("was", 0.9, 1.1), w("saying", 1.6, 2.0), w("is", 2.1, 2.3), w("this.", 2.4, 2.8), w("Next", 6.0, 6.3)];
        let b = vec![w("Sorry,", 1.2, 1.5), w("go", 1.6, 1.8), w("on.", 1.9, 2.2), w("Okay.", 4.0, 4.4)];
        let t = turns_from_tracks(vec![a, b], 1.0, 0.3);
        let text: Vec<(u32, String)> = t.iter().map(|x| (x.speaker, x.text())).collect();
        assert_eq!(
            text,
            [
                (0, "So what I was".to_string()),
                (1, "[crosstalk] Sorry, go on. [crosstalk]".to_string()),
                (0, "saying is this.".to_string()),
                (1, "Okay.".to_string()),
                (0, "Next".to_string()),
            ]
        );
        assert!(t.windows(2).all(|p| p[0].start <= p[1].start), "turns in order of their start");
        // A brief touch (under the minimum) is not crosstalk.
        let t = turns_from_tracks(vec![vec![w("Yes.", 0.0, 1.0)], vec![w("Right.", 0.9, 1.5)]], 1.0, 0.3);
        assert_eq!(t.iter().map(|x| x.text()).collect::<Vec<_>>(), ["Yes.", "Right."]);
        // Nor is a last word whose end time runs on into the next speaker's first.
        let t = turns_from_tracks(vec![vec![w("I", 0.0, 0.2), w("agree.", 0.3, 1.6)], vec![w("Good.", 1.0, 1.5)]], 1.0, 0.3);
        assert_eq!(t.iter().map(|x| x.text()).collect::<Vec<_>>(), ["I agree.", "Good."]);
    }
}
