// Prism Transcribe's desktop shell. Phase 0: a window that opens and says which model pack is installed.
// Phase 1 adds projects, import, the Transcribe button and the read-only transcript view.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use prism_transcribe_speech::models::{self, LIGHT_PACK};

#[tauri::command]
fn model_status() -> String {
    let dir = models::default_model_dir();
    match models::verify(&LIGHT_PACK, &dir, false) {
        Ok(()) => format!("{} is installed in {}", LIGHT_PACK.name, dir.display()),
        Err(e) => format!("The light pack is not installed yet ({e})"),
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![model_status])
        .run(tauri::generate_context!())
        .expect("Prism Transcribe could not start");
}
