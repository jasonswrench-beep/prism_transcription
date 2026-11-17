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
