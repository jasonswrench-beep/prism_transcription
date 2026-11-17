//! The model manager's installing half: download a pack from fixed addresses with SHA-256 checks, or import it
//! from an offline zip (open decision 5: no models inside the installer).
//!
//! The network is touched only by [`install_light_pack`], only when the user starts it, and never when `offline` is
//! set (the Offline switch): that check comes before any connection. Downloads land in `downloads/` beside the
//! models as `.part` files and are kept only if size and SHA-256 match exactly.

use crate::models::{self, ModelPack, LIGHT_PACK};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("Prism Transcribe is set to work offline, so nothing was downloaded. Turn off Offline in Settings, or import the models from an offline zip.")]
    Offline,
    #[error("the download from {url} failed: {message}")]
    Network { url: String, message: String },
    #[error("{file} did not match its published checksum (got {got}); it was thrown away")]
    Checksum { file: String, got: String },
    #[error("{file} is larger than expected ({bytes} bytes); the download was stopped")]
    TooLarge { file: String, bytes: u64 },
    #[error("the offline zip is missing {0}")]
    ZipMissing(String),
    #[error("the zip could not be read: {0}")]
    Zip(String),
    #[error("the install was cancelled")]
    Cancelled,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Speech(#[from] crate::SpeechError),
}

pub type Result<T> = std::result::Result<T, InstallError>;

/// What to do with a downloaded file once it checks out.
pub enum Unpack {
    /// Keep it as `dest`.
    File { dest: &'static str },
    /// A .tar.bz2: keep these members (path inside the archive, destination).
    TarBz2 { members: &'static [(&'static str, &'static str)] },
    /// Append one metadata entry to an ONNX model and keep it as `dest` (the WeSpeaker fix, docs/MODELS.md).
    AppendOnnxMetadata { key: &'static str, value: &'static str, dest: &'static str },
}

pub struct Download {
    pub url: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub unpack: Unpack,
}

/// Where each light-pack file comes from (docs/MODELS.md has the licences).
pub const LIGHT_PACK_DOWNLOADS: &[Download] = &[
    Download {
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx",
        file: "silero_vad.onnx",
        bytes: 643_854,
        sha256: "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6",
        unpack: Unpack::File { dest: "silero_vad.onnx" },
    },
    Download {
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2",
        file: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2",
        bytes: 487_211_443,
        sha256: "1495e91b59d7fc53ddca91f3431758ec99f8fa253fdb0e1e851be0f72b14b0f9",
        unpack: Unpack::TarBz2 {
            members: &[
                ("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/encoder.int8.onnx", "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/encoder.int8.onnx"),
                ("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/decoder.int8.onnx", "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/decoder.int8.onnx"),
                ("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/joiner.int8.onnx", "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/joiner.int8.onnx"),
                ("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/tokens.txt", "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/tokens.txt"),
            ],
        },
    },
    Download {
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_resnet34_LM.onnx",
        file: "wespeaker_en_voxceleb_resnet34_LM.onnx",
        bytes: 26_530_550,
        sha256: "e9848563da86f263117134dfd7ad63c92355b37de492b55e325400c9d9c39012",
        unpack: Unpack::AppendOnnxMetadata { key: "feature_normalize_type", value: "global-mean", dest: models::EMBEDDING_FILE },
    },
];

#[derive(Clone, Debug, serde::Serialize)]
pub struct InstallProgress {
    pub file: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    /// Bytes of the whole pack, for one progress bar.
    pub pack_done: u64,
    pub pack_total: u64,
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// The bytes that add one `metadata_props` entry (field 14 of ONNX's ModelProto) when appended to a model file.
/// Protobuf merges fields that appear again later in a message, so appending is enough; nothing is parsed.
pub fn onnx_metadata_entry(key: &str, value: &str) -> Vec<u8> {
    fn varint(mut n: usize, out: &mut Vec<u8>) {
        loop {
            let b = (n & 0x7f) as u8;
            n >>= 7;
            out.push(if n == 0 { b } else { b | 0x80 });
            if n == 0 {
                break;
            }
        }
    }
    fn field(num: u8, bytes: &[u8], out: &mut Vec<u8>) {
        out.push((num << 3) | 2);
        varint(bytes.len(), out);
        out.extend_from_slice(bytes);
    }
    let mut entry = Vec::new();
    field(1, key.as_bytes(), &mut entry);
    field(2, value.as_bytes(), &mut entry);
    let mut out = Vec::new();
    field(14, &entry, &mut out);
    out
}

fn fetch(agent: &ureq::Agent, d: &Download, part: &Path, progress: &mut dyn FnMut(u64) -> bool) -> Result<()> {
    let net = |m: String| InstallError::Network { url: d.url.into(), message: m };
    let mut resp = agent.get(d.url).call().map_err(|e| net(e.to_string()))?;
    let mut reader = resp.body_mut().as_reader();
    let mut f = std::fs::File::create(part)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut n_total = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| net(e.to_string()))?;
        if n == 0 {
            break;
        }
        n_total += n as u64;
        if n_total > d.bytes {
            drop(f);
            let _ = std::fs::remove_file(part);
            return Err(InstallError::TooLarge { file: d.file.into(), bytes: n_total });
        }
        h.update(&buf[..n]);
        f.write_all(&buf[..n])?;
        if !progress(n_total) {
            drop(f);
            let _ = std::fs::remove_file(part);
            return Err(InstallError::Cancelled);
        }
    }
    f.sync_all()?;
    let got = hex(&h.finalize());
    if got != d.sha256 || n_total != d.bytes {
        drop(f);
        let _ = std::fs::remove_file(part);
        return Err(InstallError::Checksum { file: d.file.into(), got });
    }
    Ok(())
}

fn unpack(d: &Download, file: &Path, model_dir: &Path) -> Result<()> {
    match &d.unpack {
        Unpack::File { dest } => {
            std::fs::copy(file, model_dir.join(dest))?;
        }
        Unpack::TarBz2 { members } => {
            let mut ar = tar::Archive::new(bzip2::read::BzDecoder::new(std::fs::File::open(file)?));
            for entry in ar.entries()? {
                let mut e = entry?;
                let path = e.path()?.to_string_lossy().replace('\\', "/");
                if let Some((_, dest)) = members.iter().find(|(m, _)| *m == path) {
                    let out = model_dir.join(dest);
                    std::fs::create_dir_all(out.parent().unwrap())?;
                    let mut w = std::fs::File::create(&out)?;
                    std::io::copy(&mut e, &mut w)?;
                }
            }
        }
        Unpack::AppendOnnxMetadata { key, value, dest } => {
            let mut bytes = std::fs::read(file)?;
            bytes.extend(onnx_metadata_entry(key, value));
            std::fs::write(model_dir.join(dest), bytes)?;
        }
    }
    Ok(())
}

/// Download and install the light pack. Skips anything already installed and correct. `progress` returns false
/// to cancel.
pub fn install_light_pack(model_dir: &Path, offline: bool, progress: &mut dyn FnMut(InstallProgress) -> bool) -> Result<()> {
    install(&LIGHT_PACK, LIGHT_PACK_DOWNLOADS, model_dir, offline, progress)
}

pub fn install(pack: &ModelPack, downloads: &[Download], model_dir: &Path, offline: bool, progress: &mut dyn FnMut(InstallProgress) -> bool) -> Result<()> {
    if models::verify(pack, model_dir, true).is_ok() {
        return Ok(());
    }
    // The Offline switch: refuse before any connection is made.
    if offline {
        return Err(InstallError::Offline);
    }
    let dl_dir = model_dir.join("downloads");
    std::fs::create_dir_all(&dl_dir)?;
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(None).build().into();
    let pack_total: u64 = downloads.iter().map(|d| d.bytes).sum();
    let mut pack_done = 0u64;
    for d in downloads {
        let kept = dl_dir.join(d.file);
        let have = std::fs::metadata(&kept).map(|m| m.len() == d.bytes).unwrap_or(false) && models::sha256_file(&kept)? == d.sha256;
        if !have {
            let part = dl_dir.join(format!("{}.part", d.file));
            let base = pack_done;
            fetch(&agent, d, &part, &mut |n| progress(InstallProgress { file: d.file.into(), done_bytes: n, total_bytes: d.bytes, pack_done: base + n, pack_total }))?;
            std::fs::rename(&part, &kept)?;
        }
        pack_done += d.bytes;
        unpack(d, &kept, model_dir)?;
    }
    models::verify(pack, model_dir, true)?;
    // The downloads have served their purpose; the installed files are checked.
    let _ = std::fs::remove_dir_all(&dl_dir);
    Ok(())
}

/// Install a pack from an offline zip that holds the installed files at their paths (as [`export_zip`] writes).
/// Every file is checked against the pack's SHA-256 before anything is written into the model folder.
pub fn import_zip(pack: &ModelPack, zip_path: &Path, model_dir: &Path) -> Result<()> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(zip_path)?).map_err(|e| InstallError::Zip(e.to_string()))?;
    let staging = model_dir.join("downloads").join("from-zip");
    std::fs::create_dir_all(&staging)?;
    for f in pack.files {
        let mut e = z.by_name(f.path).map_err(|_| InstallError::ZipMissing(f.path.into()))?;
        let out = staging.join(f.path);
        std::fs::create_dir_all(out.parent().unwrap())?;
        std::io::copy(&mut e, &mut std::fs::File::create(&out)?)?;
        let got = models::sha256_file(&out)?;
        if got != f.sha256 {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(InstallError::Checksum { file: f.path.into(), got });
        }
    }
    for f in pack.files {
        let dest = model_dir.join(f.path);
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::rename(staging.join(f.path), &dest)?;
    }
    let _ = std::fs::remove_dir_all(model_dir.join("downloads"));
    models::verify(pack, model_dir, true)?;
    Ok(())
}

/// Write an offline zip of an installed pack, for computers with no internet (lab machines).
pub fn export_zip(pack: &ModelPack, model_dir: &Path, zip_path: &Path) -> Result<PathBuf> {
    models::verify(pack, model_dir, true)?;
    let mut zw = zip::ZipWriter::new(std::fs::File::create(zip_path)?);
    // The models do not compress; store them.
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true);
    for f in pack.files {
        zw.start_file(f.path, o).map_err(|e| InstallError::Zip(e.to_string()))?;
        std::io::copy(&mut std::fs::File::open(model_dir.join(f.path))?, &mut zw)?;
    }
    zw.finish().map_err(|e| InstallError::Zip(e.to_string()))?;
    Ok(zip_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ModelFile, ModelPack};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A tiny local server that counts connections and answers every request with `body`.
    fn server(body: &'static [u8]) -> (String, Arc<AtomicUsize>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { continue };
                h.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(body);
            }
        });
        (format!("http://{addr}/model.onnx"), hits)
    }

    const TEST_PACK: ModelPack = ModelPack {
        id: "test",
        name: "test",
        files: &[ModelFile { path: "model.onnx", bytes: 11, sha256: "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9" }],
    };

    fn dl(url: &'static str) -> Vec<Download> {
        vec![Download { url, file: "model.onnx", bytes: 11, sha256: "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9", unpack: Unpack::File { dest: "model.onnx" } }]
    }

    #[test]
    fn offline_means_no_connection_at_all() {
        let (url, hits) = server(b"hello world");
        let url: &'static str = Box::leak(url.into_boxed_str());
        let d = tempfile::tempdir().unwrap();
        let r = install(&TEST_PACK, &dl(url), d.path(), true, &mut |_| true);
        assert!(matches!(r, Err(InstallError::Offline)));
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(hits.load(Ordering::SeqCst), 0, "offline: the server was never contacted");

        // Online, the same download goes through and is checked ("hello world" has that SHA-256).
        install(&TEST_PACK, &dl(url), d.path(), false, &mut |_| true).unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(std::fs::read(d.path().join("model.onnx")).unwrap(), b"hello world");
        assert!(!d.path().join("downloads").exists(), "downloads are cleared after install");
    }

    #[test]
    fn a_wrong_file_is_thrown_away() {
        let (url, _) = server(b"hello WORLD");
        let url: &'static str = Box::leak(url.into_boxed_str());
        let d = tempfile::tempdir().unwrap();
        let r = install(&TEST_PACK, &dl(url), d.path(), false, &mut |_| true);
        assert!(matches!(r, Err(InstallError::Checksum { .. })), "{r:?}");
        assert!(!d.path().join("model.onnx").exists());
        assert!(!d.path().join("downloads").join("model.onnx.part").exists());
    }

    #[test]
    fn offline_zip_round_trip_and_a_tampered_zip() {
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("model.onnx"), b"hello world").unwrap();
        let z = src.path().join("pack.zip");
        export_zip(&TEST_PACK, src.path(), &z).unwrap();
        let dst = tempfile::tempdir().unwrap();
        import_zip(&TEST_PACK, &z, dst.path()).unwrap();
        assert_eq!(std::fs::read(dst.path().join("model.onnx")).unwrap(), b"hello world");

        // A zip whose file differs is refused, and nothing lands in the model folder.
        let bad = src.path().join("bad.zip");
        let mut zw = zip::ZipWriter::new(std::fs::File::create(&bad).unwrap());
        zw.start_file("model.onnx", zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(b"hello WORLD").unwrap();
        zw.finish().unwrap();
        let dst2 = tempfile::tempdir().unwrap();
        assert!(matches!(import_zip(&TEST_PACK, &bad, dst2.path()), Err(InstallError::Checksum { .. })));
        assert!(!dst2.path().join("model.onnx").exists());
    }

    #[test]
    fn the_metadata_entry_matches_what_phase_0_tested() {
        // 39 bytes; the same bytes made the file sherpa-onnx reads with mean subtraction (docs/MODELS.md).
        let b = onnx_metadata_entry("feature_normalize_type", "global-mean");
        assert_eq!(b.len(), 39);
        assert_eq!(&b[..2], &[0x72, 37]);
    }
}
