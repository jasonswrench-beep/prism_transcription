# Testing to do (only a person can check these)

Things the build sessions could not check themselves. Newest phase first. When one is done, delete it and note the
result in the matching results file.

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
