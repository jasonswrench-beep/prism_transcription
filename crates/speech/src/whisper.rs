//! The second plugin: whisper.cpp, for the heavy pack (Whisper large-v3-turbo, about 99 languages).
//!
//! whisper.cpp runs as a **separate program**, `whisper-cli.exe`, as OpenStage Author's C5 already does (its ADR
//! 0002), so a build for the graphics card (Vulkan or CUDA) can replace the CPU build without changing this crate.
//! Only the recognize job goes through it; voice detection and speaker labelling stay with sherpa-onnx, so the heavy
//! pack needs the light pack beside it.
//!
//! Each call writes the stretches of speech as 16 kHz WAV files in a work folder, runs the program once for all of
//! them (so the 1.6 GB model is loaded once per batch, not once per stretch), reads its full JSON output and deletes
//! the files. Words are built from Whisper's tokens with the same rule as Parakeet's, and a word's confidence is the
//! geometric mean of its tokens' probabilities. Whisper's times come from its own token timestamps and are looser
//! than Parakeet's (the spec: "the editor shows them as approximate").
//!
//! Whisper can write text over long silences. Only the speech the voice detector found is ever given to it, and
//! the job drops any word that falls outside detected speech ([`crate::job::JobOptions::drop_outside_speech`]).

use crate::pipeline::words_from_tokens;
use crate::{Result, SpeechError, SpeechRecognizer, Word, SAMPLE_RATE};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The program's file name.
pub const PROGRAM: &str = "whisper-cli.exe";

/// Where whisper.cpp is: the path given (Settings), `PRISM_WHISPER`, a `whisper` folder beside this program,
/// beside this program, or a `whisper.cpp` folder in the shared model folder.
pub fn find_program(given: Option<&Path>, model_dir: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(g) = given {
        candidates.push(if g.is_dir() { g.join(PROGRAM) } else { g.to_path_buf() });
    }
    if let Some(e) = std::env::var_os("PRISM_WHISPER").map(PathBuf::from) {
        candidates.push(if e.is_dir() { e.join(PROGRAM) } else { e });
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        candidates.push(dir.join("whisper").join(PROGRAM));
        candidates.push(dir.join(PROGRAM));
    }
    candidates.push(model_dir.join("whisper.cpp").join(PROGRAM));
    candidates.into_iter().find(|p| p.is_file())
}

/// The languages Whisper large-v3 recognizes (ISO 639-1 codes, from whisper.cpp's language table; "haw" and "yue"
/// have no two-letter code and are left out).
pub const WHISPER_LANGUAGES: &[&str] = &[
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro",
    "da", "hu", "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br",
    "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am",
    "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "ln", "ha", "ba", "jw", "su",
];

pub struct WhisperCli {
    program: PathBuf,
    model: PathBuf,
    threads: i32,
    work: PathBuf,
}

impl WhisperCli {
    /// `work` is a folder the plugin may write its temporary WAV and JSON files in (the project's `temp`).
    pub fn new(program: &Path, model: &Path, threads: i32, work: &Path) -> Result<Self> {
        if !program.is_file() {
            return Err(SpeechError::MissingModel(format!("{} (the whisper.cpp program)", program.display())));
        }
        if !model.is_file() {
            return Err(SpeechError::MissingModel(model.display().to_string()));
        }
        Ok(Self { program: program.into(), model: model.into(), threads: threads.max(1), work: work.into() })
    }

    fn run(&self, pieces: &[(&[f32], f64)], language: Option<&str>) -> Result<Vec<Vec<Word>>> {
        if pieces.is_empty() {
            return Ok(Vec::new());
        }
        let dir = self.work.join(format!("whisper-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let result = (|| {
            let mut wavs = Vec::new();
            for (i, (samples, _)) in pieces.iter().enumerate() {
                let p = dir.join(format!("piece-{i:04}.wav"));
                write_wav(&p, samples)?;
                wavs.push(p);
            }
            let mut cmd = std::process::Command::new(&self.program);
            cmd.arg("-m").arg(&self.model).args(["-t", &self.threads.to_string(), "-l", language.unwrap_or("auto"), "-ojf", "-np"]);
            for w in &wavs {
                cmd.arg("-f").arg(w);
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            }
            let out = cmd.output()?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                let tail: Vec<&str> = err.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
                return Err(SpeechError::NoResult { what: format!("whisper.cpp (exit {}): {}", out.status.code().unwrap_or(-1), tail.join(" / ")) });
            }
            let mut all = Vec::new();
            for (w, (_, offset)) in wavs.iter().zip(pieces) {
                let json = std::fs::read_to_string(PathBuf::from(format!("{}.json", w.display())))
                    .map_err(|_| SpeechError::NoResult { what: format!("whisper.cpp output for {}", w.display()) })?;
                all.push(words_from_json(&json, *offset)?);
            }
            Ok(all)
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec { channels: 1, sample_rate: SAMPLE_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| SpeechError::Audio(e.to_string()))?;
    for &x in samples {
        w.write_sample((x.clamp(-1.0, 1.0) * 32767.0).round() as i16).map_err(|e| SpeechError::Audio(e.to_string()))?;
    }
    w.finalize().map_err(|e| SpeechError::Audio(e.to_string()))
}

#[derive(Deserialize)]
struct Out {
    #[serde(default)]
    transcription: Vec<Segment>,
}
#[derive(Deserialize)]
struct Segment {
    #[serde(default)]
    tokens: Vec<Token>,
}
#[derive(Deserialize)]
struct Token {
    text: String,
    offsets: Offsets,
    #[serde(default)]
    p: f32,
}
#[derive(Deserialize)]
struct Offsets {
    from: i64,
    to: i64,
}

/// Words from whisper-cli's `-ojf` JSON. Special tokens (`[_BEG_]`, `[_TT_150]` ...) are left out; times are in
/// milliseconds from the start of the piece.
pub fn words_from_json(json: &str, offset: f64) -> Result<Vec<Word>> {
    let out: Out = serde_json::from_str(json).map_err(|e| SpeechError::NoResult { what: format!("whisper.cpp output ({e})") })?;
    let (mut toks, mut ts, mut du, mut lp) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for t in out.transcription.iter().flat_map(|s| &s.tokens) {
        if t.text.starts_with("[_") || t.text.trim().is_empty() {
            continue;
        }
        toks.push(t.text.clone());
        ts.push(t.offsets.from as f32 / 1000.0);
        du.push(((t.offsets.to - t.offsets.from).max(0) as f32 / 1000.0).max(0.02));
        lp.push(t.p.clamp(1e-6, 1.0).ln());
    }
    Ok(words_from_tokens(&toks, &ts, &du, &lp, offset))
}

impl SpeechRecognizer for WhisperCli {
    fn recognize(&self, samples: &[f32], offset: f64, language: Option<&str>) -> Result<Vec<Word>> {
        Ok(self.run(&[(samples, offset)], language)?.pop().unwrap_or_default())
    }

    fn recognize_many(&self, pieces: &[(&[f32], f64)], language: Option<&str>) -> Result<Vec<Vec<Word>>> {
        self.run(pieces, language)
    }

    fn languages(&self) -> &[&'static str] {
        WHISPER_LANGUAGES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_json_becomes_words() {
        // Cut down from whisper-cli 1.9.5's -ojf output for the two-speaker fixture.
        let json = r#"{"result":{"language":"en"},"transcription":[{"tokens":[
            {"text":"[_BEG_]","offsets":{"from":0,"to":0},"p":0.5},
            {"text":" Thanks","offsets":{"from":490,"to":490},"p":0.82},
            {"text":" for","offsets":{"from":520,"to":740},"p":0.99},
            {"text":" Rivers","offsets":{"from":5420,"to":5530},"p":0.99},
            {"text":"ide","offsets":{"from":5580,"to":5590},"p":1.0},
            {"text":"?","offsets":{"from":5860,"to":5870},"p":0.96},
            {"text":"[_TT_300]","offsets":{"from":6000,"to":6000},"p":0.54}]}]}"#;
        let w = words_from_json(json, 10.0).unwrap();
        assert_eq!(w.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(), ["Thanks", "for", "Riverside?"]);
        assert!((w[0].start - 10.49).abs() < 1e-6);
        assert!((w[1].end - 10.74).abs() < 1e-6);
        assert!((w[0].confidence - 0.82).abs() < 1e-4);
    }

    #[test]
    fn languages_cover_far_more_than_parakeet() {
        assert!(WHISPER_LANGUAGES.len() > 90);
        assert!(crate::models::PARAKEET_LANGUAGES.iter().all(|l| WHISPER_LANGUAGES.contains(l)));
        assert!(WHISPER_LANGUAGES.contains(&"ja") && WHISPER_LANGUAGES.contains(&"sw"));
    }
}
