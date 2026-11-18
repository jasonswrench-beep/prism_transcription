//! The data handling statement: one page in plain language for an ethics protocol, saying what is stored, where,
//! which models run, that no audio or text is transmitted, and how to delete everything. It is written from the
//! project's and the computer's actual set-up, so it stays true when settings change; it names no participant.

use crate::{deid, pack_status, Result, Settings};
use prism_transcribe_core::export::Block;
use prism_transcribe_core::project::Project;
use prism_transcribe_speech::models::{HEAVY_PACK, LIGHT_PACK};
use std::path::Path;

pub fn blocks(dir: &Path, settings: &Settings) -> Result<Vec<Block>> {
    use Block::*;
    let p = Project::open(dir)?;
    let light = pack_status(settings, &LIGHT_PACK).installed;
    // Whisper is named when it can run here, or when a transcript in the project was made with it (or a file is set
    // to use it), even if the program has since moved.
    let heavy_used = p.file.files.iter().any(|m| {
        m.fields.pack.as_deref() == Some(HEAVY_PACK.id) || p.load_transcript(&m.id).ok().flatten().is_some_and(|t| t.provenance.pack == HEAVY_PACK.id)
    });
    let heavy = heavy_used || pack_status(settings, &HEAVY_PACK).installed;
    let copied = p.file.files.iter().filter(|m| m.copied).count();
    let linked = p.file.files.len() - copied;
    let key = deid::key_path(settings, &p);
    let today = prism_transcribe_core::project::today();

    let mut b = vec![
        Heading(format!("Data handling: {}", p.file.name)),
        Para(format!(
            "How the recordings in this project are transcribed and kept. Written by Prism Transcribe {} on {today} from the project's own settings.",
            env!("CARGO_PKG_VERSION")
        )),
        Heading("Transcription happens on this computer".into()),
        Para("Recordings are transcribed by Prism Transcribe on the researcher's own Windows computer. No audio, transcript, name or other text is sent anywhere: not to a cloud service, and not to the makers of the app or of the models. Nothing is counted or reported about how the app is used, and there is no crash upload.".into()),
        Para("The speech models run on the computer itself:".into()),
    ];
    let mut models = vec![
        "Silero VAD (Silero Team, MIT licence) finds where people speak.",
        "Parakeet TDT 0.6B v3 (NVIDIA, CC BY 4.0) turns speech into text in 25 European languages.",
        "WeSpeaker ResNet34-LM (WeSpeaker, trained on VoxCeleb, CC BY 4.0) tells the speakers apart.",
    ];
    if heavy {
        models.push("Whisper large-v3-turbo (OpenAI, MIT licence), run by whisper.cpp (MIT), is used for other languages, or when chosen.");
    }
    b.extend(models.into_iter().map(|m| Bullet(m.into())));
    if !light {
        b.push(Para("(The speech models are not installed on this computer yet.)".into()));
    }

    b.push(Heading("Network use".into()));
    b.push(Para(format!(
        "Prism Transcribe makes one kind of network call itself: downloading the speech models, from fixed addresses, each checked against its SHA-256, and only when the user starts it in the model manager. Updates to the app are checked by Prism Hub, not by Prism Transcribe. An Offline switch blocks every call; on this computer it is {}.",
        if settings.offline { "on, so the app makes no network call at all" } else { "off" }
    )));

    b.push(Heading("What is stored, and where".into()));
    b.push(Bullet(match (linked, copied) {
        (_, 0) => "The original recordings stay where they were. The project links to them and never changes them; a checksum taken at import shows if one is changed later.".to_string(),
        (0, _) => "The recordings are copied into the project folder's media folder; the originals are not changed.".to_string(),
        _ => format!("{linked} recording(s) stay where they were and are linked; {copied} are copied into the project folder's media folder. Originals are never changed."),
    }));
    b.push(Bullet(format!("The project folder, {}, holds the transcripts (one file per recording, with its earlier versions), the project settings, a cache of waveforms and playback copies, and, only while a transcription runs, temporary audio, which is deleted when it ends.", p.dir.display())));
    b.push(Bullet(if p.is_protected() {
        "The transcripts are password protected: sealed with XChaCha20-Poly1305 under a key made from the password with Argon2id (both from the open-source RustCrypto project). Without the password they cannot be read, and the password is not stored anywhere.".to_string()
    } else {
        "The transcripts are not password protected; anyone who can open the project folder can read them.".to_string()
    }));
    b.push(Bullet(match &key {
        Some(k) => format!("The key file, which holds the real names, their replacements and the log of every de-identification change, is kept apart from the transcripts, at {}. It is never included in an export.", k.display()),
        None => "No key file has been made yet: de-identification has not started in this project. When it is, the key file (the real names, their replacements and the log of changes) is kept apart from the transcripts and never exported.".to_string(),
    }));
    b.push(Bullet(format!(
        "The speech models are in {}, shared by the Prism tools that use them; the app's settings are in {}.",
        settings.model_dir().display(),
        Settings::path().parent().map(|d| d.display().to_string()).unwrap_or_default()
    )));

    b.push(Heading("De-identification".into()));
    b.push(Para("Real names and places are replaced from a list the researcher keeps, with consistent replacements (for example P07, [Participant 3], [City]); a pass flags email addresses, phone numbers, street addresses, dates, ID-like numbers and web addresses for review; another lists capitalised words that may be names. Every change is logged with the original, its replacement, the file, the time in the recording, when it was made and who accepted it. Before an export marked de-identified, a final check confirms that no name on the list is left. The earlier versions kept inside a transcript (for undoing) still hold the original words; they stay in the project folder and are never exported.".into()));

    b.push(Heading("Protecting and deleting the data".into()));
    b.push(Para("Full-disk encryption (BitLocker, or the institution's equivalent) is recommended whether or not the transcripts have a password, because recordings, waveforms and temporary audio are not covered by it. To delete everything: delete the project folder, the key file, and the original recordings if they are no longer needed; the models folder and the settings folder above can be deleted too (other Prism tools may share the models). Deleting a file does not securely erase it from every kind of drive; full-disk encryption covers that.".into()));
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_statement_says_what_is_true() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("proj");
        Project::create(&dir, "Study A").unwrap();
        let s = Settings { offline: true, ..Default::default() };
        let text = prism_transcribe_core::export::blocks_to_text(&blocks(&dir, &s).unwrap());
        assert!(text.starts_with("Data handling: Study A\n"));
        assert!(text.contains("on this computer it is on, so the app makes no network call at all"));
        assert!(text.contains("are not password protected"));
        assert!(text.contains("No key file has been made yet"));
        assert!(text.contains("\n- Parakeet TDT 0.6B v3"));
        // Roughly a page.
        assert!(text.split_whitespace().count() < 900, "{} words", text.split_whitespace().count());
        let docx = prism_transcribe_core::export::blocks_to_docx("Data handling", &blocks(&dir, &s).unwrap()).unwrap();
        assert!(docx.starts_with(b"PK"));
    }
}
