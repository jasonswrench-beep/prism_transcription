# Prism Transcribe

Prism Transcribe turns interview and focus-group recordings into speaker-labelled, timestamped transcripts on the
researcher's own Windows computer, and sends them to PrismQDA. Nothing leaves the machine: the audio, the transcript
and every file in between stay local, and the speech models run on the computer itself.

**Status: phase 0 (proving the speech stack).** There is no app to use yet. What exists is the repository skeleton,
the speech engine adapter, a command-line benchmark (`bench.exe`) and the results of running it. The plan is in the
specification (six phases, requirements T1 to T8); what phase 0 found is in
[docs/PHASE0-RESULTS.md](docs/PHASE0-RESULTS.md). What only a person can check is in
[TESTING-TODO.md](TESTING-TODO.md).

## Layout

| Folder | What it holds |
|---|---|
| `crates/core` | The transcript data model, projects and export (phase 1 onward). |
| `crates/speech` | The speech engine adapter (C5): three jobs, detect voice, recognize speech, label speakers, behind one small contract, with sherpa-onnx as the first plugin. Written to move into the shared core unchanged (see [CORE-REQUESTS.md](CORE-REQUESTS.md)). |
| `crates/bench` | `bench.exe`: runs the light pack on a recording and prints wall time, real-time factor, peak memory and, with a truth file, accuracy. |
| `app` | The desktop shell (Tauri). In phase 0 it opens a window and reports whether the speech models are installed. |
| `fixtures` | Two one-minute test recordings with their truth files (who spoke, when, what). Made with computer voices by `tools/make_fixtures.py`; the names, email and phone number in them are invented. |
| `tools` | Scripts used to make test audio and to measure the heavy pack. Not part of the app. |
| `vendor` | One vendored crate with a one-field change ([vendor/PATCHES.md](vendor/PATCHES.md)). |
| `assets` | The product icon (`icon.svg`); the app's icon sizes in `app/src-tauri/icons` are made from it with `tauri icon`. |
| `docs` | [MODELS.md](docs/MODELS.md) (every model file, its source, licence and SHA-256) and the phase 0 results. |

## Building

Needs Rust (stable, MSVC) and Visual Studio's C++ tools. The first build downloads sherpa-onnx's prebuilt static
library from its GitHub release (or set `SHERPA_ONNX_ARCHIVE_DIR` to a folder that already holds
`sherpa-onnx-v2.0.1-win-x64-static-MT-Release-lib.tar.bz2`, SHA-256
`23e7b986b197eda0bbc47ff3d7d5c87afd6ef2b9077214ba26b0f1ec52ef0e66`). That library uses the static C runtime, so
`.cargo/config.toml` builds the whole program the same way.

```
cargo test
cargo build --release -p prism-transcribe-bench
```

The models are not in the repository. Phase 1 adds the in-app model manager; until then, unpack the light pack into
`%LOCALAPPDATA%\OpenStage\shared-models\speech\` as listed in [docs/MODELS.md](docs/MODELS.md), and
`bench --verify` checks every file's size and SHA-256.

```
target\release\bench.exe fixtures\two-speaker.wav --truth fixtures\two-speaker.truth.json --speakers 2
```

## Privacy

The program makes no network calls. Model downloads (phase 1) happen only when the user starts one, from fixed
addresses, with a SHA-256 check. There is no telemetry.

## Licence

Apache License 2.0 ([LICENSE](LICENSE)). The speech models have their own licences, listed in [NOTICE](NOTICE) and
[docs/MODELS.md](docs/MODELS.md).
