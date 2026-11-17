//! The `media` protocol: the editor's audio and video element reads the open project's recordings from
//! `http://media.localhost/<media id>`. Only files of the open project are served, in byte ranges so the player can
//! seek, with a CORS header so the volume boost (Web Audio) may read the samples.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The largest piece sent at once; the player asks for the next.
const MAX_CHUNK: u64 = 4 * 1024 * 1024;

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

pub fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("m4a") | Some("aac") => "audio/mp4",
        Some("flac") => "audio/flac",
        Some("ogg") => "audio/ogg",
        Some("opus") => "audio/ogg",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
}

/// Parse "bytes=a-b", "bytes=a-" or "bytes=-n" against a file of `len` bytes.
pub fn parse_range(header: Option<&str>, len: u64) -> Option<(u64, u64)> {
    let spec = header?.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    let (start, end) = match (a.trim(), b.trim()) {
        ("", n) => {
            let n: u64 = n.parse().ok()?;
            (len.saturating_sub(n), len.checked_sub(1)?)
        }
        (a, "") => (a.parse().ok()?, len.checked_sub(1)?),
        (a, b) => (a.parse().ok()?, b.parse::<u64>().ok()?.min(len.checked_sub(1)?)),
    };
    (start <= end && end < len).then_some((start, end))
}

pub fn serve(path: &Path, range: Option<&str>) -> Reply {
    let cors = ("Access-Control-Allow-Origin", "*".to_string());
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Reply { status: 404, headers: vec![cors], body: Vec::new() },
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let ct = ("Content-Type", content_type(path).to_string());
    let ranges = ("Accept-Ranges", "bytes".to_string());
    let (start, end) = match range {
        Some(_) => match parse_range(range, len) {
            Some(r) => r,
            None => return Reply { status: 416, headers: vec![cors, ("Content-Range", format!("bytes */{len}"))], body: Vec::new() },
        },
        None => (0, len.saturating_sub(1)),
    };
    let end = end.min(start + MAX_CHUNK - 1);
    let mut body = vec![0u8; (end + 1 - start) as usize];
    if len == 0 || f.seek(SeekFrom::Start(start)).is_err() || f.read_exact(&mut body).is_err() {
        return Reply { status: 500, headers: vec![cors], body: Vec::new() };
    }
    let partial = range.is_some() || end + 1 < len;
    let mut headers = vec![cors, ct, ranges, ("Content-Length", body.len().to_string())];
    if partial {
        headers.push(("Content-Range", format!("bytes {start}-{end}/{len}")));
    }
    Reply { status: if partial { 206 } else { 200 }, headers, body }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range(Some("bytes=0-"), 100), Some((0, 99)));
        assert_eq!(parse_range(Some("bytes=10-19"), 100), Some((10, 19)));
        assert_eq!(parse_range(Some("bytes=90-500"), 100), Some((90, 99)));
        assert_eq!(parse_range(Some("bytes=-10"), 100), Some((90, 99)));
        assert_eq!(parse_range(Some("bytes=100-"), 100), None);
        assert_eq!(parse_range(Some("nonsense"), 100), None);
    }

    #[test]
    fn serves_a_piece_of_a_file() {
        let d = std::env::temp_dir().join(format!("ptx-media-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("a.mp3");
        std::fs::write(&p, (0u8..=255).collect::<Vec<u8>>()).unwrap();
        let r = serve(&p, Some("bytes=16-31"));
        assert_eq!(r.status, 206);
        assert_eq!(r.body, (16u8..=31).collect::<Vec<u8>>());
        assert!(r.headers.contains(&("Content-Range", "bytes 16-31/256".into())));
        assert!(r.headers.contains(&("Content-Type", "audio/mpeg".into())));
        assert_eq!(serve(&p, None).status, 200);
        assert_eq!(serve(&p, Some("bytes=300-")).status, 416);
        assert_eq!(serve(&d.join("missing.wav"), None).status, 404);
        let _ = std::fs::remove_dir_all(&d);
    }
}
