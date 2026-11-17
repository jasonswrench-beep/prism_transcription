# Prism Transcribe

Prism Transcribe turns interview and focus-group recordings into speaker-labelled, timestamped transcripts on the
researcher's own Windows computer, and sends them to PrismQDA. Nothing leaves the machine: the audio, the transcript
and every file in between stay local, and the speech models run on the computer itself.

**Status: phase 1 (a file in, a speaker-labelled Word file out).** The app can create or open a project, import
recordings, download and check its speech models (or import them from an offline zip), transcribe one file at a time
with progress and a resumable job, show the transcript (read-only, unsure words tinted), and export it to Word or
plain text. Editing is phase 2. There is no installer yet. The plan is in the specification (six phases,
requirements T1 to T8); phase 0's measurements are in [docs/PHASE0-RESULTS.md](docs/PHASE0-RESULTS.md). What only a
person can check is in [TESTING-TODO.md](TESTING-TODO.md).

## Layout

| Folder | What it holds |
|---|---|
| `crates/core` | The transcript data model (JSON, schema version 1), projects (one folder each) and the Word and text exports. |
| `crates/media` | Decoding: WAV, MP3, M4A, FLAC and OGG in Rust; video and other formats through an FFmpeg program if one is found. Writes the 16 kHz mono copy the speech engine reads. |
| `crates/speech` | The speech engine adapter (C5): detect voice, recognize speech, label speakers, behind one small contract, with sherpa-onnx as the first plugin; the resumable transcription job; the model manager (download with SHA-256 checks, offline zip). Written to move into the shared core unchanged (see [CORE-REQUESTS.md](CORE-REQUESTS.md)). |
| `crates/engine` | Import, transcribe and export joined up, for the app and for `prism-transcribe-cli`. |
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

The app is then `target\release\prism-transcribe.exe`. (A plain `cargo run` of the app expects the development server,
`npm run dev` in `app\ui`; Tauri's own build puts the interface inside the program.) The engine tests that run the real
models are skipped, with a message, when the models are not installed.

## The command line

`prism-transcribe-cli` does the app's work without the window, for scripts and tests:

```
prism-transcribe-cli models install
prism-transcribe-cli run interview.m4a --out interview.docx --speakers 2 --label INT-07
```

Also `models status|import-zip|export-zip`, `new`, `import`, `transcribe` and `export`; run it with no arguments for
the list.

## Privacy

The program's only network call is the speech-model download, which happens only when the user starts it, from fixed
addresses, with a SHA-256 check; the Offline switch in Settings blocks it before any connection is made. There is no
telemetry. Temporary audio lives in the project's `temp` folder and is deleted when a job ends.

## Licence

Apache License 2.0 ([LICENSE](LICENSE)). The speech models have their own licences, listed in [NOTICE](NOTICE),
[docs/MODELS.md](docs/MODELS.md) and the app's Settings ("About and credits").
