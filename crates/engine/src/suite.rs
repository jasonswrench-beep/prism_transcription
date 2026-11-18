//! The other Prism tools this app hands work to: PrismQDA (where exported transcripts are coded) and Prism Hub
//! (which installs it). Both install per user, where Tauri's installer puts them: `%LOCALAPPDATA%\<product>`.
//! Prism Hub has no link or switch for "install this app" yet, so without PrismQDA the app opens Hub itself.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize)]
pub struct SuiteApps {
    pub prismqda: Option<PathBuf>,
    pub hub: Option<PathBuf>,
}

fn local(product: &str, program: &str) -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(product).join(program);
    p.is_file().then_some(p)
}

pub fn find() -> SuiteApps {
    SuiteApps { prismqda: local("PrismQDA", "prismqda-app.exe"), hub: local("Prism Hub", "prism-hub.exe") }
}

/// Start a program on its own, not waiting for it.
pub fn launch(program: &std::path::Path) -> std::io::Result<()> {
    let mut c = std::process::Command::new(program);
    if let Some(d) = program.parent() {
        c.current_dir(d);
    }
    c.spawn().map(|_| ())
}
