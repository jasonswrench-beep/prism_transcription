//! prism-transcribe-cli: the app's work from the command line, for tests, batch scripts and machines without the
//! window. Nothing here uses the network except `models install`, and that refuses when Offline is on.
//!
//!   prism-transcribe-cli models status|install|import-zip <zip>|export-zip <zip>   (light pack; add "heavy" after the
//!                        command for the heavy pack: models install heavy)   [--dir <model folder>]
//!   prism-transcribe-cli new <folder> <name>
//!   prism-transcribe-cli import <project> <recording> [--label L] [--date D] [--speakers N] [--language xx] [--copy]
//!                        [--pack light-1|heavy-1] [--tracks mixed|tracks|channels]
//!   prism-transcribe-cli transcribe <project> <id>
//!   prism-transcribe-cli export <project> <id> <out.docx|out.txt> [--timestamps turn|none|<seconds>] [--line-numbers] [--clean]
//!                        [--deidentified]   (refused unless no name from the key file's list is left)
//!   prism-transcribe-cli check <project>   the final check (T6): names from the list left in each file
//!   prism-transcribe-cli statement <project> <out.docx|out.txt>   the data handling statement
//!   A password-protected project asks for its password on standard input.
//!   prism-transcribe-cli run <recording> --out <out.docx|out.txt> [--speakers N] [--project <folder>] and the import options

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
            if matches!(k, "copy" | "line-numbers" | "clean" | "quick" | "deidentified") {
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

/// A protected project's password, read from standard input (one line).
fn unlock(dir: &str) {
    let p = Project::open(dir.as_ref()).unwrap_or_else(|e| die(e));
    if p.is_unlocked() {
        return;
    }
    eprint!("Password for {}: ", p.file.name);
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap_or_else(|e| die(e));
    // Only the line ending goes: a password may end in a space.
    p.unlock(line.trim_end_matches(['\r', '\n'])).unwrap_or_else(|e| die(e));
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
        pack: a.opts.get("pack").cloned(),
        tracks: match a.opts.get("tracks").map(String::as_str) {
            None | Some("mixed") => Default::default(),
            Some("tracks") => prism_transcribe_core::project::TrackMode::Tracks,
            Some("channels") => prism_transcribe_core::project::TrackMode::Channels,
            Some(_) => usage(),
        },
    }
}

fn export_opts(a: &Args) -> ExportOptions {
    let timestamps = match a.opts.get("timestamps").map(String::as_str) {
        None | Some("turn") => Timestamps::PerTurn,
        Some("none") => Timestamps::None,
        Some(n) => Timestamps::Every { seconds: n.parse().unwrap_or_else(|_| usage()) },
    };
    ExportOptions { timestamps, line_numbers: a.flags.iter().any(|f| f == "line-numbers"), clean: a.flags.iter().any(|f| f == "clean") }
}

fn main() {
    let a = parse();
    let mut settings = Settings::load();
    if let Some(d) = a.opts.get("dir") {
        settings.model_dir = Some(PathBuf::from(d));
    }
    let model_dir = settings.model_dir();
    match a.pos.first().map(String::as_str) {
        Some("models") => {
            // `models <command> [light|heavy]`; the light pack when none is named.
            let pack = match a.pos.get(2).filter(|x| !x.contains('.')).map(String::as_str) {
                None | Some("light") => &models::LIGHT_PACK,
                Some("heavy") => &models::HEAVY_PACK,
                Some(_) => usage(),
            };
            let arg = |i: usize| a.pos.get(if a.pos.get(2).is_some_and(|x| !x.contains('.')) { i + 1 } else { i }).unwrap_or_else(|| usage()).clone();
            match a.pos.get(1).map(String::as_str) {
                Some("status") => {
                    let s = engine::pack_status(&settings, pack);
                    let hashed = if a.flags.is_empty() { models::verify(pack, &model_dir, true).map_err(|e| e.to_string()) } else { Ok(()) };
                    match (s.installed, hashed) {
                        (true, Ok(())) => println!("{} is installed in {}{}", pack.name, model_dir.display(), s.program.map(|p| format!(" (program: {p})")).unwrap_or_default()),
                        (_, Err(e)) => {
                            println!("{} is not installed in {}: {e}", pack.name, model_dir.display());
                            std::process::exit(1)
                        }
                        (false, _) => {
                            println!("{} is not ready: {}", pack.name, s.message);
                            std::process::exit(1)
                        }
                    }
                }
                Some("install") => {
                    let (p, d) = install::downloads_for(pack.id).unwrap();
                    let mut last = 0u64;
                    install::install(p, d, &model_dir, settings.offline, &mut |p| {
                        if p.pack_done - last > 20_000_000 || p.pack_done == p.pack_total {
                            last = p.pack_done;
                            eprint!("
  {} MB of {} MB ({})            ", p.pack_done / 1_000_000, p.pack_total / 1_000_000, p.file);
                        }
                        true
                    })
                    .unwrap_or_else(|e| die(e));
                    eprintln!();
                    println!("Installed and checked: {}", model_dir.display());
                }
                Some("import-zip") => {
                    let z = arg(2);
                    install::import_zip(pack, z.as_ref(), &model_dir).unwrap_or_else(|e| die(e));
                    println!("Imported and checked: {}", model_dir.display());
                }
                Some("export-zip") => {
                    let z = arg(2);
                    install::export_zip(pack, &model_dir, z.as_ref()).unwrap_or_else(|e| die(e));
                    println!("Wrote {z}");
                }
                _ => usage(),
            }
        }
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
            unlock(proj);
            let t = engine::transcribe(proj.as_ref(), id, &settings, &mut progress_line).unwrap_or_else(|e| die(e));
            println!("{id}: {} turns, {} speakers, {:.1} s", t.turns.len(), t.speakers.len(), t.provenance.seconds);
        }
        Some("export") => {
            let (proj, id, out) = (a.pos.get(1).unwrap_or_else(|| usage()), a.pos.get(2).unwrap_or_else(|| usage()), a.pos.get(3).unwrap_or_else(|| usage()));
            unlock(proj);
            let p = Project::open(proj.as_ref()).unwrap_or_else(|e| die(e));
            let fmt = ExportFormat::from_path(out.as_ref()).unwrap_or_else(|| die("the output must end in .docx or .txt"));
            let deid = a.flags.iter().any(|f| f == "deidentified");
            engine::export_checked(&p, id, fmt, out.as_ref(), &export_opts(&a), deid).unwrap_or_else(|e| die(e));
            println!("Wrote {out}{}", if deid { " (checked: no name from the list)" } else { "" });
        }
        Some("check") => {
            let proj = a.pos.get(1).unwrap_or_else(|| usage());
            unlock(proj);
            let checks = engine::deid::final_check(proj.as_ref()).unwrap_or_else(|e| die(e));
            let mut failing = 0;
            for c in checks.iter().filter(|c| c.transcribed) {
                if c.passes() {
                    println!("{}\tpasses", c.label);
                } else {
                    failing += 1;
                    let left: Vec<String> = c.leftovers.iter().map(|l| format!("{} ({})", l.text, l.place)).collect();
                    println!("{}\t{} left: {}", c.label, left.len(), left.join(", "));
                }
            }
            if failing > 0 {
                std::process::exit(1);
            }
        }
        Some("statement") => {
            let (proj, out) = (a.pos.get(1).unwrap_or_else(|| usage()), a.pos.get(2).unwrap_or_else(|| usage()));
            let blocks = engine::statement::blocks(proj.as_ref(), &settings).unwrap_or_else(|e| die(e));
            let bytes = match ExportFormat::from_path(out.as_ref()) {
                Some(ExportFormat::Word) => prism_transcribe_core::export::blocks_to_docx("Data handling statement", &blocks).unwrap_or_else(|e| die(e)),
                Some(ExportFormat::Text) => prism_transcribe_core::export::blocks_to_text(&blocks).into_bytes(),
                None => die("the output must end in .docx or .txt"),
            };
            std::fs::write(out, bytes).unwrap_or_else(|e| die(e));
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
