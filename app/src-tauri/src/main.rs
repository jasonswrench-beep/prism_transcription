// Prism Transcribe's desktop shell: projects, import, transcription with progress, the editor (phase 2), speakers
// (phase 3), de-identification, password protection and the data handling statement (phase 4), export, and the
// model manager.
//
// Long work runs on its own thread and reports through events ("job", "install", "import"); the window never
// waits on it. Nothing here touches the network except the model install, which the Offline switch blocks.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod media_protocol;
mod updates;

use prism_transcribe_core::deid::{FileCheck, Flag, LogEntry, NameEntry, NameKind, Suggestion};
use prism_transcribe_core::edit::{self, FindOptions};
use prism_transcribe_core::speakers;
use prism_transcribe_core::export::ExportOptions;
use prism_transcribe_core::model::{Speaker, Transcript, Turn};
use prism_transcribe_core::project::{ImportFields, MediaCheck, MediaFile, Project};
use prism_transcribe_core::queue::{Queue, QueueState};
use prism_transcribe_engine::{self as engine, ExportFormat, Progress, Settings};
use prism_transcribe_speech::{install, models};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

#[derive(Default)]
struct AppState {
    project: Mutex<Option<PathBuf>>,
    /// The running transcription or install, if any, and its cancel flag.
    busy: Mutex<Option<(String, Arc<AtomicBool>)>>,
    /// Whether the batch queue's thread is running.
    runner: AtomicBool,
}

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
struct ProjectView {
    dir: String,
    name: String,
    files: Vec<MediaFile>,
    /// Recordings that changed or went missing since import (T1).
    warnings: Vec<(String, MediaCheck)>,
    ffmpeg: Option<String>,
    /// Password protection: on, and whether this session has unlocked it.
    protected: bool,
    locked: bool,
    word_list: Vec<String>,
}

fn view(dir: &std::path::Path, check: bool) -> Res<ProjectView> {
    let p = Project::open(dir).map_err(err)?;
    let warnings = if check { p.check_media().into_iter().filter(|(_, c)| *c != MediaCheck::Unchanged).collect() } else { Vec::new() };
    Ok(ProjectView {
        dir: p.dir.display().to_string(),
        name: p.file.name.clone(),
        files: p.file.files.clone(),
        warnings,
        ffmpeg: Settings::load().ffmpeg().map(|f| f.display().to_string()),
        protected: p.is_protected(),
        locked: !p.is_unlocked(),
        word_list: p.file.word_list.clone(),
    })
}

fn current(state: &State<AppState>) -> Res<PathBuf> {
    state.project.lock().unwrap().clone().ok_or_else(|| "No project is open.".to_string())
}

#[tauri::command]
fn project_new(state: State<AppState>, dir: String, name: String) -> Res<ProjectView> {
    let dir = PathBuf::from(dir);
    Project::create(&dir, &name).map_err(err)?;
    *state.project.lock().unwrap() = Some(dir.clone());
    view(&dir, false)
}

#[tauri::command]
fn project_open(state: State<AppState>, path: String) -> Res<ProjectView> {
    let p = Project::open(PathBuf::from(&path).as_path()).map_err(err)?;
    *state.project.lock().unwrap() = Some(p.dir.clone());
    // A queue still marked running was stopped by a crash or by closing the app: it waits for Resume (T8).
    if !state.runner.load(Ordering::SeqCst) && p.file.queue.state == QueueState::Running {
        Project::update(&p.dir, |q| {
            q.file.queue.recover();
            q.file.queue.rev += 1;
            Ok(())
        })
        .map_err(err)?;
    }
    view(&p.dir, true)
}

#[tauri::command]
fn project_view(state: State<AppState>) -> Res<ProjectView> {
    view(&current(&state)?, false)
}

#[tauri::command]
fn project_clean_up(state: State<AppState>) -> Res<usize> {
    if state.busy.lock().unwrap().is_some() {
        return Err("Wait until the running job ends.".into());
    }
    Project::open(&current(&state)?).map_err(err)?.clean_up().map_err(err)
}

#[derive(Clone, Serialize)]
struct ImportEvent {
    path: String,
    done: bool,
    id: Option<String>,
    error: Option<String>,
}

#[tauri::command]
fn media_import(app: tauri::AppHandle, state: State<AppState>, paths: Vec<String>, fields: ImportFields, copy: bool) -> Res<()> {
    let dir = current(&state)?;
    let ffmpeg = Settings::load().ffmpeg();
    let many = paths.len() > 1;
    std::thread::spawn(move || {
        for path in paths {
            let mut f = fields.clone();
            if many {
                f.label = String::new(); // one label for many files would collide; each takes its file name
            }
            let r = engine::import(&dir, path.as_ref(), f, copy, ffmpeg.as_deref(), &mut |_| true);
            let (id, error) = match r {
                Ok(id) => (Some(id), None),
                Err(e) => (None, Some(e.to_string())),
            };
            let _ = app.emit("import", ImportEvent { path: path.clone(), done: true, id, error });
        }
    });
    Ok(())
}

#[tauri::command]
fn media_update(state: State<AppState>, id: String, fields: ImportFields) -> Res<ProjectView> {
    let dir = current(&state)?;
    Project::update(&dir, |p| {
        p.media_mut(&id)?.fields = fields;
        Ok(())
    })
    .map_err(err)?;
    view(&dir, false)
}

/// Take a recording with no transcript out of the project (a failed import). The recording itself is not touched.
#[tauri::command]
fn media_remove(state: State<AppState>, id: String) -> Res<ProjectView> {
    let dir = current(&state)?;
    if state.busy.lock().unwrap().as_ref().is_some_and(|(running, _)| *running == id) {
        return Err("This recording is being transcribed; cancel that first.".into());
    }
    Project::remove_media(&dir, &id).map_err(err)?;
    view(&dir, false)
}

#[derive(Clone, Serialize)]
struct JobEvent {
    id: String,
    progress: Option<Progress>,
    finished: bool,
    cancelled: bool,
    error: Option<String>,
}

#[tauri::command]
fn transcribe(app: tauri::AppHandle, state: State<AppState>, id: String) -> Res<()> {
    let dir = current(&state)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut busy = state.busy.lock().unwrap();
        if busy.is_some() {
            return Err("Another job is running. Prism Transcribe works on one file at a time.".into());
        }
        *busy = Some((id.clone(), cancel.clone()));
    }
    let settings = Settings::load();
    std::thread::spawn(move || {
        let emit = |e: JobEvent| {
            let _ = app.emit("job", e);
        };
        let r = engine::transcribe(&dir, &id, &settings, &mut |p| {
            emit(JobEvent { id: id.clone(), progress: Some(p), finished: false, cancelled: false, error: None });
            !cancel.load(Ordering::SeqCst)
        });
        let cancelled = matches!(r, Err(engine::EngineError::Speech(prism_transcribe_speech::SpeechError::Cancelled)));
        emit(JobEvent { id: id.clone(), progress: None, finished: true, cancelled, error: r.err().filter(|_| !cancelled).map(|e| e.to_string()) });
        if let Some(s) = app.try_state::<AppState>() {
            *s.busy.lock().unwrap() = None;
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel(state: State<AppState>) -> Res<()> {
    if let Some((_, flag)) = state.busy.lock().unwrap().as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
fn transcript(state: State<AppState>, id: String) -> Res<Option<Transcript>> {
    Project::open(&current(&state)?).map_err(err)?.load_transcript(&id).map_err(err)
}

/// Export one transcript, or several into one table or package (T7). The format comes from the file ending. With
/// `deidentified`, the export is refused unless every file passes the final check (T6).
#[tauri::command]
async fn export(state: State<'_, AppState>, ids: Vec<String>, path: String, options: ExportOptions, deidentified: bool) -> Res<()> {
    let dir = current(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let out = PathBuf::from(&path);
        let fmt = ExportFormat::from_path(&out).ok_or("Choose a file ending in .docx, .txt, .srt, .vtt, .csv, .xlsx, .prismtx or .qdpx.")?;
        let p = Project::open(&dir).map_err(err)?;
        engine::export_files(&p, &ids, fmt, &out, &options, deidentified).map_err(err)
    })
    .await
    .map_err(err)?
}

/// What the export dialog shows before exporting: each file's review status and, when there is a key file, its
/// final check.
#[derive(Serialize)]
struct ExportCheck {
    key: engine::deid::KeyState,
    files: Vec<FileExportCheck>,
}

#[derive(Serialize)]
struct FileExportCheck {
    id: String,
    label: String,
    deidentified: bool,
    check: Option<FileCheck>,
}

#[tauri::command]
fn export_check(state: State<AppState>, ids: Vec<String>) -> Res<ExportCheck> {
    let dir = current(&state)?;
    let key = engine::deid::key_state(&dir).map_err(err)?;
    let ready = matches!(key, engine::deid::KeyState::Ready { .. });
    let p = Project::open(&dir).map_err(err)?;
    let mut files = Vec::new();
    for id in ids {
        let check = if ready { Some(engine::deid::check_one(&dir, &id).map_err(err)?) } else { None };
        let review = p.load_transcript(&id).map_err(err)?.map(|t| t.review).unwrap_or_default();
        let label = p.media(&id).map(|m| m.fields.label.clone()).unwrap_or_else(|_| id.clone());
        files.push(FileExportCheck { id, label, deidentified: prism_transcribe_core::deid::is_deidentified(review), check });
    }
    Ok(ExportCheck { key, files })
}

/// Whether PrismQDA and Prism Hub are installed for this user (T7: "Open in PrismQDA", or get it through Hub).
#[tauri::command]
fn suite_apps() -> engine::suite::SuiteApps {
    engine::suite::find()
}

#[tauri::command]
fn suite_open(app: String) -> Res<()> {
    let found = engine::suite::find();
    let program = match app.as_str() {
        "prismqda" => found.prismqda.ok_or("PrismQDA is not installed for this user.")?,
        "hub" => found.hub.ok_or("Prism Hub is not installed for this user.")?,
        _ => return Err("unknown program".into()),
    };
    engine::suite::launch(&program).map_err(err)
}

/// The light pack's state at the top level (what the first-run panel needs), and the heavy pack's beside it.
#[derive(Serialize)]
struct ModelStatus {
    installed: bool,
    message: String,
    dir: String,
    pack: String,
    download_mb: u64,
    heavy: engine::PackStatus,
}

#[tauri::command]
fn models_status() -> ModelStatus {
    let settings = Settings::load();
    let light = engine::pack_status(&settings, &models::LIGHT_PACK);
    ModelStatus {
        installed: light.installed,
        message: light.message,
        dir: settings.model_dir().display().to_string(),
        pack: light.name,
        download_mb: light.download_mb,
        heavy: engine::pack_status(&settings, &models::HEAVY_PACK),
    }
}

#[derive(Clone, Serialize)]
struct InstallEvent {
    progress: Option<install::InstallProgress>,
    finished: bool,
    error: Option<String>,
}

#[tauri::command]
fn models_install(app: tauri::AppHandle, state: State<AppState>, pack: Option<String>) -> Res<()> {
    let settings = Settings::load();
    let (pack, downloads) = install::downloads_for(pack.as_deref().unwrap_or("light-1")).ok_or("There is no such model pack.")?;
    if settings.offline {
        return Err(install::InstallError::Offline.to_string());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut busy = state.busy.lock().unwrap();
        if busy.is_some() {
            return Err("Another job is running.".into());
        }
        *busy = Some(("models".into(), cancel.clone()));
    }
    std::thread::spawn(move || {
        let mut last = 0u64;
        let r = install::install(pack, downloads, &settings.model_dir(), settings.offline, &mut |p| {
            if p.pack_done - last >= 2_000_000 || p.pack_done == p.pack_total {
                last = p.pack_done;
                let _ = app.emit("install", InstallEvent { progress: Some(p), finished: false, error: None });
            }
            !cancel.load(Ordering::SeqCst)
        });
        let _ = app.emit("install", InstallEvent { progress: None, finished: true, error: r.err().map(|e| e.to_string()) });
        if let Some(s) = app.try_state::<AppState>() {
            *s.busy.lock().unwrap() = None;
        }
    });
    Ok(())
}

#[tauri::command]
fn models_import_zip(path: String, pack: Option<String>) -> Res<ModelStatus> {
    let (pack, _) = install::downloads_for(pack.as_deref().unwrap_or("light-1")).ok_or("There is no such model pack.")?;
    install::import_zip(pack, path.as_ref(), &Settings::load().model_dir()).map_err(err)?;
    Ok(models_status())
}

#[tauri::command]
fn settings_get() -> Settings {
    Settings::load()
}

#[tauri::command]
fn settings_set(mut settings: Settings) -> Res<Settings> {
    // Where the key files are is set only by De-identify; the Settings window never overwrites it.
    settings.key_files = Settings::load().key_files;
    settings.save().map_err(err)?;
    Ok(Settings::load())
}

// ---------------------------------------------------------------------------------------------------------
// The editor (phase 2)
// ---------------------------------------------------------------------------------------------------------

#[derive(Serialize)]
struct EditorInfo {
    /// What the audio or video element plays: the original, or a WAV copy when WebView2 cannot play the original.
    url: String,
    playback_copy: bool,
    has_video: bool,
    duration: f64,
    peaks_per_second: u32,
    notation: Vec<String>,
    hotkeys: std::collections::BTreeMap<String, String>,
    rewind_on_resume: f64,
    /// Where the recording should be, when it is not there: the transcript opens, and the editor says so.
    missing: Option<String>,
}

#[tauri::command]
fn editor_info(state: State<AppState>, id: String) -> Res<EditorInfo> {
    let dir = current(&state)?;
    let settings = Settings::load();
    let (path, copy) = engine::playback_file(&dir, &id, &settings).map_err(err)?;
    let p = Project::open(&dir).map_err(err)?;
    let m = p.media(&id).map_err(err)?;
    let original = p.media_path(m);
    let missing = (!original.is_file()).then(|| original.display().to_string().trim_start_matches(r"\\?\").to_string());
    // A version number in the address, so a different project's F001 is never served from the player's cache.
    let stamp = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    Ok(EditorInfo {
        url: format!("http://media.localhost/{id}?v={stamp}"),
        playback_copy: copy,
        has_video: m.has_video && !copy,
        duration: m.duration,
        peaks_per_second: engine::peaks_per_second(),
        notation: p.file.notation.clone(),
        hotkeys: settings.effective_hotkeys(),
        rewind_on_resume: settings.rewind(),
        missing,
    })
}

#[tauri::command]
async fn peaks(state: State<'_, AppState>, id: String) -> Res<Vec<i8>> {
    let dir = current(&state)?;
    tauri::async_runtime::spawn_blocking(move || engine::peaks(&dir, &id, &Settings::load()).map_err(err)).await.map_err(err)?
}

/// A turn whose text was edited: new words, with times kept by T5's rule. `lo` and `hi` bound typed words at the
/// turn's edges (the previous turn's end and the next turn's start).
#[tauri::command]
fn retime_turn(turn: Turn, text: String, lo: f64, hi: f64) -> Turn {
    let words = edit::retime(&turn.words, &text, &turn.speaker, lo, hi);
    let (start, end) = match (words.first(), words.last()) {
        (Some(f), Some(l)) => (f.start, l.end),
        _ => (turn.start, turn.start),
    };
    Turn { words, start, end, ..turn }
}

/// For each turn, which words the clean view shows.
#[tauri::command]
fn clean_masks(turns: Vec<Turn>) -> Vec<Vec<bool>> {
    turns.iter().map(|t| edit::clean_mask(&t.words)).collect()
}

#[tauri::command]
fn transcript_save(state: State<AppState>, transcript: Transcript) -> Res<()> {
    engine::save_transcript(&current(&state)?, transcript).map_err(err)
}

#[tauri::command]
fn replace_in_project(state: State<AppState>, find: String, replace: String, options: FindOptions) -> Res<Vec<(String, usize)>> {
    engine::replace_in_project(&current(&state)?, &find, &replace, &options).map_err(err)
}

#[tauri::command]
fn notation_set(state: State<AppState>, list: Vec<String>) -> Res<Vec<String>> {
    let list = engine::notation_list(&list);
    Project::update(&current(&state)?, |p| {
        p.file.notation = list.clone();
        Ok(())
    })
    .map_err(err)?;
    Ok(list)
}

// ---- Speakers (T3) ----

/// One speaker operation on the editor's current speakers and turns.
#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum SpeakerOp {
    /// Split a turn before a word.
    Split { turn: usize, word: usize },
    /// Give a turn, or its words `from..to`, to a speaker.
    Reassign { turn: usize, from: Option<usize>, to: Option<usize>, speaker: String },
    /// Join a turn onto the one before it.
    JoinPrevious { turn: usize },
    /// Merge one speaker into another.
    Merge { from: String, into: String },
    Add,
    /// Remove a speaker who has no turns.
    Remove { id: String },
}

#[derive(Serialize)]
struct SpeakerEdit {
    speakers: Vec<Speaker>,
    turns: Vec<Turn>,
    /// The turn the cursor belongs in afterwards, and whether anything changed.
    turn: Option<usize>,
    changed: bool,
}

#[tauri::command]
fn speakers_edit(op: SpeakerOp, mut speakers: Vec<Speaker>, mut turns: Vec<Turn>) -> SpeakerEdit {
    let (turn, changed) = match op {
        SpeakerOp::Split { turn, word } => {
            let ok = speakers::split_turn(&mut turns, turn, word);
            (Some(if ok { turn + 1 } else { turn }), ok)
        }
        SpeakerOp::Reassign { turn, from, to, speaker } => {
            let range = match (from, to) {
                (Some(a), Some(b)) => Some(a..b),
                _ => None,
            };
            let before = turns.clone();
            let at = speakers::reassign(&mut turns, turn, range, &speaker);
            (at, turns != before)
        }
        SpeakerOp::JoinPrevious { turn } => {
            let ok = speakers::join_with_previous(&mut turns, turn);
            (Some(turn.saturating_sub(if ok { 1 } else { 0 })), ok)
        }
        SpeakerOp::Merge { from, into } => (None, speakers::merge_speakers(&mut speakers, &mut turns, &from, &into)),
        SpeakerOp::Add => {
            speakers::add_speaker(&mut speakers);
            (None, true)
        }
        SpeakerOp::Remove { id } => (None, speakers::remove_speaker(&mut speakers, &turns, &id)),
    };
    SpeakerEdit { speakers, turns, turn, changed }
}

#[tauri::command]
fn speakers_stats(speakers: Vec<Speaker>, turns: Vec<Turn>) -> Vec<speakers::SpeakerStats> {
    speakers::stats(&speakers, &turns)
}

/// Find the speakers again with an expected count, keeping the words as they are (T3). Takes a minute or two for an
/// hour of recording, so it runs off the window's thread.
#[tauri::command]
async fn speakers_find_again(state: State<'_, AppState>, id: String, turns: Vec<Turn>, expected: Option<u32>) -> Res<SpeakerEdit> {
    let dir = current(&state)?;
    let (speakers, turns) = tauri::async_runtime::spawn_blocking(move || engine::find_speakers_again(&dir, &id, &turns, expected, &Settings::load()).map_err(err)).await.map_err(err)??;
    Ok(SpeakerEdit { speakers, turns, turn: None, changed: true })
}

#[tauri::command]
fn hotkeys_set(hotkeys: std::collections::BTreeMap<String, String>, rewind_on_resume: f64) -> Res<std::collections::BTreeMap<String, String>> {
    let mut s = Settings::load();
    s.hotkeys = hotkeys;
    s.rewind_on_resume = Some(rewind_on_resume);
    s.save().map_err(err)?;
    Ok(Settings::load().effective_hotkeys())
}

#[tauri::command]
fn hotkeys_default() -> std::collections::BTreeMap<String, String> {
    engine::default_hotkeys()
}

// ---- The batch queue (T8) ----

#[derive(Clone, Serialize)]
struct QueueSummary {
    done: Vec<String>,
    /// (file label, why)
    failed: Vec<(String, String)>,
    seconds: f64,
}

#[derive(Clone, Serialize)]
struct QueueEvent {
    queue: Queue,
    /// Set once, when the queue has run to its end.
    summary: Option<QueueSummary>,
}

fn emit_queue(app: &tauri::AppHandle, dir: &std::path::Path, summary: Option<QueueSummary>) {
    if let Ok(p) = Project::open(dir) {
        let _ = app.emit("queue", QueueEvent { queue: p.file.queue, summary });
    }
}

/// Change the queue and say so; start its thread when it should run.
fn queue_change(app: &tauri::AppHandle, state: &State<AppState>, f: impl FnOnce(&mut Queue)) -> Res<Queue> {
    let dir = current(state)?;
    let q = Project::update(&dir, |p| {
        f(&mut p.file.queue);
        p.file.queue.rev += 1;
        Ok(p.file.queue.clone())
    })
    .map_err(err)?;
    if q.state == QueueState::Running && q.has_waiting() {
        run_queue(app.clone(), dir.clone());
    }
    emit_queue(app, &dir, None);
    Ok(q)
}

/// Stop the running transcription, if it is `id` (or any, with `None`); its checkpoint is kept.
fn stop_job(state: &State<AppState>, id: Option<&str>) {
    if let Some((running, flag)) = state.busy.lock().unwrap().as_ref() {
        if id.is_none_or(|i| i == running) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

/// The queue's thread: one file at a time, keeping the computer awake, until nothing waits or the queue is paused
/// or cancelled. At the end, a Windows notification and an in-app summary naming any file that failed and why.
fn run_queue(app: tauri::AppHandle, dir: PathBuf) {
    if app.state::<AppState>().runner.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let _awake = engine::power::KeepAwake::new();
        let started = std::time::Instant::now();
        let (mut done, mut failed) = (Vec::new(), Vec::new());
        let label = |id: &str| Project::open(&dir).ok().and_then(|p| p.media(id).ok().map(|m| m.fields.label.clone())).unwrap_or_else(|| id.to_string());
        loop {
            let st = app.state::<AppState>();
            // Claim the job slot and the next file together, so a model install never runs beside a transcription.
            let cancel = Arc::new(AtomicBool::new(false));
            let id = {
                let mut busy = st.busy.lock().unwrap();
                if busy.is_some() {
                    break;
                }
                match Project::update(&dir, |p| {
                    let next = p.file.queue.take_next();
                    p.file.queue.rev += 1;
                    Ok(next)
                }) {
                    Ok(Some(id)) => {
                        *busy = Some((id.clone(), cancel.clone()));
                        id
                    }
                    _ => break,
                }
            };
            emit_queue(&app, &dir, None);
            let emit = |e: JobEvent| {
                let _ = app.emit("job", e);
            };
            let r = engine::transcribe(&dir, &id, &Settings::load(), &mut |p| {
                emit(JobEvent { id: id.clone(), progress: Some(p), finished: false, cancelled: false, error: None });
                !cancel.load(Ordering::SeqCst)
            });
            let cancelled = matches!(r, Err(engine::EngineError::Speech(prism_transcribe_speech::SpeechError::Cancelled)));
            let error = r.as_ref().err().filter(|_| !cancelled).map(|e| e.to_string());
            *st.busy.lock().unwrap() = None;
            let _ = Project::update(&dir, |p| {
                p.file.queue.finish(&id, error.clone().map_or(Ok(()), Err), cancelled);
                p.file.queue.rev += 1;
                Ok(())
            });
            match &error {
                None if !cancelled => done.push(label(&id)),
                Some(e) => failed.push((label(&id), e.clone())),
                None => {}
            }
            emit(JobEvent { id: id.clone(), progress: None, finished: true, cancelled, error });
        }
        app.state::<AppState>().runner.store(false, Ordering::SeqCst);
        // Run to its end: the queue goes idle and says what happened. Paused or cancelled: it just stops.
        let q = Project::update(&dir, |p| {
            let q = &mut p.file.queue;
            if q.state == QueueState::Running && !q.has_waiting() {
                q.state = QueueState::Idle;
            }
            q.rev += 1;
            Ok(q.clone())
        });
        let finished = matches!(&q, Ok(q) if q.state == QueueState::Idle) && (!done.is_empty() || !failed.is_empty());
        let summary = finished.then(|| QueueSummary { done: done.clone(), failed: failed.clone(), seconds: started.elapsed().as_secs_f64() });
        if let Some(s) = &summary {
            use tauri_plugin_notification::NotificationExt;
            let body = if s.failed.is_empty() {
                format!("{} file{} transcribed.", s.done.len(), if s.done.len() == 1 { "" } else { "s" })
            } else {
                format!("{} transcribed, {} failed: {}.", s.done.len(), s.failed.len(), s.failed.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>().join(", "))
            };
            let _ = app.notification().builder().title("Prism Transcribe: the queue has finished").body(body).show();
        }
        emit_queue(&app, &dir, summary);
    });
}

#[tauri::command]
fn queue_get(state: State<AppState>) -> Res<Queue> {
    Ok(Project::open(&current(&state)?).map_err(err)?.file.queue)
}

/// Add files to the end of the queue; with `start`, run it.
#[tauri::command]
fn queue_add(app: tauri::AppHandle, state: State<AppState>, ids: Vec<String>, start: bool) -> Res<Queue> {
    queue_change(&app, &state, |q| {
        q.add(&ids);
        if start {
            q.state = QueueState::Running;
            q.interrupted = false;
        }
    })
}

/// Run, or resume after Pause or a restart (each file carries on from its last finished chunk).
#[tauri::command]
fn queue_start(app: tauri::AppHandle, state: State<AppState>) -> Res<Queue> {
    queue_change(&app, &state, |q| {
        q.state = QueueState::Running;
        q.interrupted = false;
    })
}

#[tauri::command]
fn queue_pause(app: tauri::AppHandle, state: State<AppState>) -> Res<Queue> {
    let q = queue_change(&app, &state, |q| q.state = QueueState::Paused)?;
    stop_job(&state, q.running().map(|i| i.id.as_str()));
    Ok(q)
}

/// Cancel the whole queue: the running file stops (its checkpoint is kept) and every unfinished file leaves it.
#[tauri::command]
fn queue_cancel(app: tauri::AppHandle, state: State<AppState>) -> Res<Queue> {
    let running = queue_get(state.clone())?.running().map(|i| i.id.clone());
    if let Some(id) = &running {
        stop_job(&state, Some(id));
    }
    queue_change(&app, &state, |q| q.cancel_all())
}

/// Hold one file (skip it, keeping its place) or release it. Holding the running file stops it.
#[tauri::command]
fn queue_hold(app: tauri::AppHandle, state: State<AppState>, id: String, hold: bool) -> Res<Queue> {
    let was_running = queue_get(state.clone())?.running().is_some_and(|i| i.id == id);
    let q = queue_change(&app, &state, |q| {
        q.hold(&id, hold);
    })?;
    if hold && was_running {
        stop_job(&state, Some(&id));
    }
    Ok(q)
}

#[tauri::command]
fn queue_remove(app: tauri::AppHandle, state: State<AppState>, id: String) -> Res<Queue> {
    let was_running = queue_get(state.clone())?.running().is_some_and(|i| i.id == id);
    if was_running {
        stop_job(&state, Some(&id));
    }
    queue_change(&app, &state, |q| {
        q.remove(&id);
    })
}

/// Move a file to a new place in the queue (drag and drop, or the move buttons).
#[tauri::command]
fn queue_move(app: tauri::AppHandle, state: State<AppState>, id: String, to: usize) -> Res<Queue> {
    queue_change(&app, &state, |q| {
        q.move_to(&id, to);
    })
}

#[tauri::command]
fn queue_clear(app: tauri::AppHandle, state: State<AppState>) -> Res<Queue> {
    queue_change(&app, &state, |q| q.clear_finished())
}

// ---- Check for updates (docs/UPDATES.md): a button, never a timer ----

#[tauri::command]
fn update_state(app: tauri::AppHandle) -> serde_json::Value {
    updates::state(&app, &updates::Feed::built_in(), Settings::load().offline)
}

#[tauri::command]
async fn update_check(app: tauri::AppHandle) -> Res<serde_json::Value> {
    updates::check(&app, &updates::Feed::built_in(), Settings::load().offline).await
}

/// Download, check the signature and run the installer, which closes the program. Refused while a job runs.
#[tauri::command]
async fn update_install(app: tauri::AppHandle, state: State<'_, AppState>) -> Res<serde_json::Value> {
    let busy = state.busy.lock().unwrap().is_some() || state.runner.load(Ordering::SeqCst);
    updates::install(&app, &updates::Feed::built_in(), Settings::load().offline, busy).await
}

// ---- De-identification (T6) ----

use engine::deid as di;

#[tauri::command]
fn deid_key_state(state: State<AppState>) -> Res<di::KeyState> {
    di::key_state(&current(&state)?).map_err(err)
}

/// Open or start the key file at `path`.
#[tauri::command]
fn deid_key_use(state: State<AppState>, path: String) -> Res<di::KeyState> {
    di::key_use(&current(&state)?, PathBuf::from(path).as_path()).map_err(err)
}

#[tauri::command]
fn deid_names(state: State<AppState>) -> Res<Vec<NameEntry>> {
    Ok(di::key_open(&current(&state)?).map_err(err)?.2.names)
}

#[tauri::command]
fn deid_names_set(state: State<AppState>, names: Vec<NameEntry>) -> Res<Vec<NameEntry>> {
    di::names_set(&current(&state)?, names).map_err(err)
}

#[tauri::command]
fn deid_names_paste(state: State<AppState>, text: String) -> Res<Vec<NameEntry>> {
    di::names_paste(&current(&state)?, &text).map_err(err)
}

#[tauri::command]
fn deid_auto_variants(real: String, kind: NameKind) -> Vec<String> {
    prism_transcribe_core::deid::auto_variants(&real, kind)
}

#[tauri::command]
async fn deid_names_preview(state: State<'_, AppState>) -> Res<Vec<di::FileCount>> {
    let dir = current(&state)?;
    tauri::async_runtime::spawn_blocking(move || di::names_preview(&dir).map_err(err)).await.map_err(err)?
}

#[tauri::command]
async fn deid_names_apply(state: State<'_, AppState>) -> Res<Vec<di::FileCount>> {
    let dir = current(&state)?;
    tauri::async_runtime::spawn_blocking(move || di::names_apply(&dir).map_err(err)).await.map_err(err)?
}

#[tauri::command]
async fn deid_flags(state: State<'_, AppState>) -> Res<Vec<Flag>> {
    let dir = current(&state)?;
    tauri::async_runtime::spawn_blocking(move || di::flags_find(&dir).map_err(err)).await.map_err(err)?
}

#[tauri::command]
fn deid_flag_apply(state: State<AppState>, flag: Flag, replacement: String) -> Res<bool> {
    di::flag_apply(&current(&state)?, &flag, &replacement).map_err(err)
}

#[tauri::command]
fn deid_flag_skip(state: State<AppState>, text: String) -> Res<()> {
    di::flag_skip(&current(&state)?, &text).map_err(err)
}

#[derive(Serialize)]
struct Suggestions {
    list: Vec<Suggestion>,
    /// The dictionary used ("en-US"), or none when Windows has none for the language.
    dictionary: Option<String>,
}

#[tauri::command]
async fn deid_suggestions(state: State<'_, AppState>) -> Res<Suggestions> {
    let dir = current(&state)?;
    let (list, dictionary) = tauri::async_runtime::spawn_blocking(move || di::suggestions(&dir).map_err(err)).await.map_err(err)??;
    Ok(Suggestions { list, dictionary })
}

#[tauri::command]
async fn deid_suggestion_accept(state: State<'_, AppState>, word: String, replacement: String, kind: NameKind) -> Res<Vec<di::FileCount>> {
    let dir = current(&state)?;
    tauri::async_runtime::spawn_blocking(move || di::suggestion_accept(&dir, &word, &replacement, kind).map_err(err)).await.map_err(err)?
}

#[tauri::command]
fn deid_suggestion_skip(state: State<AppState>, word: String) -> Res<Vec<String>> {
    di::suggestion_skip(&current(&state)?, &word).map_err(err)
}

#[tauri::command]
fn word_list_set(state: State<AppState>, words: Vec<String>) -> Res<Vec<String>> {
    di::word_list_set(&current(&state)?, words).map_err(err)
}

#[tauri::command]
async fn deid_check(state: State<'_, AppState>) -> Res<Vec<FileCheck>> {
    let dir = current(&state)?;
    tauri::async_runtime::spawn_blocking(move || di::final_check(&dir).map_err(err)).await.map_err(err)?
}

#[tauri::command]
fn deid_mark(state: State<AppState>, id: String) -> Res<FileCheck> {
    di::mark_deidentified(&current(&state)?, &id).map_err(err)
}

#[tauri::command]
fn deid_log(state: State<AppState>) -> Res<Vec<LogEntry>> {
    di::log(&current(&state)?).map_err(err)
}

// ---- Privacy: password protection and the data handling statement ----

/// Turn password protection on, change the password, or turn it off (`None`). Waits for no running job.
#[tauri::command]
async fn project_password_set(state: State<'_, AppState>, password: Option<String>) -> Res<ProjectView> {
    if state.busy.lock().unwrap().is_some() {
        return Err("Wait until the running job ends.".into());
    }
    let dir = current(&state)?;
    let d = dir.clone();
    tauri::async_runtime::spawn_blocking(move || -> Res<()> {
        // The key file is sealed with the project's password too: read it with the old key, write it with the new.
        let key = di::key_open(&d).ok();
        Project::set_password(&d, password.as_deref().filter(|p| !p.is_empty())).map_err(err)?;
        if let Some((_, path, k)) = key {
            k.save(&path, &Project::open(&d).map_err(err)?).map_err(err)?;
        }
        Ok(())
    })
    .await
    .map_err(err)??;
    view(&dir, false)
}

#[tauri::command]
async fn project_unlock(state: State<'_, AppState>, password: String) -> Res<ProjectView> {
    let dir = current(&state)?;
    let d = dir.clone();
    tauri::async_runtime::spawn_blocking(move || Project::open(&d).and_then(|p| p.unlock(&password)).map_err(err)).await.map_err(err)??;
    view(&dir, false)
}

/// Lock a protected project now: its key leaves memory.
#[tauri::command]
fn project_lock(state: State<AppState>) -> Res<ProjectView> {
    if state.busy.lock().unwrap().is_some() {
        return Err("Wait until the running job ends.".into());
    }
    let dir = current(&state)?;
    prism_transcribe_core::crypt::forget(&dir);
    view(&dir, false)
}

/// Save the data handling statement as .docx or .txt.
#[tauri::command]
fn statement_export(state: State<AppState>, path: String) -> Res<()> {
    let dir = current(&state)?;
    let blocks = engine::statement::blocks(&dir, &Settings::load()).map_err(err)?;
    let out = PathBuf::from(&path);
    let bytes = match ExportFormat::from_path(&out) {
        Some(ExportFormat::Word) => prism_transcribe_core::export::blocks_to_docx("Data handling statement", &blocks).map_err(err)?,
        Some(ExportFormat::Text) => prism_transcribe_core::export::blocks_to_text(&blocks).into_bytes(),
        _ => return Err("Choose a file ending in .docx or .txt.".into()),
    };
    std::fs::write(&out, bytes).map_err(err)
}

/// The statement as text, for the window to show before saving.
#[tauri::command]
fn statement_text(state: State<AppState>) -> Res<String> {
    let blocks = engine::statement::blocks(&current(&state)?, &Settings::load()).map_err(err)?;
    Ok(prism_transcribe_core::export::blocks_to_text(&blocks))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::default())
        .register_uri_scheme_protocol("media", |ctx, request| {
            let reply = (|| {
                let state = ctx.app_handle().state::<AppState>();
                let dir = state.project.lock().unwrap().clone()?;
                let id = request.uri().path().trim_start_matches('/').to_string();
                let (path, _) = engine::playback_file(&dir, &id, &Settings::load()).ok()?;
                let range = request.headers().get("range").and_then(|v| v.to_str().ok());
                Some(media_protocol::serve(&path, range))
            })()
            .unwrap_or(media_protocol::Reply { status: 404, headers: vec![("Access-Control-Allow-Origin", "*".into())], body: Vec::new() });
            let mut b = tauri::http::Response::builder().status(reply.status);
            for (k, v) in reply.headers {
                b = b.header(k, v);
            }
            b.body(reply.body).unwrap()
        })
        .invoke_handler(tauri::generate_handler![
            project_new,
            project_open,
            project_view,
            project_clean_up,
            media_import,
            media_update,
            media_remove,
            transcribe,
            cancel,
            transcript,
            export,
            models_status,
            models_install,
            models_import_zip,
            settings_get,
            settings_set,
            editor_info,
            peaks,
            retime_turn,
            clean_masks,
            transcript_save,
            replace_in_project,
            notation_set,
            hotkeys_set,
            hotkeys_default,
            speakers_edit,
            speakers_stats,
            speakers_find_again,
            export_check,
            deid_key_state,
            deid_key_use,
            deid_names,
            deid_names_set,
            deid_names_paste,
            deid_auto_variants,
            deid_names_preview,
            deid_names_apply,
            deid_flags,
            deid_flag_apply,
            deid_flag_skip,
            deid_suggestions,
            deid_suggestion_accept,
            deid_suggestion_skip,
            word_list_set,
            deid_check,
            deid_mark,
            deid_log,
            project_password_set,
            project_unlock,
            project_lock,
            statement_export,
            statement_text,
            queue_get,
            queue_add,
            queue_start,
            queue_pause,
            queue_cancel,
            queue_hold,
            queue_remove,
            queue_move,
            queue_clear,
            suite_apps,
            suite_open,
            update_state,
            update_check,
            update_install
        ])
        .run(tauri::generate_context!())
        .expect("Prism Transcribe could not start");
}
