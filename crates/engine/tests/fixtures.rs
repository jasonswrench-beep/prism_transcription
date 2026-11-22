//! Phase 1's tests on the real models: the two fixture clips become transcripts with at least two speakers and word
//! times that only go forward, and export to Word and text. Skipped (with a message) when the light pack is not
//! installed, so `cargo test` still passes on a machine without the models.

use prism_transcribe_core::export::ExportOptions;
use prism_transcribe_core::project::{ImportFields, Project};
use prism_transcribe_engine::{self as engine, ExportFormat, Settings};
use prism_transcribe_speech::models;
use std::path::Path;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

fn models_ready(settings: &Settings) -> bool {
    match models::verify(&models::LIGHT_PACK, &settings.model_dir(), false) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("light pack not installed ({e}): skipped");
            false
        }
    }
}

#[test]
fn fixtures_transcribe_with_speakers_and_forward_times() {
    let settings = Settings { offline: true, ..Settings::load() };
    if !models_ready(&settings) {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("proj");
    Project::create(&dir, "Fixture test").unwrap();
    for (clip, speakers) in [("two-speaker.wav", 2usize), ("four-speaker.wav", 4)] {
        let id = engine::import(&dir, &fixture(clip), ImportFields::default(), false, None, &mut |_| true).unwrap();
        let t = engine::transcribe(&dir, &id, &settings, &mut |_| true).unwrap();
        let p = Project::open(&dir).unwrap();
        let found = t.speakers.len();
        assert!(found >= 2, "{clip}: {found} speakers");
        assert_eq!(found, speakers, "{clip}: the fixture's speakers are all found");
        let words: Vec<_> = t.turns.iter().flat_map(|x| &x.words).collect();
        assert!(words.len() > 100, "{clip}: {} words", words.len());
        assert!(words.windows(2).all(|w| w[0].start <= w[1].start), "{clip}: word times go forward");
        assert!(words.iter().all(|w| w.end >= w.start && (0.0..=1.0).contains(&w.confidence)));
        assert_eq!(t.snapshots.len(), 1, "version 0 is kept");
        assert!(!p.temp_dir().join(format!("{id}.16k.wav")).exists(), "temporary audio is deleted when the job ends");

        // Saved, reloaded, exported.
        let back = p.load_transcript(&id).unwrap().unwrap();
        assert_eq!(back, t);
        let docx = d.path().join(format!("{id}.docx"));
        engine::export(&p, &id, ExportFormat::Word, &docx, &ExportOptions::default()).unwrap();
        assert!(std::fs::metadata(&docx).unwrap().len() > 1000);
        let txt = d.path().join(format!("{id}.txt"));
        engine::export(&p, &id, ExportFormat::Text, &txt, &ExportOptions::default()).unwrap();
        let text = std::fs::read_to_string(&txt).unwrap();
        assert!(text.contains("[00:00:0") && text.contains("Speaker 1:") && text.contains("Speaker 2:"), "{text}");
    }
}

/// A WAV of `seconds` of silence, 16 kHz mono 16-bit.
fn silent_wav(path: &Path, seconds: u32) {
    let n = 16_000 * seconds;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&16_000u32.to_le_bytes());
    b.extend_from_slice(&32_000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n * 2).to_le_bytes());
    b.resize(b.len() + (n * 2) as usize, 0);
    std::fs::write(path, b).unwrap();
}

#[test]
fn a_cancelled_or_failed_second_run_keeps_the_first_transcript_and_silence_says_so() {
    use prism_transcribe_core::project::MediaStatus;
    let settings = Settings { offline: true, ..Settings::load() };
    if !models_ready(&settings) {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("proj");
    Project::create(&dir, "Second run").unwrap();
    let id = engine::import(&dir, &fixture("two-speaker.wav"), ImportFields::default(), false, None, &mut |_| true).unwrap();
    let first = engine::transcribe(&dir, &id, &settings, &mut |_| true).unwrap();

    // Cancelled part way: the first transcript is still the file's, and the card still offers Edit and Export.
    let mut calls = 0;
    let r = engine::transcribe(&dir, &id, &settings, &mut |_| {
        calls += 1;
        calls < 3
    });
    assert!(r.is_err());
    let p = Project::open(&dir).unwrap();
    assert_eq!(p.media(&id).unwrap().status, MediaStatus::Transcribed);
    assert_eq!(p.load_transcript(&id).unwrap().unwrap().turns, first.turns);

    // A failure part way (the recording moved away after its 16 kHz copy was deleted) keeps it too, and says why.
    let moved = d.path().join("moved.wav");
    std::fs::copy(fixture("two-speaker.wav"), &moved).unwrap();
    let mid = engine::import(&dir, &moved, ImportFields::default(), false, None, &mut |_| true).unwrap();
    engine::transcribe(&dir, &mid, &settings, &mut |_| true).unwrap();
    std::fs::remove_file(&moved).unwrap();
    assert!(engine::transcribe(&dir, &mid, &settings, &mut |_| true).is_err());
    let p = Project::open(&dir).unwrap();
    let m = p.media(&mid).unwrap();
    assert_eq!(m.status, MediaStatus::Transcribed);
    assert!(m.error.as_deref().unwrap_or("").contains("earlier transcript is kept"), "{:?}", m.error);
    assert!(p.load_transcript(&mid).unwrap().is_some());

    // Silence: no transcript, and the reason in words.
    let quiet = d.path().join("silence.wav");
    silent_wav(&quiet, 6);
    let sid = engine::import(&dir, &quiet, ImportFields::default(), false, None, &mut |_| true).unwrap();
    let e = engine::transcribe(&dir, &sid, &settings, &mut |_| true).err().unwrap();
    assert!(matches!(e, engine::EngineError::NoSpeech(_)), "{e}");
    let p = Project::open(&dir).unwrap();
    assert_eq!(p.media(&sid).unwrap().status, MediaStatus::Failed);
    assert!(p.load_transcript(&sid).unwrap().is_none());
    assert!(e.to_string().starts_with("no speech was heard in silence"), "{e}");
}
