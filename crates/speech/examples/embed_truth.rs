// Phase 0 diagnostic: embed each true turn of a fixture and print the mean cosine similarity within and across
// speakers, to see whether the embedding model separates the voices at all.
use prism_transcribe_speech::wav::read_wav;
use sherpa_onnx as so;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (wav, truth, model) = (&a[1], &a[2], &a[3]);
    let audio = read_wav(wav.as_ref()).unwrap();
    let t: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(truth).unwrap()).unwrap();
    let mut c = so::SpeakerEmbeddingExtractorConfig::default();
    c.model = Some(model.clone());
    c.num_threads = 4;
    let ex = so::SpeakerEmbeddingExtractor::create(&c).unwrap();
    let max_len: f64 = a.get(4).map(|s| s.parse().unwrap()).unwrap_or(999.0);
    let mut items: Vec<(String, Vec<f32>)> = Vec::new();
    for turn in t["turns"].as_array().unwrap() {
        if turn["overlap"].as_bool().unwrap_or(false) { continue; }
        let s = turn["start"].as_f64().unwrap();
        let e = turn["end"].as_f64().unwrap().min(s + max_len);
        let st = ex.create_stream().unwrap();
        st.accept_waveform(16000, &audio.samples[(s * 16000.0) as usize..(e * 16000.0) as usize]);
        st.input_finished();
        if !ex.is_ready(&st) { continue; }
        let mut v = ex.compute(&st).unwrap();
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter_mut().for_each(|x| *x /= n);
        items.push((turn["speaker"].as_str().unwrap().to_string(), v));
    }
    let spk: Vec<String> = { let mut s: Vec<String> = items.iter().map(|i| i.0.clone()).collect(); s.sort(); s.dedup(); s };
    for x in &spk {
        let row: Vec<String> = spk.iter().map(|y| {
            let (mut sum, mut n) = (0.0f32, 0);
            for (i, a) in items.iter().enumerate() { for (j, b) in items.iter().enumerate() {
                if i != j && &a.0 == x && &b.0 == y { sum += a.1.iter().zip(&b.1).map(|(p, q)| p * q).sum::<f32>(); n += 1; }
            }}
            format!("{y}:{:.2}", sum / n.max(1) as f32)
        }).collect();
        println!("{x} -> {}", row.join("  "));
    }
}
