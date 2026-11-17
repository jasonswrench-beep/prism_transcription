//! Prism Transcribe's data model, projects and export (no speech code here: that is `prism-transcribe-speech`).
//!
//! Phase 0 holds only the schema version. Phase 1 builds the model the spec names: project, media file, turn,
//! word (text, start, end, confidence, speaker), speaker (label, name, role, attributes), notation tag and
//! snapshot, written as JSON with this schema version, and the plain-text and Word exports.

/// The transcript JSON's `schema_version`. Starts at 1; raised only with a reader for the version before.
pub const SCHEMA_VERSION: u32 = 1;
