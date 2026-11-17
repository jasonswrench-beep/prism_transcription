//! Accuracy against a truth file written by tools/make_fixtures.py: word error rate, which speaker said each word,
//! diarization error over time, and how far word start times are from the truth.

use prism_transcribe_speech::pipeline::Turn;
use prism_transcribe_speech::SpeakerSpan;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Truth {
    pub duration: f64,
    pub speakers: Vec<TruthSpeaker>,
    pub turns: Vec<TruthTurn>,
}
#[derive(Deserialize)]
pub struct TruthSpeaker {
    pub id: String,
}
#[derive(Deserialize)]
pub struct TruthTurn {
    pub speaker: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(default)]
    pub words: Vec<TruthWord>,
}
#[derive(Deserialize)]
pub struct TruthWord {
    pub text: String,
    pub start: f64,
}

#[derive(Serialize, Default, Debug)]
pub struct Score {
    pub truth_words: usize,
    pub hyp_words: usize,
    pub substitutions: usize,
    pub deletions: usize,
    pub insertions: usize,
    /// (S + D + I) / truth words.
    pub wer: f64,
    pub truth_speakers: usize,
    pub found_speakers: usize,
    /// Share of correctly recognized words given to the right speaker, after mapping each found label to the true
    /// speaker it shares most words with (one to one).
    pub word_speaker_accuracy: f64,
    /// Diarization error rate of the labeller's own spans, scored on 10 ms frames where exactly one true speaker
    /// talks (overlapped speech is left out), no collar: (missed + false alarm + confusion) / true speech.
    pub der: f64,
    pub der_missed: f64,
    pub der_false_alarm: f64,
    pub der_confusion: f64,
    /// Over correctly recognized words: the median and 90th percentile of |found start - true start|, seconds.
    pub word_start_error_median: f64,
    pub word_start_error_p90: f64,
    pub word_start_within_half_second: f64,
}

fn norm(s: &str) -> Vec<String> {
    s.to_lowercase()
        .replace(['-', '\u{2014}', '\u{2013}', '/'], " ")
        .split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect::<String>().trim_matches('\'').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

struct RefWord {
    w: String,
    start: f64,
    speaker: usize,
}

struct HypWord {
    w: String,
    start: f64,
    speaker: u32,
}

/// Edit-distance alignment. Returns (S, D, I, matched pairs as (ref index, hyp index)).
fn align(r: &[RefWord], h: &[HypWord]) -> (usize, usize, usize, Vec<(usize, usize)>) {
    let (n, m) = (r.len(), h.len());
    // Back-pointers: 0 match, 1 substitution, 2 deletion (ref word missing), 3 insertion (extra hyp word).
    let mut bp = vec![0u8; (n + 1) * (m + 1)];
    let mut prev: Vec<u32> = (0..=m as u32).collect();
    let mut cur = vec![0u32; m + 1];
    for j in 1..=m {
        bp[j] = 3;
    }
    for i in 1..=n {
        cur[0] = i as u32;
        bp[i * (m + 1)] = 2;
        for j in 1..=m {
            let same = r[i - 1].w == h[j - 1].w;
            let diag = prev[j - 1] + if same { 0 } else { 1 };
            let del = prev[j] + 1;
            let ins = cur[j - 1] + 1;
            let (c, b) = if diag <= del && diag <= ins { (diag, if same { 0 } else { 1 }) } else if del <= ins { (del, 2) } else { (ins, 3) };
            cur[j] = c;
            bp[i * (m + 1) + j] = b;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let (mut s, mut d, mut ins, mut pairs) = (0, 0, 0, Vec::new());
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        match bp[i * (m + 1) + j] {
            0 => {
                pairs.push((i - 1, j - 1));
                i -= 1;
                j -= 1;
            }
            1 => {
                s += 1;
                i -= 1;
                j -= 1;
            }
            2 => {
                d += 1;
                i -= 1;
            }
            _ => {
                ins += 1;
                j -= 1;
            }
        }
    }
    pairs.reverse();
    (s, d, ins, pairs)
}

/// Greedy one-to-one mapping from found labels to true speakers by the largest shared counts.
fn map_labels(counts: &[Vec<f64>], n_true: usize) -> Vec<Option<usize>> {
    let n_hyp = counts.len();
    let mut map = vec![None; n_hyp];
    let mut used_t = vec![false; n_true];
    let mut cells: Vec<(f64, usize, usize)> = Vec::new();
    for (hi, row) in counts.iter().enumerate() {
        for (ti, &c) in row.iter().enumerate() {
            if c > 0.0 {
                cells.push((c, hi, ti));
            }
        }
    }
    cells.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, hi, ti) in cells {
        if map[hi].is_none() && !used_t[ti] {
            map[hi] = Some(ti);
            used_t[ti] = true;
        }
    }
    map
}

fn percentile(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f64 * p).round() as usize]
}

pub fn score(truth: &Truth, turns: &[Turn], spans: &[SpeakerSpan]) -> Score {
    let spk_index = |id: &str| truth.speakers.iter().position(|s| s.id == id).unwrap_or(0);
    let n_true = truth.speakers.len();

    // The truth's words in time order, each with its speaker.
    let mut refs: Vec<RefWord> = Vec::new();
    let mut order: Vec<&TruthTurn> = truth.turns.iter().collect();
    order.sort_by(|a, b| a.start.total_cmp(&b.start));
    for t in &order {
        let speaker = spk_index(&t.speaker);
        if t.words.is_empty() {
            let ws = norm(&t.text);
            let step = (t.end - t.start) / ws.len().max(1) as f64;
            for (k, w) in ws.into_iter().enumerate() {
                refs.push(RefWord { w, start: t.start + k as f64 * step, speaker });
            }
        } else {
            for tw in &t.words {
                for w in norm(&tw.text) {
                    refs.push(RefWord { w, start: tw.start, speaker });
                }
            }
        }
    }
    let mut hyps: Vec<HypWord> = Vec::new();
    for t in turns {
        for w in &t.words {
            for nw in norm(&w.text) {
                hyps.push(HypWord { w: nw, start: w.start, speaker: t.speaker });
            }
        }
    }
    let (s, d, ins, pairs) = align(&refs, &hyps);
    let mut sc = Score {
        truth_words: refs.len(),
        hyp_words: hyps.len(),
        substitutions: s,
        deletions: d,
        insertions: ins,
        wer: (s + d + ins) as f64 / refs.len().max(1) as f64,
        truth_speakers: n_true,
        ..Default::default()
    };

    // Which speaker said each word.
    let n_hyp_labels = turns.iter().map(|t| t.speaker as usize + 1).chain(spans.iter().map(|s| s.speaker as usize + 1)).max().unwrap_or(0);
    let mut counts = vec![vec![0.0; n_true]; n_hyp_labels];
    for &(ri, hi) in &pairs {
        counts[hyps[hi].speaker as usize][refs[ri].speaker] += 1.0;
    }
    let map = map_labels(&counts, n_true);
    let right = pairs.iter().filter(|&&(ri, hi)| map[hyps[hi].speaker as usize] == Some(refs[ri].speaker)).count();
    sc.word_speaker_accuracy = right as f64 / pairs.len().max(1) as f64;
    sc.found_speakers = {
        let mut seen: Vec<u32> = turns.iter().map(|t| t.speaker).collect();
        seen.sort();
        seen.dedup();
        seen.len()
    };

    // Word start times.
    let mut errs: Vec<f64> = pairs.iter().map(|&(ri, hi)| (hyps[hi].start - refs[ri].start).abs()).collect();
    sc.word_start_within_half_second = errs.iter().filter(|e| **e <= 0.5).count() as f64 / errs.len().max(1) as f64;
    sc.word_start_error_median = percentile(&mut errs, 0.5);
    sc.word_start_error_p90 = percentile(&mut errs, 0.9);

    // Diarization error rate on 10 ms frames.
    let frames = (truth.duration * 100.0).ceil() as usize + 1;
    let mut t_count = vec![0u8; frames];
    let mut t_label = vec![u8::MAX; frames];
    for t in &truth.turns {
        let k = spk_index(&t.speaker) as u8;
        for f in (t.start * 100.0) as usize..((t.end * 100.0) as usize).min(frames) {
            t_count[f] = t_count[f].saturating_add(1);
            t_label[f] = k;
        }
    }
    let mut h_label = vec![u32::MAX; frames];
    for sp in spans {
        for f in (sp.start * 100.0) as usize..((sp.end * 100.0) as usize).min(frames) {
            h_label[f] = sp.speaker;
        }
    }
    let mut fcounts = vec![vec![0.0; n_true]; n_hyp_labels.max(1)];
    for f in 0..frames {
        if t_count[f] == 1 && h_label[f] != u32::MAX {
            fcounts[h_label[f] as usize][t_label[f] as usize] += 1.0;
        }
    }
    let fmap = map_labels(&fcounts, n_true);
    let (mut speech, mut miss, mut fa, mut conf) = (0.0, 0.0, 0.0, 0.0);
    for f in 0..frames {
        match (t_count[f], h_label[f]) {
            (0, u32::MAX) => {}
            (0, _) => fa += 1.0,
            (1, u32::MAX) => {
                speech += 1.0;
                miss += 1.0;
            }
            (1, h) => {
                speech += 1.0;
                if fmap[h as usize] != Some(t_label[f] as usize) {
                    conf += 1.0;
                }
            }
            _ => {} // overlapped speech: not scored
        }
    }
    let speech = f64::max(speech, 1.0);
    sc.der_missed = miss / speech;
    sc.der_false_alarm = fa / speech;
    sc.der_confusion = conf / speech;
    sc.der = (miss + fa + conf) / speech;
    sc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalising_and_alignment() {
        assert_eq!(norm("Mm-hm. It's 'fine', OK?"), ["mm", "hm", "it's", "fine", "ok"]);
        let r: Vec<RefWord> = ["a", "b", "c", "d"].iter().map(|w| RefWord { w: w.to_string(), start: 0.0, speaker: 0 }).collect();
        let h: Vec<HypWord> = ["a", "x", "c", "d", "e"].iter().map(|w| HypWord { w: w.to_string(), start: 0.0, speaker: 0 }).collect();
        let (s, d, i, pairs) = align(&r, &h);
        assert_eq!((s, d, i), (1, 0, 1));
        assert_eq!(pairs, [(0, 0), (2, 2), (3, 3)]);
    }

    #[test]
    fn mapping_is_one_to_one() {
        let m = map_labels(&[vec![5.0, 1.0], vec![4.0, 0.0], vec![0.0, 3.0]], 2);
        assert_eq!(m, [Some(0), None, Some(1)]);
    }
}
