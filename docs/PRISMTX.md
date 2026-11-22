# The Prism transcript package (.prismtx), version 1.0

What Prism Transcribe hands to PrismQDA: one or more transcripts with their speakers (as cases), the speakers'
attributes (as case attributes), and a link to each recording, so a coded passage can play its stretch of the
recording. The code that writes it is `crates/core/src/package.rs`; this page is the contract a reader builds on.

A `.prismtx` file is a zip. Every text file in it is UTF-8 JSON (or WebVTT), with `/` in paths.

```text
manifest.json
<id>/transcript.json      one folder per recording, named by its id in the project (F001, F002, ...)
<id>/document.json
<id>/transcript.vtt
```

## manifest.json

```json
{
  "format": "prism-transcribe-package",
  "version": "1.0",
  "producer": { "app": "Prism Transcribe", "version": "0.1.0" },
  "created": "2026-11-17T19:30:00.123Z",
  "deidentified": true,
  "documents": [
    { "id": "F001", "label": "INT-07", "transcript": "F001/transcript.json", "document": "F001/document.json", "captions": "F001/transcript.vtt" }
  ]
}
```

- `format` is always `prism-transcribe-package`. A reader refuses a `version` whose major number it does not know;
  new fields within a major version may be added and must be ignored.
- `deidentified` is true only when every document passed the final check against the project's name list (T6) at
  export. The name list and the change log are never in the package.

## <id>/transcript.json

The transcript as Prism Transcribe's editor writes it (`crates/core/src/model.rs`, `schema_version` 1), with
`snapshots` always empty: snapshots keep the words from before de-identification, so they never leave the project.

- `speakers[]`: `id` ("S1"), `label` ("Speaker 1"), `name` (the name the user gave, usually a pseudonym; shown
  instead of the label when set), `role` (`interviewer`, `moderator`, `participant`, `other`), `attributes`
  (name to text).
- `turns[]`: `speaker`, `start`, `end` (seconds from the start of the recording, as floating point), and `words[]`,
  each with `text`, `start`, `end`, `confidence` (0 to 1; 1 for typed words) and `speaker`. A turn's text is its
  words joined by single spaces. Notation such as `[laughter]` or `[inaudible 00:12:03]` is one word.
- `review`: `not_started`, `first_pass`, `checked`, `de_identified`, `final`.
- `provenance`: the pack and engine that made it.

## <id>/document.json

What a QDA program needs beside the words, in its terms.

```json
{
  "label": "INT-07",
  "source_type": "transcript",
  "date": "2026-03-04",
  "language": "en",
  "media": {
    "kind": "audio",
    "file_name": "int 07.wav",
    "path": "C:\\Research\\Recordings\\int 07.wav",
    "size_bytes": 3145728,
    "duration_ms": 61500,
    "sha256": "…",
    "fingerprint": "…"
  },
  "cases": [
    { "speaker": "S1", "name": "Interviewer", "role": "interviewer", "interviewer": true,
      "attributes": [ { "name": "role", "level": "nominal", "value": "interviewer" } ] },
    { "speaker": "S2", "name": "P07", "role": "participant", "interviewer": false,
      "attributes": [ { "name": "role", "level": "nominal", "value": "participant" },
                      { "name": "site", "level": "nominal", "value": "North" } ] }
  ],
  "turns_ms": [ ["S1", 1000, 1900], ["S2", 2500, 3400] ]
}
```

- `media.path` is the recording's absolute path when it was packaged; the recording is linked, not copied. A
  package marked `deidentified` has no `path`, and its `file_name` is the recording's id and file ending
  (`F001.wav`), because a file or folder name often holds a participant's or a site's name; the reader finds the
  recording by `fingerprint` or asks where it is. (The `.qdpx` export does the same: a de-identified one links
  `relative://F001.wav`.)
  `sha256` is the whole file's, taken at import. `fingerprint` is PrismQDA's media fingerprint: SHA-256 of the file
  size as 8 bytes little-endian, then the first MiB, then the last MiB (the whole file when it is smaller), so the
  recording can be found again after it moves. `kind` is `audio` or `video`.
- `cases`: one per speaker who has at least one turn. In PrismQDA, each becomes a case whose members are that
  speaker's turns (`case_member` with a unit); `interviewer` true maps to the turns being left out of coding
  (`unit.excluded`). Attribute levels are PrismQDA's (`nominal` for now: every value is text).
- `turns_ms`: the turns of transcript.json in order, in integer milliseconds, for readers that store times that way
  (PrismQDA's `unit.time_start_ms` and `time_end_ms`).

## <id>/transcript.vtt

The same turns as WebVTT, one cue group per turn, each cue starting with a voice tag (`<v P07>`). It is there for
readers that know only captions: PrismQDA already imports it beside a recording of the same name.

## How PrismQDA should read it (the matching development item)

1. Refuse an unknown `format` or major `version`, naming the file.
2. For each document: a source of type `transcript`, its text the turns joined by blank lines (NFC), one
   `speaker_turn` unit per turn with its speaker and `time_start_ms`/`time_end_ms`, and the speaker's turns
   `excluded` when the case says `interviewer`.
3. Link the recording by `media.path`; if it is not there (or not given), look for a file with the same
   `fingerprint`, and else ask the user where it is.
4. One case per entry of `cases`, holding that speaker's turns, with its attributes.
5. Say on import whether the package was marked `deidentified`.

Until PrismQDA reads `.prismtx`, the REFI-QDA project export (`.qdpx`) opens there with the same turns, timings,
cases and attributes, and with click-to-play.
