// Prism Transcribe's desktop shell. Phase 1: create or open a project, import recordings, transcribe one at a
// time with progress, read the transcript, export it to Word or plain text, and install the speech models.
//
// Long work runs on its own thread and reports through events ("job", "install", "import"); the window never
// waits on it. Nothing here touches the network except the model install, which the Offline switch blocks.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod media_protocol;

use prism_transcribe_core::edit::{self, FindOptions};
use prism_transcribe_core::export::ExportOptions;
use prism_transcribe_core::model::{Transcript, Turn};
use prism_transcribe_core::project::{ImportFields, MediaCheck, MediaFile, Project};
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
            return Err("Another job is running. Phase 1 transcribes one file at a time.".into());
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

#[tauri::command]
fn export(state: State<AppState>, id: String, path: String, options: ExportOptions) -> Res<()> {
    let out = PathBuf::from(&path);
    let fmt = ExportFormat::from_path(&out).ok_or("Choose a file ending in .docx or .txt.")?;
    let p = Project::open(&current(&state)?).map_err(err)?;
    engine::export(&p, &id, fmt, &out, &options).map_err(err)
}

#[derive(Serialize)]
struct ModelStatus {
    installed: bool,
    message: String,
    dir: String,
    pack: String,
    download_mb: u64,
}

#[tauri::command]
fn models_status() -> ModelStatus {
    let dir = Settings::load().model_dir();
    let r = models::verify(&models::LIGHT_PACK, &dir, false);
    ModelStatus {
        installed: r.is_ok(),
        message: r.err().map(|e| e.to_string()).unwrap_or_default(),
        dir: dir.display().to_string(),
        pack: models::LIGHT_PACK.name.into(),
        download_mb: install::LIGHT_PACK_DOWNLOADS.iter().map(|d| d.bytes).sum::<u64>() / 1_000_000,
    }
}

#[derive(Clone, Serialize)]
struct InstallEvent {
    progress: Option<install::InstallProgress>,
    finished: bool,
    error: Option<String>,
}

#[tauri::command]
fn models_install(app: tauri::AppHandle, state: State<AppState>) -> Res<()> {
    let settings = Settings::load();
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
        let r = install::install_light_pack(&settings.model_dir(), settings.offline, &mut |p| {
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
fn models_import_zip(path: String) -> Res<ModelStatus> {
    install::import_zip(&models::LIGHT_PACK, path.as_ref(), &Settings::load().model_dir()).map_err(err)?;
    Ok(models_status())
}

#[tauri::command]
fn settings_get() -> Settings {
    Settings::load()
}

#[tauri::command]
fn settings_set(settings: Settings) -> Res<Settings> {
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
}

#[tauri::command]
fn editor_info(state: State<AppState>, id: String) -> Res<EditorInfo> {
    let dir = current(&state)?;
    let settings = Settings::load();
    let (path, copy) = engine::playback_file(&dir, &id, &settings).map_err(err)?;
    let p = Project::open(&dir).map_err(err)?;
    let m = p.media(&id).map_err(err)?;
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
    let list: Vec<String> = list.into_iter().map(|s| s.trim().to_string()).filter(|s| s.starts_with('[') && s.ends_with(']')).collect();
    Project::update(&current(&state)?, |p| {
        p.file.notation = list.clone();
        Ok(())
    })
    .map_err(err)?;
    Ok(list)
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
            hotkeys_default
        ])
        .run(tauri::generate_context!())
        .expect("Prism Transcribe could not start");
}
