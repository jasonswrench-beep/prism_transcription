//! Prism Transcribe's data model, projects and export. No speech or media code here: those are
//! `prism-transcribe-speech` and `prism-transcribe-media`, and `prism-transcribe-engine` ties them together.

pub mod edit;
pub mod export;
pub mod model;
pub mod project;

pub use model::SCHEMA_VERSION;
