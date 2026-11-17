//! prism-transcribe-cli: the app's work from the command line, for tests, batch scripts and machines without the
//! window. Nothing here uses the network except `models install`, and that refuses when Offline is on.
//!
//!   prism-transcribe-cli models status|install|import-zip <zip>|export-zip <zip>   [--dir <model folder>]
//!   prism-transcribe-cli new <folder> <name>
//!   prism-transcribe-cli import <project> <recording> [--label L] [--date D] [--speakers N] [--language xx] [--copy]
//!   prism-transcribe-cli transcribe <project> <id>
//!   prism-transcribe-cli export <project> <id> <out.docx|out.txt> [--timestamps turn|none|<seconds>] [--line-numbers]
//!   prism-transcribe-cli run <recording> --out <out.docx|out.txt> [--speakers N] [--project <folder>]

use prism_transcribe_core::export::{ExportOptions, Timestamps};
use prism_transcribe_core::project::{ImportFields, Project};
use prism_transcribe_engine::{self as engine, ExportFormat, Progress, Settings};
use prism_transcribe_speech::{install, models};
use std::io::Write;
use std::path::PathBuf;

fn die(e: impl std::fmt::Display) -> ! {
    eprintln!("prism-transcribe-cli: {e}");
    std::process::exit(1)
}

fn usage() -> ! {
    eprintln!("{}", include_str!("main.rs").lines().skip(3).take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!")).collect::<Vec<_>>().join("\n"));
    std::process::exit(2)
}

struct Args {
    pos: Vec<String>,
    opts: std::collections::HashMap<String, String>,
    flags: Vec<String>,
}

fn parse() -> Args {
    let mut a = Args { pos: Vec::new(), opts: Default::default(), flags: Vec::new() };
    let mut it = std::env::args().skip(1).peekable();
    while let Some(x) = it.next() {
        if let Some(k) = x.strip_prefix("--") {
            if matches!(k, "copy" | "line-numbers") {
                a.flags.push(k.into());
            } else {
                a.opts.insert(k.into(), it.next().unwrap_or_else(|| usage()));
            }
        } else {
            a.pos.push(x);
        }
    }
    a
}

fn progress_line(p: Progress) -> bool {
    match &p {
        Progress::DetectVoice => eprint!("\r  finding speech...                    "),
        Progress::Recognize { done, total, seconds } => eprint!("\r  recognizing {done}/{total} ({:.0} min in)      ", seconds / 60.0),
        Progress::LabelSpeakers => eprint!("\r  labelling speakers...                "),
        Progress::Finished => eprintln!("\r  done.                                 "),
    }
    let _ = std::io::stderr().flush();
    true
}

fn fields(a: &Args) -> ImportFields {
    ImportFields {
        label: a.opts.get("label").cloned().unwrap_or_default(),
        date: a.opts.get("date").cloned(),
        interviewer: a.opts.get("interviewer").cloned(),
        expected_speakers: a.opts.get("speakers").map(|s| s.parse().unwrap_or_else(|_| usage())),
        language: a.opts.get("language").cloned(),
        note: None,
    }
}

fn export_opts(a: &Args) -> ExportOptions {
    let timestamps = match a.opts.get("timestamps").map(String::as_str) {
        None | Some("turn") => Timestamps::PerTurn,
        Some("none") => Timestamps::None,
        Some(n) => Timestamps::Every { seconds: n.parse().unwrap_or_else(|_| usage()) },
    };
    ExportOptions { timestamps, line_numbers: a.flags.iter().any(|f| f == "line-numbers") }
}

fn main() {
    let a = parse();
    let mut settings = Settings::load();
    if let Some(d) = a.opts.get("dir") {
        settings.model_dir = Some(PathBuf::from(d));
    }
    let model_dir = settings.model_dir();
    match a.pos.first().map(String::as_str) {
        Some("models") => match a.pos.get(1).map(String::as_str) {
            Some("status") => match models::verify(&models::LIGHT_PACK, &model_dir, a.flags.is_empty()) {
                Ok(()) => println!("{} is installed in {}", models::LIGHT_PACK.name, model_dir.display()),
                Err(e) => {
                    println!("The light pack is not installed in {}: {e}", model_dir.display());
                    std::process::exit(1)
                }
            },
            Some("install") => {
                let mut last = 0u64;
                install::install_light_pack(&model_dir, settings.offline, &mut |p| {
                    if p.pack_done - last > 20_000_000 || p.pack_done == p.pack_total {
                        last = p.pack_done;
                        eprint!("\r  {} MB of {} MB ({})            ", p.pack_done / 1_000_000, p.pack_total / 1_000_000, p.file);
                    }
                    true
                })
                .unwrap_or_else(|e| die(e));
                eprintln!();
                println!("Installed and checked: {}", model_dir.display());
            }
            Some("import-zip") => {
                let z = a.pos.get(2).unwrap_or_else(|| usage());
                install::import_zip(&models::LIGHT_PACK, z.as_ref(), &model_dir).unwrap_or_else(|e| die(e));
                println!("Imported and checked: {}", model_dir.display());
            }
            Some("export-zip") => {
                let z = a.pos.get(2).unwrap_or_else(|| usage());
                install::export_zip(&models::LIGHT_PACK, &model_dir, z.as_ref()).unwrap_or_else(|e| die(e));
                println!("Wrote {z}");
            }
            _ => usage(),
        },
        Some("new") => {
            let (dir, name) = (a.pos.get(1).unwrap_or_else(|| usage()), a.pos.get(2).unwrap_or_else(|| usage()));
            Project::create(dir.as_ref(), name).unwrap_or_else(|e| die(e));
            println!("Created {dir}");
        }
        Some("import") => {
            let (proj, file) = (a.pos.get(1).unwrap_or_else(|| usage()), a.pos.get(2).unwrap_or_else(|| usage()));
            let id = engine::import(proj.as_ref(), file.as_ref(), fields(&a), a.flags.iter().any(|f| f == "copy"), settings.ffmpeg().as_deref(), &mut |_| true).unwrap_or_else(|e| die(e));
            let p = Project::open(proj.as_ref()).unwrap_or_else(|e| die(e));
            let m = p.media(&id).unwrap();
            println!("{id}\t{}\t{:.1} min{}", m.fields.label, m.duration / 60.0, if m.has_video { "\tvideo" } else { "" });
        }
        Some("transcribe") => {
            let (proj, id) = (a.pos.get(1).unwrap_or_else(|| usage()), a.pos.get(2).unwrap_or_else(|| usage()));
            let t = engine::transcribe(proj.as_ref(), id, &settings, &mut progress_line).unwrap_or_else(|e| die(e));
            println!("{id}: {} turns, {} speakers, {:.1} s", t.turns.len(), t.speakers.len(), t.provenance.seconds);
        }
        Some("export") => {
            let (proj, id, out) = (a.pos.get(1).unwrap_or_else(|| usage()), a.pos.get(2).unwrap_or_else(|| usage()), a.pos.get(3).unwrap_or_else(|| usage()));
            let p = Project::open(proj.as_ref()).unwrap_or_else(|e| die(e));
            let fmt = ExportFormat::from_path(out.as_ref()).unwrap_or_else(|| die("the output must end in .docx or .txt"));
            engine::export(&p, id, fmt, out.as_ref(), &export_opts(&a)).unwrap_or_else(|e| die(e));
            println!("Wrote {out}");
        }
        Some("run") => {
            let file = a.pos.get(1).unwrap_or_else(|| usage());
            let out = PathBuf::from(a.opts.get("out").unwrap_or_else(|| usage()));
            let fmt = ExportFormat::from_path(&out).unwrap_or_else(|| die("--out must end in .docx or .txt"));
            let dir = a.opts.get("project").map(PathBuf::from).unwrap_or_else(|| out.with_extension("project"));
            if !dir.join(prism_transcribe_core::project::PROJECT_FILE).exists() {
                Project::create(&dir, "Command-line run").unwrap_or_else(|e| die(e));
            }
            let id = engine::import(&dir, file.as_ref(), fields(&a), false, settings.ffmpeg().as_deref(), &mut |_| true).unwrap_or_else(|e| die(e));
            let t = engine::transcribe(&dir, &id, &settings, &mut progress_line).unwrap_or_else(|e| die(e));
            let p = Project::open(&dir).unwrap_or_else(|e| die(e));
            engine::export(&p, &id, fmt, &out, &export_opts(&a)).unwrap_or_else(|e| die(e));
            println!("{}: {} turns, {} speakers, {:.1} min of audio in {:.1} min; wrote {}", file, t.turns.len(), t.speakers.len(), p.media(&id).unwrap().duration / 60.0, t.provenance.seconds / 60.0, out.display());
        }
        _ => usage(),
    }
}
