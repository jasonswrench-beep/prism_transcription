//! The model packs: which files, from where, under which licence, with which SHA-256.
//!
//! Models live in one shared folder per computer so every suite tool that uses C5 finds the same files
//! (proposed in `CORE-REQUESTS.md`; the core has not fixed the location yet):
//! `%LOCALAPPDATA%\OpenStage\shared-models\speech\`, or the folder named by `PRISM_SPEECH_MODELS`.
//! The full record (licence text, where each file came from) is `docs/MODELS.md`.

use crate::{Result, SpeechError};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct ModelFile {
    /// Path under the shared speech-model folder.
    pub path: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub struct ModelPack {
    pub id: &'static str,
    pub name: &'static str,
    pub files: &'static [ModelFile],
}

pub const PARAKEET_DIR: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
/// Only for [`crate::sherpa::PyannoteLabeller`], the comparison labeller; not part of the light pack.
pub const SEGMENTATION_DIR: &str = "sherpa-onnx-pyannote-segmentation-3-0";
/// WeSpeaker ResNet34-LM (VoxCeleb, CC BY 4.0) with per-utterance mean subtraction added at its input by
/// tools/add_mean_subtraction.py, because sherpa-onnx does not apply it to "wespeaker" models (docs/MODELS.md).
pub const EMBEDDING_FILE: &str = "wespeaker_en_voxceleb_resnet34_LM.cmn.onnx";

/// The light default pack: CPU only, every machine.
pub const LIGHT_PACK: ModelPack = ModelPack {
    id: "light-1",
    name: "Light pack (Parakeet TDT 0.6B v3, WeSpeaker ResNet34-LM, Silero VAD)",
    files: &[
        ModelFile { path: "silero_vad.onnx", bytes: 643_854, sha256: "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6" },
        ModelFile { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/encoder.int8.onnx", bytes: 652_282_294, sha256: "1afbe69c1ffdabee94755f42e4daf1d93549131a3da11356bda84fcef0f2c479" },
        ModelFile { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/decoder.int8.onnx", bytes: 11_845_274, sha256: "0222a86147299d4dee90edf95b6d13827edb42ad8b7c9cf0b2582f030937c0b5" },
        ModelFile { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/joiner.int8.onnx", bytes: 6_355_277, sha256: "cbd02f3696e9944970111fb439d9c3fd365a80374565f13f2b9c7007d253fc98" },
        ModelFile { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/tokens.txt", bytes: 93_939, sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d" },
        ModelFile { path: "wespeaker_en_voxceleb_resnet34_LM.cmn.onnx", bytes: 26_530_764, sha256: "d467fd5b50a917ed4c986fabddd3913e4dfddfc3eaf21af96adbddf77bcf8e52" },
    ],
};

/// The 25 languages Parakeet TDT 0.6B v3 recognizes (its model card).
pub const PARAKEET_LANGUAGES: &[&str] = &[
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt", "pl", "pt", "ro", "sk",
    "sl", "es", "sv", "ru", "uk",
];

/// The shared speech-model folder.
pub fn default_model_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("PRISM_SPEECH_MODELS") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("OpenStage").join("shared-models").join("speech")
}

/// Check that every file of a pack is there with the right size; with `hash`, also the SHA-256 (slower: the
/// Parakeet encoder alone is 650 MB). Returns the first problem found.
pub fn verify(pack: &ModelPack, dir: &Path, hash: bool) -> Result<()> {
    for f in pack.files {
        let p = dir.join(f.path);
        let meta = std::fs::metadata(&p).map_err(|_| SpeechError::MissingModel(p.display().to_string()))?;
        if meta.len() != f.bytes {
            return Err(SpeechError::MissingModel(format!("{} (size {} bytes, expected {})", p.display(), meta.len(), f.bytes)));
        }
        if hash {
            let got = sha256_file(&p)?;
            if got != f.sha256 {
                return Err(SpeechError::MissingModel(format!("{} (SHA-256 {got}, expected {})", p.display(), f.sha256)));
            }
        }
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
