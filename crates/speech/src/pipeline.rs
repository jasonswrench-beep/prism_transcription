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
}
