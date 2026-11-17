# Testing to do (only a person can check these)

Things the build sessions could not check themselves. Newest phase first. When one is done, delete it and note the
result in the matching results file.

## Phase 3

- [ ] **Speakers, by hand.** In a transcript: open Speakers (Ctrl+Shift+P), name a speaker and give a role and an
  attribute (site, age band); merge two speakers; in a turn, press Alt+1 or Alt+2 to give it to a speaker, select
  some words and press Alt+2 to give just those; press Enter to split a turn and Backspace at a turn's start to join
  it to the one above; use each turn's speaker menu ("New speaker..."). Undo (Ctrl+Z) after each. The build session
  drove all of these through the debugging port on the two-speaker fixture and they worked; a person should judge
  whether they feel right.
- [ ] **Find the speakers again** on a real focus group where the first pass found the wrong number: set the count
  in Speakers and press Find speakers again. Your typed corrections should survive; names start again (Undo brings
  them back).
- [ ] **A real two-microphone recording.** Transcribe a recording with one microphone per person (a two-lavalier
  recorder writing left and right, or a file with one track per person) with Details > Advanced > "One channel per
  person". Check that nobody's words appear twice (each microphone hears the others; the app silences each one
  outside its own speaker) and that [crosstalk] marks real overlaps and not ordinary turn changes. Tested only on a
  made-up file (`tools/make_tracks_fixture.py`: two voices, the other voice 18 dB down, one planted overlap), where
  the result was right. Microphones whose levels differ a lot, and room echo, are the risk.
- [ ] **Separate audio tracks in one file** (Details > Advanced > "One audio track per person"). Built and unit tested
  for decoding, but no multi-track test file was at hand; an MKV or MP4 from a recorder that writes one track per
  person is needed. Files symphonia cannot read go through FFmpeg (`-map 0:a:N`).
- [ ] **The heavy pack on the graphics card, and its speed.** Phase 3 runs Whisper through a whisper-cli build for
  the processor (`tools/build-whisper.bat`, whisper.cpp v1.9.5). On this computer's processor it took 1.1 minutes for
  the one-minute fixture with the model loaded per batch; a 60-minute file was not timed. A Vulkan build
  (`tools/build-whisper.bat <src> <out> vulkan`, needs the Vulkan SDK) should be timed on `interview-60.wav`; the
  spec's target is under 10 minutes with a 6 GB card. Until then, Settings warns that the processor alone can take
  longer than the recording.
- [ ] **The heavy pack on another language.** Import a recording in a language the light pack does not cover (Details
  > Language, for example ja), Transcribe, and check that the heavy pack is used and the words are right. Not tested:
  no non-English recording was at hand. Whisper's word times are looser than Parakeet's (the editor says so).
- [ ] **Screen reader on the speakers panel and each turn's speaker menu (NVDA).**

## Phase 2

- [ ] **The phase's own test: correct a whole transcript with the keyboard only.** Open a transcribed recording (Edit),
  and without the mouse:
  - Play and pause with F4, skip with F3 and F5, and change speed with F6 and F7.
  - Move between turns with Ctrl+Up and Ctrl+Down, and type corrections.
  - Insert notation with Ctrl+K and timestamps with Ctrl+T.
  - Mark progress with Ctrl+M, save a snapshot with Ctrl+Shift+S, find with Ctrl+F, and save with Ctrl+S.

  The build session drove these keys through the debugging port on the four-person fixture. Playback, speed,
  rewind on resume, typing, notation, the checked marker, snapshot, find, save, undo and redo all worked, and the saved
  file kept word times in order. A person doing a real correction pass is the real test.
- [ ] **A foot pedal.** If you have a USB transcription pedal that types keys, set it up in Keys (press Change, then the
  pedal). Pedals that present as a game controller are a later phase.
- [ ] **Volume boost.** Choose 2× while a quiet recording plays: it should get louder. (Not checked by ear; the
  browser plays the recording through Web Audio once the boost is on.)
- [ ] **Screen reader in the editor (NVDA).** Each turn is announced as the speaker and time, and its text reads as
  plain text. Check that the playing-word highlight is not read out, and that the dialogs (Notation, Snapshots, Keys,
  Find) read sensibly.
- [ ] **Video in the editor.** An MP4 shows its picture above the waveform. A file the browser cannot play (WMA, AVI,
  MKV) plays from a sound-only copy, with a note saying so.
- [ ] **Spell check.** Words are underlined by Windows' own spell checker. The spec's *project word list* (so participants'
  pseudonyms stop being flagged) is **not built yet**: the browser's spell checker does not take a custom list. It
  needs a spell checker of our own, and is carried to the next phase.

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
