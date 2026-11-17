// Prism Transcribe's desktop shell. Phase 1: create or open a project, import recordings, transcribe one at a
// time with progress, read the transcript, export it to Word or plain text, and install the speech models.
//
// Long work runs on its own thread and reports through events ("job", "install", "import"); the window never
// waits on it. Nothing here touches the network except the model install, which the Offline switch blocks.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use prism_transcribe_core::export::ExportOptions;
use prism_transcribe_core::model::Transcript;
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
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
            settings_set
        ])
        .run(tauri::generate_context!())
        .expect("Prism Transcribe could not start");
}
