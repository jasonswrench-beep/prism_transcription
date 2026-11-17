# Requests to the shared core (openstage-core)

What Prism Transcribe needs from the core, in the form other suite tools use. The core's notes are
`C:\Dev\openstage-core\CORE-NOTES.md`; nothing here is built in the core yet.

## T-C5-1: lift C5, the speech engine, with a three-job contract

**Asked 2026-11-17, phase 0.** Read against the core at commit `27315b9` (CORE-NOTES updated 2026-11-16) and
OpenStage Author at `f207df2`.

**What the core has today.** C5 is not lifted. It is on the core's ROADMAP under "OpenStage Recorder: captions"
("C5, the speech engine (built inside OpenStage Author today), lifted out so the tools share one install of the
speech models"). In Author, C5 is ADR 0002: `whisper-cli.exe` (whisper.cpp v1.9.5, commit `d1be6fd`, built by
`BUILD SPEECH RECOGNISER.bat`) run as a separate program; English-only models `ggml-small.en` and `ggml-base.en`
in `%LOCALAPPDATA%\com.openstageauthor.editor\speech-models`; the contract is whisper-cli's command line and its
`-ojf` JSON. Author has no voice detection or speaker labelling.

**What Transcribe needs.** Three jobs behind one contract, which Transcribe has written as Rust traits in
`crates/speech/src/lib.rs` so it can move into the core unchanged:

1. **Detect voice**: 16 kHz mono samples in, speech spans (start, end in seconds) out.
2. **Recognize speech**: samples and their offset in the recording in, words out, each with text, start, end and a
   confidence from 0 to 1; plus the list of languages the model covers.
3. **Label speakers**: samples and an optional expected speaker count in, spans with anonymous speaker numbers out.

**Plugins.** Transcribe's first plugin is sherpa-onnx (Apache-2.0, linked statically, CPU): Silero VAD, Parakeet TDT
0.6B v3 for 25 European languages, and WeSpeaker ResNet34-LM embeddings with Transcribe's own clustering for speakers
(phase 0 measured this well ahead of sherpa-onnx's pyannote pipeline and of CAM++; see docs/PHASE0-RESULTS.md). One
thing for whoever owns C5: sherpa-onnx 2.0.1 does not remove the feature mean for "wespeaker" embedding models, which
ruins them; Transcribe adds the subtraction to the model file (`tools/add_mean_subtraction.py`). Worth reporting
upstream. Author's engine
runs on whisper.cpp, not sherpa-onnx, so by the spec's open decision 2 it joins C5 as a **second plugin**
(recognize speech only), and the heavy pack's Whisper large-v3-turbo uses the same whisper.cpp plugin. One contract,
two engines, models installed once.

**One shared model folder.** Proposed: `%LOCALAPPDATA%\OpenStage\shared-models\speech\`, one sub-folder or file
per model exactly as its source archive unpacks, so a model downloaded by any tool serves every tool. Transcribe
uses this path now (overridable with `PRISM_SPEECH_MODELS`). C8 (the AI runtime) is expected to define the
equivalent for language models; the two should sit side by side. Author's existing folder would move here, or Author
would look in both during a changeover.

**A pack manifest.** Each pack is a list of files with path, size and SHA-256, plus licence and source
(`crates/speech/src/models.rs`, and `docs/MODELS.md` for the record). The core would own one manifest format so
Transcribe, Author, Recorder, Studio and Scroll check models the same way.

**What Transcribe will do until the core has it.** Keep C5 as `crates/speech` in this repository with no
Transcribe-specific types in it, and move it when the core opens the C5 round.

## T-C5-2: a speech test set

**Asked 2026-11-17.** Like C3 (item analysis), a small shared test set of recordings with known words, speakers and
times, so every tool's speech features are held to the same figures. Transcribe's `tools/make_fixtures.py` makes
synthetic ones (Kokoro voices with the truth written alongside); real recordings with consent and a transcript are
the part only a person can supply (see TESTING-TODO.md).
