# Testing to do (only a person can check these)

Things the build sessions could not check themselves. Newest phase first. When one is done, delete it and note the
result in the matching results file.

## Phase 1

Build the app first (README, "Building"); it is `target\release\prism-transcribe.exe`.

- [ ] **The phase's own test: a 60-minute interview to Word with the network unplugged.** Unplug the cable or turn
  off Wi-Fi, start the app, New project, Import a real 60-minute interview (Expected speakers: 2), Transcribe, then
  Export to Word. The build session did this with the command line (`prism-transcribe-cli run`, 8.4 minutes, Word
  opened the result) but with the network connected; the transcription code has no network code in it, which is
  what this checks for real.
- [ ] **The window by hand.** The session drove the window through its debugging port (project, import, Transcribe
  button, progress, transcript, export), so the native parts were not tried: the folder and file pickers, the Save
  dialogs, Cancel during a transcription and then Transcribe again (it should carry on, not start over), and closing
  the app mid-transcription then transcribing again (the same).
- [ ] **Model download in the window.** On a computer without the models (or after renaming
  `%LOCALAPPDATA%\OpenStage\shared-models\speech`): the Speech models panel, Download (about 514 MB), the progress bar,
  and Import from an offline zip (`prism-transcribe-cli models export-zip light-pack.zip` makes one). With Settings,
  Offline on, Download should be greyed and the status bar should show the padlock.
- [ ] **Keyboard and screen reader.** Tab through the window and every dialog, and listen with NVDA: buttons, the
  progress bar's text, the transcript as plain text. Phase 2 makes the editor fully keyboard-driven; this is the
  first look.
- [ ] **A video file.** Import an MP4 or MKV. This computer has an FFmpeg (a GPL build from winget) on the PATH, which
  the app found. Whether the app ships an LGPL FFmpeg is still open (the FFmpeg item below).
- [ ] **Push.** Phase 1 is committed locally; it is pushed only when you say so.

## Phase 0

- [ ] **Run bench.exe on a laptop with no graphics card** (the spec's 4-core laptop target: a 60-minute file in
  under 20 minutes, speakers included). Copy `target\release\bench.exe` and the light pack folder
  (`%LOCALAPPDATA%\OpenStage\shared-models\speech\`: the files listed in docs/MODELS.md, about 0.7 GB) to the laptop,
  into the same place, plus `D:\Prism-Transcribe-Bench\interview-60.wav` and its `.truth.json`, then run:
  `bench.exe interview-60.wav --truth interview-60.truth.json --speakers 2 --verify --json laptop.json`.
  Send back `laptop.json`. The build machine's 4-thread run is only a stand-in: its cores are much faster.
- [ ] **A real recording.** Every phase 0 figure comes from computer voices reading a script. Run bench.exe on one
  real interview or focus group you have consent to use (WAV, 16-bit, any length) with
  `--transcript out.txt` (add `--speakers N` if you know the count). Without a truth file it reports speed only; read
  the transcript and judge the words and the speaker turns. Phase 0's speaker results are an upper bound.
- [ ] **The light pack's licences, read by a person.** docs/MODELS.md records each model's licence and the texts are in
  docs/licences/. Two are CC BY 4.0 and need credit in the About box and the docs: Parakeet (NVIDIA) and the WeSpeaker
  ResNet34-LM embedding (WeSpeaker, VoxCeleb). The embedding file is changed by us (mean subtraction added); CC BY 4.0
  allows that with a note of the change. Decide whether phase 1 hosts the changed file or changes it on the computer
  after download (docs/MODELS.md, "The change to the embedding model").
- [ ] **FFmpeg.** OpenStage Author's ADR 0002 says "OpenStage has a rule against shipping FFmpeg"; this spec plans an
  LGPL-only FFmpeg as a separate program for video and unusual audio. Settle which holds before phase 1 builds import.
- [ ] **Graphics-card build of whisper.cpp.** CUDA 12.8 on this computer refuses Visual Studio 2026's compiler, so
  the heavy pack's whisper.cpp was not built for the GPU. Either install the Vulkan SDK (whisper.cpp's Vulkan build
  works with any recent compiler) or a CUDA release that supports Visual Studio 2026; then a build session can run
  `tools\build-whisper-cuda.bat` (or a Vulkan equivalent) and time it on `interview-60.wav`.
- [ ] **Look at the app shell.** `cargo build -p prism-transcribe-app`, then run `target\debug\prism-transcribe.exe`:
  a window titled Prism Transcribe with the waveform icon should open and say whether the light pack is installed.
