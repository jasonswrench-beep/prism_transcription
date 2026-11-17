//! Decode a recording and write the 16 kHz, mono, 16-bit WAV copy the speech engine reads (T1, phase 1).
//!
//! WAV, MP3, M4A/AAC, FLAC and OGG Vorbis decode in Rust (symphonia). Video and everything else (WMA, OPUS, MKV,
//! AVI, WEBM, MOV and MP4 that symphonia cannot read) go through FFmpeg as a **separate program**, never linked:
//! the spec plans an LGPL-only build shipped beside the app. Until that is settled (TESTING-TODO.md), the app uses
//! an `ffmpeg.exe` it finds: the path in `PRISM_FFMPEG`, one beside the program, or one on the PATH.
//!
//! The original is only read. Playback uses the original; the 16 kHz copy lives in the project's `temp` folder.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use std::path::{Path, PathBuf};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

pub const TARGET_RATE: u32 = 16_000;

/// The file types import accepts (T1).
pub const IMPORT_EXTENSIONS: &[&str] = &["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus", "wma", "mp4", "mov", "mkv", "avi", "webm"];

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("this file could not be read as audio ({0}), and FFmpeg is not available to try it")]
    NeedsFfmpeg(String),
    #[error("FFmpeg could not read this file: {0}")]
    Ffmpeg(String),
    #[error("this file has no audio track")]
    NoAudio,
    #[error("the audio could not be decoded: {0}")]
    Decode(String),
    #[error("the job was cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, MediaError>;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct MediaInfo {
    /// Seconds of audio in the 16 kHz copy.
    pub duration: f64,
    pub source_rate: u32,
    pub source_channels: usize,
    pub has_video: bool,
    pub audio_tracks: usize,
    /// "rust" or "ffmpeg".
    pub decoder: String,
}

/// Turn mono samples at any rate into 16 kHz and hand them on in blocks, keeping memory flat.
struct To16k {
    rs: Option<Async<f32>>,
    pending: Vec<f32>,
    out: Vec<f32>,
    delay_left: usize,
    produced: u64,
    seen: u64,
    ratio: f64,
}

type Sink<'a> = dyn FnMut(&[f32]) -> Result<()> + 'a;

impl To16k {
    fn new(rate: u32) -> Result<Self> {
        let ratio = TARGET_RATE as f64 / rate as f64;
        if rate == TARGET_RATE {
            return Ok(Self { rs: None, pending: Vec::new(), out: Vec::new(), delay_left: 0, produced: 0, seen: 0, ratio });
        }
        let params = SincInterpolationParameters::new(128, WindowFunction::BlackmanHarris2).oversampling_factor(128).interpolation(SincInterpolationType::Linear);
        let rs = Async::<f32>::new_sinc(ratio, 1.0, &params, 4096, 1, FixedAsync::Input).map_err(|e| MediaError::Decode(e.to_string()))?;
        let delay = rs.output_delay();
        let out = vec![0.0; rs.output_frames_max()];
        Ok(Self { rs: Some(rs), pending: Vec::new(), out, delay_left: delay, produced: 0, seen: 0, ratio })
    }

    /// Resample every whole block waiting, dropping the resampler's start-up delay; with `cap`, stop at that many
    /// output samples in all.
    fn drain(&mut self, cap: Option<u64>, sink: &mut Sink) -> Result<()> {
        let rs = self.rs.as_mut().expect("drain is only called when resampling");
        while self.pending.len() >= rs.input_frames_next() {
            let need = rs.input_frames_next();
            let input = InterleavedSlice::new(&self.pending[..need], 1, need).map_err(|e| MediaError::Decode(e.to_string()))?;
            let room = self.out.len();
            let mut output = InterleavedSlice::new_mut(&mut self.out, 1, room).map_err(|e| MediaError::Decode(e.to_string()))?;
            let (_, n) = rs.process_into_buffer(&input, &mut output, None).map_err(|e| MediaError::Decode(e.to_string()))?;
            self.pending.drain(..need);
            let skip = self.delay_left.min(n);
            self.delay_left -= skip;
            let mut keep = &self.out[skip..n];
            if let Some(c) = cap {
                keep = &keep[..keep.len().min(c.saturating_sub(self.produced) as usize)];
            }
            self.produced += keep.len() as u64;
            if !keep.is_empty() {
                sink(keep)?;
            }
        }
        Ok(())
    }

    fn push(&mut self, mono: &[f32], sink: &mut Sink) -> Result<()> {
        self.seen += mono.len() as u64;
        if self.rs.is_none() {
            self.produced += mono.len() as u64;
            return sink(mono);
        }
        self.pending.extend_from_slice(mono);
        self.drain(None, sink)
    }

    /// Push silence through until every input sample has come out, stopping at exactly the expected length.
    fn finish(&mut self, sink: &mut Sink) -> Result<()> {
        let Some(rs) = self.rs.as_ref() else { return Ok(()) };
        let expected = (self.seen as f64 * self.ratio).round() as u64;
        let need = rs.input_frames_next();
        for _ in 0..64 {
            if self.produced >= expected {
                break;
            }
            self.pending.resize(self.pending.len().max(need), 0.0);
            self.drain(Some(expected), sink)?;
        }
        Ok(())
    }
}

fn wav_writer(path: &Path) -> Result<hound::WavWriter<std::io::BufWriter<std::fs::File>>> {
    let spec = hound::WavSpec { channels: 1, sample_rate: TARGET_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    hound::WavWriter::create(path, spec).map_err(|e| MediaError::Io(std::io::Error::other(e)))
}

/// Decode `input` with symphonia and write the 16 kHz copy to `output`. `progress` gets seconds of source audio
/// decoded so far and returns false to cancel.
pub fn decode_rust(input: &Path, output: &Path, progress: &mut dyn FnMut(f64) -> bool) -> Result<MediaInfo> {
    let src = std::fs::File::open(input)?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| MediaError::Decode(e.to_string()))?;
    let has_video = format.tracks().iter().any(|t| matches!(t.codec_params, Some(CodecParameters::Video(_))));
    let audio_tracks = format.tracks().iter().filter(|t| matches!(t.codec_params, Some(CodecParameters::Audio(_)))).count();
    let track = format.default_track(TrackType::Audio).ok_or(MediaError::NoAudio)?;
    let track_id = track.id;
    let params = match &track.codec_params {
        Some(CodecParameters::Audio(a)) => a.clone(),
        _ => return Err(MediaError::NoAudio),
    };
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(&params, &Default::default()).map_err(|e| MediaError::Decode(e.to_string()))?;

    let mut writer = wav_writer(output)?;
    let mut sink = |s: &[f32]| -> Result<()> {
        for &x in s {
            writer.write_sample((x.clamp(-1.0, 1.0) * 32767.0).round() as i16).map_err(|e| MediaError::Io(std::io::Error::other(e)))?;
        }
        Ok(())
    };
    let mut to16: Option<To16k> = None;
    let (mut rate, mut channels) = (0u32, 0usize);
    let mut interleaved: Vec<f32> = Vec::new();
    let mut mono: Vec<f32> = Vec::new();
    let mut frames_in = 0u64;
    let mut last_report = 0.0;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(MediaError::Decode(e.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => continue, // a damaged packet: skip it
            Err(e) => return Err(MediaError::Decode(e.to_string())),
        };
        if to16.is_none() {
            rate = buf.spec().rate();
            channels = buf.spec().channels().count().max(1);
            to16 = Some(To16k::new(rate)?);
        }
        interleaved.resize(buf.samples_interleaved(), 0.0);
        buf.copy_to_slice_interleaved(&mut interleaved[..]);
        mono.clear();
        mono.extend(interleaved.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32));
        frames_in += mono.len() as u64;
        to16.as_mut().unwrap().push(&mono, &mut sink)?;
        let t = frames_in as f64 / rate as f64;
        if t - last_report >= 5.0 {
            last_report = t;
            if !progress(t) {
                return Err(MediaError::Cancelled);
            }
        }
    }
    let Some(mut r) = to16 else { return Err(MediaError::NoAudio) };
    r.finish(&mut sink)?;
    let produced = r.produced;
    writer.finalize().map_err(|e| MediaError::Io(std::io::Error::other(e)))?;
    Ok(MediaInfo { duration: produced as f64 / TARGET_RATE as f64, source_rate: rate, source_channels: channels, has_video, audio_tracks, decoder: "rust".into() })
}

/// Where FFmpeg is, if anywhere: `PRISM_FFMPEG`, beside this program, or on the PATH.
pub fn find_ffmpeg() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PRISM_FFMPEG").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    if let Some(p) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("ffmpeg.exe"))).filter(|p| p.is_file()) {
        return Some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join("ffmpeg.exe")).find(|p| p.is_file())
}

fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Convert with FFmpeg as a separate program.
pub fn decode_ffmpeg(ffmpeg: &Path, input: &Path, output: &Path) -> Result<MediaInfo> {
    // What is in the file: FFmpeg prints the streams on its error channel when asked for no output.
    let probe = no_window(std::process::Command::new(ffmpeg).args(["-hide_banner", "-nostdin", "-i"]).arg(input)).output()?;
    let info = String::from_utf8_lossy(&probe.stderr).to_string();
    let has_video = info.lines().any(|l| l.contains("Stream #") && l.contains("Video:") && !l.contains("attached pic"));
    let audio_tracks = info.lines().filter(|l| l.contains("Stream #") && l.contains("Audio:")).count();
    if audio_tracks == 0 {
        return Err(if info.contains("Invalid data") || info.contains("No such file") { MediaError::Ffmpeg(info.lines().last().unwrap_or("").to_string()) } else { MediaError::NoAudio });
    }
    let out = no_window(std::process::Command::new(ffmpeg).args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-i"]).arg(input).args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", "-f", "wav"]).arg(output)).output()?;
    if !out.status.success() {
        return Err(MediaError::Ffmpeg(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    let r = hound::WavReader::open(output).map_err(|e| MediaError::Ffmpeg(e.to_string()))?;
    Ok(MediaInfo { duration: r.duration() as f64 / TARGET_RATE as f64, source_rate: 0, source_channels: 0, has_video, audio_tracks, decoder: "ffmpeg".into() })
}

/// Make the 16 kHz copy: Rust first, then FFmpeg when Rust cannot read the file (or for video, whose picture only
/// matters for playback but whose container symphonia may not know).
pub fn to_16k_wav(input: &Path, output: &Path, ffmpeg: Option<&Path>, progress: &mut dyn FnMut(f64) -> bool) -> Result<MediaInfo> {
    match decode_rust(input, output, progress) {
        Ok(info) => Ok(info),
        Err(MediaError::Cancelled) => Err(MediaError::Cancelled),
        Err(e) => {
            let _ = std::fs::remove_file(output);
            match ffmpeg {
                Some(f) => decode_ffmpeg(f, input, output),
                None => Err(MediaError::NeedsFfmpeg(e.to_string())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(path: &Path, rate: u32, channels: u16, seconds: f64) {
        let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (rate as f64 * seconds) as usize;
        for i in 0..n {
            let s = (i as f64 / rate as f64 * 440.0 * std::f64::consts::TAU).sin() * 0.5;
            for _ in 0..channels {
                w.write_sample((s * 32767.0) as i16).unwrap();
            }
        }
        w.finalize().unwrap();
    }

    fn read16(path: &Path) -> Vec<f32> {
        let mut r = hound::WavReader::open(path).unwrap();
        assert_eq!(r.spec().sample_rate, TARGET_RATE);
        assert_eq!(r.spec().channels, 1);
        r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    }

    #[test]
    fn stereo_48k_becomes_16k_mono_of_the_same_length() {
        let d = tempfile::tempdir().unwrap();
        let (i, o) = (d.path().join("in.wav"), d.path().join("out.wav"));
        tone(&i, 48_000, 2, 7.3);
        let info = to_16k_wav(&i, &o, None, &mut |_| true).unwrap();
        let s = read16(&o);
        assert_eq!(s.len(), (7.3 * 16_000.0f64).round() as usize, "exact length");
        assert!((info.duration - 7.3).abs() < 1e-3);
        assert_eq!((info.source_rate, info.source_channels, info.decoder.as_str()), (48_000, 2, "rust"));
        // The tone survives: its level in the middle matches the input's (0.5 peak, about 0.354 RMS).
        let mid = &s[16_000..96_000];
        let rms = (mid.iter().map(|x| x * x).sum::<f32>() / mid.len() as f32).sqrt();
        assert!((rms - 0.3535).abs() < 0.01, "rms {rms}");
    }

    #[test]
    fn a_16k_file_passes_through_unchanged_in_length() {
        let d = tempfile::tempdir().unwrap();
        let (i, o) = (d.path().join("in.wav"), d.path().join("out.wav"));
        tone(&i, 16_000, 1, 2.0);
        to_16k_wav(&i, &o, None, &mut |_| true).unwrap();
        assert_eq!(read16(&o).len(), 32_000);
    }

    #[test]
    fn unreadable_without_ffmpeg_says_so() {
        let d = tempfile::tempdir().unwrap();
        let (i, o) = (d.path().join("in.wma"), d.path().join("out.wav"));
        std::fs::write(&i, b"not really a wma file").unwrap();
        assert!(matches!(to_16k_wav(&i, &o, None, &mut |_| true), Err(MediaError::NeedsFfmpeg(_))));
    }

    /// The other formats, made from a tone by the FFmpeg on this computer if there is one (skipped otherwise).
    #[test]
    fn other_formats_decode_in_rust() {
        let Some(ff) = find_ffmpeg() else {
            eprintln!("no ffmpeg on this computer: skipped");
            return;
        };
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("src.wav");
        tone(&src, 44_100, 2, 5.0);
        for (ext, codec) in [("mp3", "libmp3lame"), ("flac", "flac"), ("ogg", "libvorbis"), ("m4a", "aac")] {
            let f = d.path().join(format!("t.{ext}"));
            let ok = std::process::Command::new(&ff).args(["-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(&src).args(["-c:a", codec]).arg(&f).status().map(|s| s.success()).unwrap_or(false);
            if !ok {
                eprintln!("this ffmpeg cannot write {ext}: skipped");
                continue;
            }
            let o = d.path().join(format!("{ext}.16k.wav"));
            let info = decode_rust(&f, &o, &mut |_| true).unwrap_or_else(|e| panic!("{ext}: {e}"));
            // Encoders add a little padding; the length is within a tenth of a second.
            assert!((info.duration - 5.0).abs() < 0.1, "{ext}: {}", info.duration);
            let s = read16(&o);
            let mid = &s[16_000..48_000];
            let rms = (mid.iter().map(|x| x * x).sum::<f32>() / mid.len() as f32).sqrt();
            assert!((rms - 0.3535).abs() < 0.03, "{ext}: rms {rms}");
        }
        // A video file goes through FFmpeg and is marked as having a picture.
        let v = d.path().join("v.mkv");
        let ok = std::process::Command::new(&ff).args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "color=c=black:s=64x64:d=3", "-i"]).arg(&src).args(["-shortest", "-c:v", "mpeg4", "-c:a", "libvorbis"]).arg(&v).status().map(|s| s.success()).unwrap_or(false);
        if ok {
            let o = d.path().join("v.16k.wav");
            let info = to_16k_wav(&v, &o, Some(&ff), &mut |_| true).unwrap();
            assert!(info.has_video, "{info:?}");
            assert!((info.duration - 3.0).abs() < 0.2, "{info:?}");
        }
    }
}

/// Waveform peaks for the editor: `per_second` pairs of (min, max) a second, scaled to -127..127, read from a 16 kHz
/// WAV. 50 a second keeps an hour at 360 kB.
pub fn peaks_from_wav(path: &Path, per_second: u32) -> Result<Vec<i8>> {
    let mut r = hound::WavReader::open(path).map_err(|e| MediaError::Decode(e.to_string()))?;
    let spec = r.spec();
    let step = (spec.sample_rate / per_second).max(1) as usize;
    let channels = spec.channels.max(1) as usize;
    let mut out = Vec::with_capacity(r.duration() as usize / step * 2 + 2);
    let (mut lo, mut hi, mut n) = (0i32, 0i32, 0usize);
    let scale = |v: i32| (v.clamp(-32768, 32767) / 258) as i8;
    for (i, s) in r.samples::<i16>().enumerate() {
        if i % channels != 0 {
            continue;
        }
        let v = s.map_err(|e| MediaError::Decode(e.to_string()))? as i32;
        lo = lo.min(v);
        hi = hi.max(v);
        n += 1;
        if n == step {
            out.push(scale(lo));
            out.push(scale(hi));
            (lo, hi, n) = (0, 0, 0);
        }
    }
    if n > 0 {
        out.push(scale(lo));
        out.push(scale(hi));
    }
    Ok(out)
}

/// The file types WebView2 plays directly. Anything else is played from a WAV copy made at first use.
pub const PLAYABLE_EXTENSIONS: &[&str] = &["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus", "mp4", "webm"];

pub fn playable(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).map(|e| PLAYABLE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false)
}

#[cfg(test)]
mod peak_tests {
    use super::*;

    #[test]
    fn peaks_follow_the_level() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("t.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&p, spec).unwrap();
        for i in 0..32_000 {
            // One loud second, then one quiet second.
            let amp = if i < 16_000 { 16_000.0 } else { 1_000.0 };
            w.write_sample(((i as f64 * 0.3).sin() * amp) as i16).unwrap();
        }
        w.finalize().unwrap();
        let pk = peaks_from_wav(&p, 50).unwrap();
        assert_eq!(pk.len(), 200, "50 pairs a second for 2 s");
        assert!(pk[1] > 55 && pk[0] < -55, "loud: {} {}", pk[0], pk[1]);
        assert!(pk[199] < 5 && pk[198] > -5, "quiet: {} {}", pk[198], pk[199]);
    }
}
