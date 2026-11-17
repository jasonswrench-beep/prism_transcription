// Phase 0 diagnostic: print the voice detector's spans between two times.
use prism_transcribe_speech::{models, sherpa::SileroVad, wav::read_wav, VoiceDetector};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let audio = read_wav(a[1].as_ref()).unwrap();
    let (s, e): (f64, f64) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let (ms, mx): (f32, f32) = (a.get(4).map(|v| v.parse().unwrap()).unwrap_or(0.3), a.get(5).map(|v| v.parse().unwrap()).unwrap_or(20.0));
    let vad = SileroVad::new(&models::default_model_dir(), ms, mx).unwrap();
    let spans = vad.detect(&audio.samples).unwrap();
    let total: f64 = spans.iter().map(|x| x.end - x.start).sum();
    println!("{} spans, {:.1} min of speech in {:.1} min", spans.len(), total / 60.0, audio.duration() / 60.0);
    for x in spans.iter().filter(|x| x.end > s && x.start < e) { println!("{:.2}-{:.2} ({:.1} s)", x.start, x.end, x.end - x.start); }
    let mut long: Vec<_> = spans.iter().map(|x| x.end - x.start).collect(); long.sort_by(|a, b| b.total_cmp(a));
    println!("longest: {:?}", &long[..5.min(long.len())]);
}
