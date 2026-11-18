# Updates: the Check for updates button

Prism Transcribe makes no network connection of its own except the ones a person starts: downloading the speech models,
and **Settings > Updates > Check for updates** (also in Help). This page says what the button does, why it can be
trusted, and what the maintainer does so a release is offered by it. It follows PrismQDA's design (its decision 189),
chosen for Prism Transcribe on 2026-11-18.

## What a person sees

- Nothing is checked at start, in the background, or on a schedule.
- Pressing **Check for updates** asks one address (the update feed, `latest.json` on github.com) whether a newer
  version has been published. The request carries what every web request carries (the program's version and the kind
  of computer) and nothing from any project.
- If there is a newer version, the window shows its number and its notes. **Download and install** is a second step,
  with a confirmation. The editor saves, the installer is downloaded, **its signature is checked against the key built
  into the program**, and the installer runs for the current Windows account (no administrator rights), replacing the
  program, which opens again. A file whose signature does not match is refused and nothing is installed.
- Nothing is checked while **Offline** is on, and nothing is installed while a transcription or a model download runs
  (the installer closes the program).
- A build made before the update key was set up says so ("not set up") and cannot connect to anything. 0.1.0-alpha.1
  is such a build: it is updated through Prism Hub or a new setup by hand.

## How it is built

- `app/src-tauri/src/updates.rs` uses Tauri's updater plugin, but only when the window's own commands ask
  (`update_state`, `update_check`, `update_install`). The address (`ENDPOINT`) and the public key (`PUBKEY`) are
  constants there; while `PUBKEY` is empty the updater is not set up. The check has a 20-second timeout; a failure is
  shown in words and changes nothing.
- The update signature is a separate key from the code-signing certificate: the Azure signature tells Windows who made
  the setup; the update signature tells Prism Transcribe that this file is the maintainer's. It is made **after** the
  code signature, over the exact bytes people download.
- Tests (`cargo test -p prism-transcribe-app`), with a feed on this computer: a build with no key never connects;
  Offline and a running job are refused before any connection; a newer version is announced with its notes and an
  older one is not; a missing feed fails in words; a download passes only when it matches its signature (a changed
  byte, or a signature by another key, is refused). The test key and signed file are PrismQDA's fixtures.

## Making the key (once)

```
powershell -ExecutionPolicy Bypass -File tools\make-update-key.ps1
```

It asks for a password (twice), makes `prism-transcribe-update.key` and `.pub` in `PrismTranscribe-update-key` in your
profile folder, outside the repository, and puts the public half into `PUBKEY` in `updates.rs`; commit that change.
**Back up the folder and the password.** If either is lost, installed programs can never be updated by the button again
(a new setup would have to be installed by hand once, carrying a new key). If someone else gets both, make a new key at
once and publish a setup built with it.

## Making a release that the button offers

1. Build with the key, from the tagged commit: set `TAURI_SIGNING_PRIVATE_KEY_PATH` to the `.key` file and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to its password, then
   `app\BUILD-INSTALLER.ps1 -Sign -Updates -Whisper <folder> -NotesFile <notes.txt>`. It stops if the key is missing,
   or if `PUBKEY` is empty or is not the key's public half. It signs the upload copy of the setup in
   `target\release-assets\` with the update key and writes `latest.json` there, pointing at
   `https://github.com/jasonswrench-beep/prism_transcription/releases/download/v<version>/<setup>`.
2. Publish the release `v<version>` with the setup and its `.sha256.txt` (docs/RELEASING.md).
3. Publish the feed: the release `update-feed` (a pre-release made once, titled "Update feed: not a download") holds
   one asset, `latest.json`. Replace it:
   `gh release upload update-feed target\release-assets\latest.json --clobber --repo jasonswrench-beep/prism_transcription`.
4. Check from a computer with an older installed version: Check for updates, read the notes, install, see the version.

**Versions** are compared as semantic versions: `0.1.0-alpha.1` is older than `0.1.0-alpha.2`, which is older than
`0.1.0`. Every release needs a higher number (Cargo.toml, tauri.conf.json and app/ui/package.json), or it is not offered.

**Withdrawing a release:** put the previous `latest.json` back in the feed. People who installed the bad version
install the earlier setup by hand.

## What it does not do

No automatic checks, no downloads without a go-ahead, no telemetry, no account, and no check while Offline is on.
