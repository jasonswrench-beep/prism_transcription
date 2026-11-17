// Phase 0 diagnostic: recognize one stretch of a recording and print the words with times.
use prism_transcribe_speech::{models, sherpa::Parakeet, wav::read_wav, SpeechRecognizer};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let audio = read_wav(a[1].as_ref()).unwrap();
    let (s, e): (f64, f64) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let asr = Parakeet::new(&models::default_model_dir(), 8).unwrap();
    let w = asr.recognize(&audio.samples[(s * 16000.0) as usize..(e * 16000.0) as usize], s, None).unwrap();
    let last = w.last().map(|x| x.end).unwrap_or(s);
    println!("{s}-{e}: {} words, last ends {last:.1}", w.len());
    println!("{}", w.iter().map(|x| format!("{}@{:.1}", x.text, x.start)).collect::<Vec<_>>().join(" "));
}
