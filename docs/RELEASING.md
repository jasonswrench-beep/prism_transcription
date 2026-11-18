# Releasing Prism Transcribe through Prism Hub

The first release is 0.1.0-alpha.1. With a pre-release suffix it is shown in Prism Hub as a preview; a plain version
(0.1.0) is shown as stable. Everything below that leaves this repository waits for Jason's go-ahead.

## In this repository

1. **Licences.** `python tools/third_party.py --check` must pass. It flags symphonia (the audio decoder, MPL-2.0,
   used unchanged) and names the MPL-2.0 crates Tauri's window needs (cssparser, cssparser-macros, dtoa-short,
   option-ext, selectors). Prism Survey accepted the window's five by name; for Prism Transcribe both groups need
   Jason's word, then they go in `EXCEPTIONS` in the script and `python tools/third_party.py` writes THIRD-PARTY.md.
2. **Commit and tag.** Commit everything, then tag that commit `v0.1.0-alpha.1` (the version in Cargo.toml,
   tauri.conf.json and app/ui/package.json). A pushed tag never moves; a rebuilt release takes a new version.
3. **Build.** `app\BUILD-INSTALLER.ps1 -Sign -Whisper <whisper-cli folder>` on Jason's computer, after `az login`
   and with `OPENSTAGE_SIGN_TOOLS` and the `OPENSTAGE_SIGN_*` settings set (the suite's signing scripts; the same
   Azure Artifact Signing account as PrismQDA). It refuses an untagged or dirty tree, a tag that does not match the
   version, and an unaccepted licence. The result is `target\release\bundle\nsis\Prism Transcribe_0.1.0-alpha.1_x64-
   setup.exe` with its `.sha256.txt` and `.source.txt`. `-Whisper` puts whisper.cpp's whisper-cli (built by
   `tools\build-whisper.bat`) inside, signed, so the heavy pack needs no setup.
4. **Try it.** Install on a computer without the development tools (TESTING-TODO, phase 5).

## On GitHub

5. **A public repository with a release.** Prism Hub downloads only from a public repository's releases. The
   repository (`jasonswrench-beep/prism_transcription`) is private today. Publish a release for the tag with the setup
   and its `.sha256.txt`, titled "Prism Transcribe 0.1.0 alpha 1", the body in plain words with a "Download" list
   (each file's name, size and SHA-256) and a note that it is an alpha, as PrismQDA's betas do.

## In Prism Hub (C:\Dev\OpenStage-Hub, under its own CLAUDE.md)

6. **The catalog entry**, in `brands\prism\apps.json` under `apps`, with an icon in `brands\prism\icons\` (an SVG of at
   most 32 KB; `assets\icon.svg` here, checked for size):

   ```json
   {
     "id": "prism-transcribe",
     "name": "Prism Transcribe",
     "mark": { "letters": "tr", "color": "#4B3FD8", "icon": "icons/prism-transcribe.svg" },
     "description": "Transcribe interviews and focus groups on your own computer and send them to PrismQDA",
     "repository": "jasonswrench-beep/prism_transcription",
     "installedAs": "Prism Transcribe",
     "programFile": "prism-transcribe.exe",
     "quietSwitch": ["/P"],
     "expectedSigner": "Jason Wrench",
     "setupPattern": "^Prism Transcribe_.*_x64-setup\\.exe$",
     "comingSoonUntilReleased": true
   }
   ```

   The description is the spec's ("How it appears in Prism Hub"). `comingSoonUntilReleased` lists it under Coming
   soon until the repository is public and has a release. Then `Update Prism Catalog.bat` signs and publishes the
   catalog (it needs `gh` signed in and the catalog key's password).

## In PrismQDA (C:\Dev\prismqda, under its own CLAUDE.md)

7. **The matching development item** (the spec's phase 5 note), for `TODO.md` under the development list:

   > - [ ] **7. Read Prism Transcribe's .prismtx.** Import a `.prismtx` package (docs/PRISMTX.md in
   >   prism_transcription) as transcript sources: one `speaker_turn` unit per turn with `time_start_ms` and
   >   `time_end_ms`, interviewer turns `excluded`, one case per speaker holding that speaker's turns with its
   >   attributes, and the recording linked by path or, failing that, by fingerprint. Refuse an unknown major version.
   >   Open `.prismtx` from the command line and register it as a file type, so Prism Transcribe's "Open in PrismQDA"
   >   can hand it over. Until then, Prism Transcribe's `.qdpx` export imports with the same turns, times and cases
   >   (checked against `import_qdpx` on 2026-11-17).

   A request for the suite's core to own the format, as PSTAT-1 and the family exchange are, would go in
   PrismQDA's `CORE-REQUESTS.md` as R7.

## After the release

- "Open in PrismQDA" today opens PrismQDA (found at `%LOCALAPPDATA%\PrismQDA`) and says to import the `.qdpx`; once
  PrismQDA reads `.prismtx` from the command line, it can open the package directly.
- Without PrismQDA, the app opens Prism Hub. Hub has no "install this app" link yet; that would be a Hub feature.
