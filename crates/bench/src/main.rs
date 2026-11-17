//! bench.exe: run the light pack on one recording and report how long each step took, the real-time factor and the
//! peak memory; with a truth file from tools/make_fixtures.py, also the accuracy.
//!
//!   bench <recording.wav> [--truth <file.truth.json>] [--speakers N] [--threads N] [--models DIR]
//!         [--labeller pyannote|pause] [--threshold F] [--window S] [--seg-model F] [--emb-model F] [--gap S] [--chunk SECONDS] [--no-speakers] [--verify]
//!         [--json OUT.json] [--transcript OUT.txt]
//!
//! Nothing here uses the network. Phase 0 reads WAV only.

mod score;

use prism_transcribe_speech::models::{self, LIGHT_PACK};
use prism_transcribe_speech::pipeline::{self, Turn};
use prism_transcribe_speech::sherpa::{Parakeet, PauseLabeller, PyannoteLabeller, SileroVad};
use prism_transcribe_speech::{wav, SpeakerLabeler, VoiceDetector, Word};
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

struct Args {
    input: PathBuf,
    truth: Option<PathBuf>,
    speakers: Option<u32>,
    threads: i32,
    models: PathBuf,
    threshold: Option<f32>,
    labeller: String,
    window: f64,
    seg_model: Option<PathBuf>,
    gap: f64,
    emb_model: Option<PathBuf>,
    chunk: f64,
    diarize: bool,
    verify: bool,
    json: Option<PathBuf>,
    transcript: Option<PathBuf>,
}

fn usage() -> ! {
    eprintln!("usage: bench <recording.wav> [--truth F] [--speakers N] [--threads N] [--models DIR] [--labeller pyannote|pause] [--threshold F] [--window S] [--seg-model F] [--emb-model F] [--gap S] [--chunk S] [--no-speakers] [--verify] [--json F] [--transcript F]");
    std::process::exit(2)
}

fn parse() -> Args {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        input: PathBuf::new(),
        truth: None,
        speakers: None,
        threads: std::thread::available_parallelism().map(|n| n.get().min(8) as i32).unwrap_or(4),
        models: models::default_model_dir(),
        threshold: None,
        labeller: "pause".into(),
        window: 1.5,
        seg_model: None,
        gap: 1.5,
        emb_model: None,
        chunk: 30.0,
        diarize: true,
        verify: false,
        json: None,
        transcript: None,
    };
    let mut input = None;
    while let Some(x) = it.next() {
        let mut val = || it.next().unwrap_or_else(|| usage());
        match x.as_str() {
            "--truth" => a.truth = Some(val().into()),
            "--speakers" => a.speakers = Some(val().parse().unwrap_or_else(|_| usage())),
            "--threads" => a.threads = val().parse().unwrap_or_else(|_| usage()),
            "--models" => a.models = val().into(),
            "--threshold" => a.threshold = Some(val().parse().unwrap_or_else(|_| usage())),
            "--labeller" => a.labeller = val(),
            "--window" => a.window = val().parse().unwrap_or_else(|_| usage()),
            "--seg-model" => a.seg_model = Some(val().into()),
            "--gap" => a.gap = val().parse().unwrap_or_else(|_| usage()),
            "--emb-model" => a.emb_model = Some(val().into()),
            "--chunk" => a.chunk = val().parse().unwrap_or_else(|_| usage()),
            "--json" => a.json = Some(val().into()),
            "--transcript" => a.transcript = Some(val().into()),
            "--no-speakers" => a.diarize = false,
            "--verify" => a.verify = true,
            "-h" | "--help" => usage(),
            s if s.starts_with("--") => usage(),
            s => input = Some(PathBuf::from(s)),
        }
    }
    a.input = input.unwrap_or_else(|| usage());
    a
}

#[derive(Serialize, Default)]
struct Report {
    file: String,
    audio_seconds: f64,
    threads: i32,
    expected_speakers: Option<u32>,
    labeller: String,
    cpu: String,
    seconds_load_models: f64,
    seconds_read_audio: f64,
    seconds_detect_voice: f64,
    seconds_recognize: f64,
    seconds_label_speakers: f64,
    seconds_merge: f64,
    /// Everything after the models are loaded: read, detect, recognize, label, merge.
    seconds_processing: f64,
    /// seconds_processing / audio_seconds. Below 1 is faster than the recording.
    real_time_factor: f64,
    peak_memory_mb: f64,
    speech_spans: usize,
    chunks: usize,
    words: usize,
    turns: usize,
    speakers_found: usize,
    low_confidence_share: f64,
    score: Option<score::Score>,
}

fn main() {
    let a = parse();
    let mut r = Report { file: a.input.display().to_string(), threads: a.threads, expected_speakers: a.speakers, labeller: a.labeller.clone(), cpu: cpu_name(), ..Default::default() };

    let t = Instant::now();
    if let Err(e) = models::verify(&LIGHT_PACK, &a.models, a.verify) {
        eprintln!("The light pack is not complete in {}: {e}", a.models.display());
        std::process::exit(1);
    }
    let vad = SileroVad::new(&a.models, 0.3, 20.0).unwrap_or_else(|e| die(e));
    let asr = Parakeet::new(&a.models, a.threads).unwrap_or_else(|e| die(e));
    let diar: Box<dyn SpeakerLabeler> = match a.labeller.as_str() {
        "pyannote" => Box::new(PyannoteLabeller::new(&a.models, a.threads, a.threshold.unwrap_or(0.5), a.seg_model.as_deref(), a.emb_model.as_deref()).unwrap_or_else(|e| die(e))),
        "pause" => Box::new(PauseLabeller::new(&a.models, a.threads, a.threshold.unwrap_or(0.6), a.window, a.emb_model.as_deref()).unwrap_or_else(|e| die(e))),
        _ => usage(),
    };
    r.seconds_load_models = t.elapsed().as_secs_f64();

    let t0 = Instant::now();
    let audio = wav::read_wav(&a.input).unwrap_or_else(|e| die(e));
    r.audio_seconds = audio.duration();
    r.seconds_read_audio = t0.elapsed().as_secs_f64();
    eprintln!("{}: {:.1} minutes ({} Hz, {} channel(s)), {} threads", r.file, r.audio_seconds / 60.0, audio.source_rate, audio.source_channels, a.threads);

    let t = Instant::now();
    let spans = vad.detect(&audio.samples).unwrap_or_else(|e| die(e));
    r.seconds_detect_voice = t.elapsed().as_secs_f64();
    let chunks = pipeline::chunks_from_speech(&spans, a.chunk, 0.25, r.audio_seconds);
    r.speech_spans = spans.len();
    r.chunks = chunks.len();

    let t = Instant::now();
    let mut words: Vec<Word> = Vec::new();
    // BENCH_WORDS=<file>: reuse words recognized by an earlier run (for comparing speaker labellers only; the
    // recognize time is then not a measurement).
    let cache = std::env::var_os("BENCH_WORDS").map(PathBuf::from);
    if let Some(Ok(text)) = cache.as_ref().map(std::fs::read_to_string) {
        words = serde_json::from_str(&text).unwrap_or_else(|e| die(e));
        eprintln!("  reusing {} recognized words from {}", words.len(), cache.as_ref().unwrap().display());
    }
    let need = words.is_empty();
    for (i, c) in chunks.iter().enumerate().filter(|_| need) {
        words.extend(pipeline::recognize_chunk(&asr, &audio.samples, *c, None, a.gap).unwrap_or_else(|e| die(e)));
        if i % 20 == 19 || i + 1 == chunks.len() {
            eprint!("\r  recognized {}/{} chunks ({:.0}%)   ", i + 1, chunks.len(), c.end / r.audio_seconds * 100.0);
            let _ = std::io::stderr().flush();
        }
    }
    eprintln!();
    r.seconds_recognize = t.elapsed().as_secs_f64();
    if let Some(p) = cache.as_ref().filter(|p| !p.exists()) {
        std::fs::write(p, serde_json::to_string(&words).unwrap()).unwrap_or_else(|e| die(e));
    }

    let t = Instant::now();
    let speaker_spans = if a.diarize { diar.label(&audio.samples, a.speakers).unwrap_or_else(|e| die(e)) } else { Vec::new() };
    r.seconds_label_speakers = t.elapsed().as_secs_f64();

    let t = Instant::now();
    let labels = pipeline::speakers_for_words(&words, &speaker_spans);
    r.words = words.len();
    r.low_confidence_share = words.iter().filter(|w| w.confidence < 0.6).count() as f64 / words.len().max(1) as f64;
    let turns: Vec<Turn> = pipeline::turns(words, &labels);
    r.turns = turns.len();
    r.speakers_found = {
        let mut v: Vec<u32> = turns.iter().map(|t| t.speaker).collect();
        v.sort();
        v.dedup();
        v.len()
    };
    r.seconds_merge = t.elapsed().as_secs_f64();
    r.seconds_processing = t0.elapsed().as_secs_f64();
    r.real_time_factor = r.seconds_processing / r.audio_seconds.max(1e-9);
    r.peak_memory_mb = peak_memory_mb();

    if let Some(tp) = &a.truth {
        let text = std::fs::read_to_string(tp).unwrap_or_else(|e| die(e));
        let truth: score::Truth = serde_json::from_str(&text).unwrap_or_else(|e| die(e));
        r.score = Some(score::score(&truth, &turns, &speaker_spans));
    }

    if std::env::var_os("BENCH_SPANS").is_some() { for s in &speaker_spans { eprintln!("SPAN {} {:.2} {:.2}", s.speaker, s.start, s.end); } }
    print_report(&r);
    if let Some(p) = &a.json {
        std::fs::write(p, serde_json::to_string_pretty(&r).unwrap() + "\n").unwrap_or_else(|e| die(e));
    }
    if let Some(p) = &a.transcript {
        let mut out = String::new();
        for t in &turns {
            out.push_str(&format!("[{}] Speaker {}: {}\n\n", hms(t.start), t.speaker + 1, t.text()));
        }
        std::fs::write(p, out).unwrap_or_else(|e| die(e));
    }
}

fn print_report(r: &Report) {
    println!("file                 {}", r.file);
    println!("audio                {:.1} min", r.audio_seconds / 60.0);
    println!("cpu                  {} ({} threads)", r.cpu, r.threads);
    println!("load models          {:.1} s", r.seconds_load_models);
    println!("read audio           {:.1} s", r.seconds_read_audio);
    println!("detect voice         {:.1} s  ({} spans, {} chunks)", r.seconds_detect_voice, r.speech_spans, r.chunks);
    println!("recognize            {:.1} s", r.seconds_recognize);
    println!("label speakers       {:.1} s  ({})", r.seconds_label_speakers, r.labeller);
    println!("merge                {:.2} s", r.seconds_merge);
    println!("processing (wall)    {:.1} s = {:.2} min", r.seconds_processing, r.seconds_processing / 60.0);
    println!("real-time factor     {:.3}", r.real_time_factor);
    println!("peak memory          {:.0} MB", r.peak_memory_mb);
    println!("words / turns        {} / {}", r.words, r.turns);
    println!("speakers found       {}", r.speakers_found);
    println!("words below 0.6      {:.1}%", r.low_confidence_share * 100.0);
    if let Some(s) = &r.score {
        println!("--- against the truth file");
        println!("word error rate      {:.2}%  (S {} D {} I {} of {} words)", s.wer * 100.0, s.substitutions, s.deletions, s.insertions, s.truth_words);
        println!("speakers             {} found, {} true", s.found_speakers, s.truth_speakers);
        println!("word speaker right   {:.1}%", s.word_speaker_accuracy * 100.0);
        println!("DER (no overlap)     {:.1}%  (missed {:.1}, false alarm {:.1}, confusion {:.1})", s.der * 100.0, s.der_missed * 100.0, s.der_false_alarm * 100.0, s.der_confusion * 100.0);
        println!("word start error     median {:.3} s, 90th pct {:.3} s, {:.1}% within 0.5 s", s.word_start_error_median, s.word_start_error_p90, s.word_start_within_half_second * 100.0);
    }
}

fn hms(t: f64) -> String {
    let s = t as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn die(e: impl std::fmt::Display) -> ! {
    eprintln!("bench: {e}");
    std::process::exit(1)
}

#[cfg(windows)]
fn peak_memory_mb() -> f64 {
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let mut c: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) != 0 {
            return c.PeakWorkingSetSize as f64 / (1024.0 * 1024.0);
        }
    }
    f64::NAN
}

#[cfg(not(windows))]
fn peak_memory_mb() -> f64 {
    f64::NAN
}

fn cpu_name() -> String {
    #[cfg(windows)]
    {
        if let Ok(out) = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "(Get-CimInstance Win32_Processor | Select-Object -First 1).Name"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown".into())
}
