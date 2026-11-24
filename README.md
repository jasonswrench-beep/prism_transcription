# Prism Transcribe

Prism Transcribe turns interview and focus-group recordings into speaker-labelled, timestamped transcripts on the
researcher's own Windows computer, and sends them to PrismQDA. Nothing leaves the machine: the audio, the transcript
and every file in between stay local, and the speech models run on the computer itself.

**Status: 0.1.0-alpha.3, a preview ([releases](https://github.com/jasonswrench-beep/prism_transcription/releases)): signed, for testing, not yet in Prism Hub. From 0.1.0-alpha.2 on, Check for updates installs newer ones.** The app creates or opens a
project, imports recordings, installs and checks its speech models (or imports them from an offline zip), transcribes
them through a batch queue, and opens each transcript in an editor:
- **Panes:** the transcript beside the recording, with its picture when it has one, a waveform and a whole-file overview with a colour band per speaker.
- **Playback:** click a word to play from it, with the playing word highlighted. Speed goes from 0.5 to 2.0 at natural pitch, with a volume boost.
- **Keys and notation:** foot-pedal style keys that are all remappable, rewind on resume, and a notation menu editable per project.
- **Speakers:** rename a speaker once for every turn; give each a role and attributes (pseudonym, age band, site) for PrismQDA; merge two speakers; give a turn, or the words selected in it, to speaker 1 to 9 with Alt+1 to Alt+9; Enter splits a turn and Backspace at its start joins it to the one above; find the speakers again with another expected count, keeping every edit.
- **One microphone per person:** a file with a track or a channel per person is transcribed one microphone at a time, each a known speaker, with the other voices heard through it silenced, and overlaps marked [crosstalk].
- **Two packs:** the light pack (Parakeet, 25 European languages) and the optional heavy pack (Whisper large-v3-turbo, about 99 languages), chosen per file or by its language.
- **Keeping and reviewing:** unlimited undo, autosave every 30 seconds, named snapshots, a review status, and a "checked up to here" marker.
- **Searching:** find and replace in a transcript or the whole project, and a clean view.
- **Batch queue:** any number of files, each with its own settings, in an order set by dragging; run, pause, resume and cancel, or hold one file. The queue survives a restart or crash and each file carries on from its last finished chunk; the computer stays awake while it runs, and a Windows notification and an in-app summary say what finished and what failed and why.
- **Export:** Word and plain text; captions (SubRip and WebVTT, two lines of 42 characters, speaker names optional); the table (CSV and Excel, one row per turn); the PrismQDA package (.prismtx, specified in [docs/PRISMTX.md](docs/PRISMTX.md)); and a REFI-QDA project (.qdpx), which PrismQDA's own importer reads today with each turn timed, the recording linked and each speaker a case. The table and the packages can hold every transcript at once. Every export is verbatim or clean, de-identified or not. After a QDA export the app offers a button that opens PrismQDA, or Prism Hub when PrismQDA is not installed. A package marked de-identified leaves out the recording's file name and folder, which often hold a participant's or a site's name.
- **Help and updates:** Help (the button, or F1) has getting started, the keys as they are set now, privacy in brief and the version. Check for updates (Settings, and Help) asks once when pressed, never on its own, and installs only after a go-ahead and a signature check ([docs/UPDATES.md](docs/UPDATES.md)).
- **De-identification:** a name list per project, typed or pasted from a roster, with consistent replacements (P07, [Participant 3], [City]) applied across every transcript, ignoring case and keeping possessives ("Maria's" becomes "P07's"). A pass flags email addresses, phone numbers, street addresses and postcodes, dates, birth details, ID-like numbers and web addresses, written or spoken. Another lists capitalised words that may be names, using Windows' own dictionary and a project word list. Every change is logged (original, replacement, file, time in the recording, when, who) in a key file kept apart from the transcripts and never exported, and a final check refuses a de-identified export while any name on the list is left.
- **Privacy:** optional password protection of the transcripts and the key file (Argon2id and XChaCha20-Poly1305, from RustCrypto), the Offline switch, and a one-page data handling statement for ethics protocols, written from the project's actual set-up.

Edited words keep their times (T5). The installer is built and signed by `app\BUILD-INSTALLER.ps1` from a tagged
commit; the release steps, and what is left (the Prism Hub entry and PrismQDA's reader item), are in
[docs/RELEASING.md](docs/RELEASING.md). The plan is in the
specification (six phases, requirements T1 to T8); phase 0's measurements are in
[docs/PHASE0-RESULTS.md](docs/PHASE0-RESULTS.md). What only a person can check is in
[TESTING-TODO.md](TESTING-TODO.md).

## Layout

| Folder | What it holds |
|---|---|
| `crates/core` | The transcript data model (JSON, schema version 1), projects (one folder each), the editing rules (word times kept through edits, notation, find and replace, the clean view, speakers: merge, split, reassign), de-identification (name map, pattern and suggestion passes, audit log, key file, final check), password protection, the Word and text exports, captions and the table (`formats`), the .prismtx and .qdpx packages (`package`), and the batch queue's state (`queue`). |
| `crates/media` | Decoding: WAV, MP3, M4A, FLAC and OGG in Rust; video and other formats through an FFmpeg program if one is found. Writes the 16 kHz mono copy the speech engine reads, or one per track or channel. |
| `crates/speech` | The speech engine adapter (C5): detect voice, recognize speech, label speakers, behind one small contract, with sherpa-onnx as the first plugin and whisper.cpp (a separate program) as the second; the resumable transcription job; one microphone per person (bleed and crosstalk); the model manager (download with SHA-256 checks, offline zip). Written to move into the shared core unchanged (see [CORE-REQUESTS.md](CORE-REQUESTS.md)). |
| `crates/engine` | Import, transcribe, de-identify and export joined up, for the app and for `prism-transcribe-cli`; Windows' spell checker for the suggestions pass; the data handling statement. |
| `crates/bench` | `bench.exe`: phase 0's benchmark; speed, memory and, with a truth file, accuracy. |
| `app` | The desktop app: `app/ui` (React and React Aria, built by Vite) inside a Tauri window (`app/src-tauri`). |
| `fixtures` | Two one-minute test recordings with their truth files (who spoke, when, what). Made with computer voices by `tools/make_fixtures.py`; the names, email and phone number in them are invented. |
| `tools` | Scripts used to make test audio and to measure models. Not part of the app. |
| `vendor` | One vendored crate with a one-field change ([vendor/PATCHES.md](vendor/PATCHES.md)). |
| `assets` | The product icon (`icon.svg`); the app's icon sizes in `app/src-tauri/icons` are made from it with `tauri icon`. |
| `docs` | [MODELS.md](docs/MODELS.md) (every model file, its source, licence and SHA-256) and the phase 0 results. |

## Building

Needs Rust (stable, MSVC), Visual Studio's C++ tools and Node.js. The first build downloads sherpa-onnx's prebuilt
static library from its GitHub release (or set `SHERPA_ONNX_ARCHIVE_DIR` to a folder that already holds
`sherpa-onnx-v2.0.1-win-x64-static-MT-Release-lib.tar.bz2`, SHA-256
`23e7b986b197eda0bbc47ff3d7d5c87afd6ef2b9077214ba26b0f1ec52ef0e66`). That library uses the static C runtime, so
`.cargo/config.toml` builds the whole program the same way.

```
cargo test
cd app\ui && npm install && npm test && npm run build
cd app && node ui\node_modules\@tauri-apps\cli\tauri.js build --no-bundle
```

The app is then `target\release\prism-transcribe.exe`. The heavy pack also needs whisper.cpp's `whisper-cli.exe`
(MIT), a separate program: `tools\build-whisper.bat` builds it, and the app looks for it in a `whisper` folder beside
itself, beside itself, in `PRISM_WHISPER`, or where Settings points. (A plain `cargo run` of the app expects the development server,
`npm run dev` in `app\ui`; Tauri's own build puts the interface inside the program.) The engine tests that run the real
models are skipped, with a message, when the models are not installed.

## The command line

`prism-transcribe-cli` does the app's work without the window, for scripts and tests:

```
prism-transcribe-cli models install
prism-transcribe-cli run interview.m4a --out interview.docx --speakers 2 --label INT-07
```

Also `models status|import-zip|export-zip` (add `heavy` for the heavy pack: `models install heavy`), `new`, `import`
(with `--pack heavy-1` or `--tracks channels|tracks`), `transcribe`, `export` (to .docx, .txt, .srt, .vtt, .csv,
.xlsx, .prismtx or .qdpx; several ids, or `all`, go into one table or package; `--speaker-names` for captions;
`--deidentified`, refused while a name from the key file's list is left), `check` (the final check) and `statement`
(the data handling statement); run it with no arguments for the list. A password-protected project asks for its password on standard input.

## Privacy

The program connects only when the user starts it: the speech-model download (fixed addresses, a SHA-256 check), and
Check for updates in Settings or Help, which asks github.com once for the newest version and installs it only after a
go-ahead and a signature check ([docs/UPDATES.md](docs/UPDATES.md)). Nothing is checked on its own, and the Offline
switch in Settings blocks both before any connection is made. There is no telemetry. Temporary audio lives in the
project's `temp` folder and is deleted when a job ends.

De-identification keeps its name map and change log in a key file (`.prismkey`) that the user saves outside the
project folder; the app remembers where it is in its own settings, not in the project. Password protection (Privacy,
off by default) seals each transcript and the key file with XChaCha20-Poly1305 under a key made from the password
with Argon2id; the password is stored nowhere, so a lost password means the transcripts cannot be opened. Recordings,
waveforms and the temporary audio of a running job are not sealed, which is why the data handling statement (Privacy,
"Save as Word") recommends full-disk encryption either way.

## Licence

Apache License 2.0 ([LICENSE](LICENSE)). The speech models have their own licences, listed in [NOTICE](NOTICE),
[docs/MODELS.md](docs/MODELS.md) and the app's Settings ("About and credits").
