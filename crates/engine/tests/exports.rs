//! Every export format through the engine (T7), for one file and for several, with no speech models needed.

use prism_transcribe_core::export::ExportOptions;
use prism_transcribe_core::model::{Role, Speaker, Transcript, Turn, Word};
use prism_transcribe_core::project::Project;
use prism_transcribe_engine::{export_files, ExportFormat};
use std::io::Read;

fn project_with_two(dir: &std::path::Path) -> Project {
    let mut p = Project::create(&dir.join("proj"), "Study A").unwrap();
    for (n, who) in [(1, "P07"), (2, "P08")] {
        let rec = dir.join(format!("int{n}.wav"));
        std::fs::write(&rec, vec![1u8; 4096]).unwrap();
        let id = p.add_media(&rec, Default::default(), false).unwrap();
        let mut t = Transcript::new(&id);
        t.speakers = vec![
            Speaker { id: "S1".into(), label: "Speaker 1".into(), name: Some("Interviewer".into()), role: Role::Interviewer, attributes: Default::default() },
            Speaker { id: "S2".into(), label: "Speaker 2".into(), name: Some(who.into()), role: Role::Participant, attributes: Default::default() },
        ];
        let w = |t: &str, a: f64, s: &str| Word { text: t.into(), start: a, end: a + 0.4, confidence: 0.9, speaker: s.into() };
        t.turns = vec![
            Turn { speaker: "S1".into(), start: 1.0, end: 1.9, words: vec![w("How", 1.0, "S1"), w("was it?", 1.5, "S1")] },
            Turn { speaker: "S2".into(), start: 2.5, end: 3.4, words: vec![w("Fine,", 2.5, "S2"), w("thanks.", 3.0, "S2")] },
        ];
        p.save_transcript(&t).unwrap();
    }
    Project::open(&dir.join("proj")).unwrap()
}

#[test]
fn every_format_for_one_and_for_many() {
    let tmp = tempfile::tempdir().unwrap();
    let p = project_with_two(tmp.path());
    let one = vec!["F001".to_string()];
    let both = vec!["F001".to_string(), "F002".to_string()];
    let o = ExportOptions { speaker_names: true, ..Default::default() };
    for ext in ["docx", "txt", "srt", "vtt", "csv", "xlsx", "prismtx", "qdpx"] {
        let out = tmp.path().join(format!("one.{ext}"));
        export_files(&p, &one, ExportFormat::from_path(&out).unwrap(), &out, &o, false).unwrap();
        assert!(std::fs::metadata(&out).unwrap().len() > 0, "{ext}");
    }
    let srt = std::fs::read_to_string(tmp.path().join("one.srt")).unwrap();
    assert!(srt.contains("Interviewer: How was it?"), "{srt}");

    // The table and the packages hold every file; Word holds one.
    let out = tmp.path().join("all.csv");
    export_files(&p, &both, ExportFormat::Csv, &out, &o, false).unwrap();
    let csv = std::fs::read_to_string(&out).unwrap();
    assert_eq!(csv.lines().count(), 5, "a header and two turns per file: {csv}");
    assert!(csv.contains("P08"));
    let out = tmp.path().join("all.qdpx");
    export_files(&p, &both, ExportFormat::Qdpx, &out, &o, false).unwrap();
    let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    let mut qde = String::new();
    z.by_name("project.qde").unwrap().read_to_string(&mut qde).unwrap();
    assert_eq!(qde.matches("<AudioSource ").count(), 2);
    assert_eq!(qde.matches("<Case ").count(), 4, "two speakers in each of two recordings");
    let out = tmp.path().join("all.prismtx");
    export_files(&p, &both, ExportFormat::Prismtx, &out, &o, false).unwrap();
    let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    assert!(z.by_name("F002/transcript.json").is_ok());
    let mut m = String::new();
    z.by_name("manifest.json").unwrap().read_to_string(&mut m).unwrap();
    assert!(m.contains("\"deidentified\": false"));
    assert!(export_files(&p, &both, ExportFormat::Word, &tmp.path().join("x.docx"), &o, false).is_err());
}
