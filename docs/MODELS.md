# Speech models

Every model file Prism Transcribe uses: where it came from, its licence (the exact text is in
[licences/](licences/)), its size and SHA-256. Read and downloaded on 2026-11-17. Models are never committed and are
never inside the installer; they live in the shared speech-model folder
`%LOCALAPPDATA%\OpenStage\shared-models\speech\` (proposed for C5 in [CORE-REQUESTS.md](../CORE-REQUESTS.md)).

The phase 0 rule: stop on any model whose licence does not allow redistribution in a free app. **None had to be
stopped.** Every model is MIT, Apache 2.0 or CC BY 4.0, all of which allow redistribution (CC BY 4.0 with credit, and
with a note of any change).

## Light pack (CPU, every machine): `light-1`

About 0.70 GB unpacked (0.51 GB to download). The files the program checks are listed in
`crates/speech/src/models.rs`, and where each comes from in `crates/speech/src/install.rs`; `bench --verify` and
`prism-transcribe-cli models status` check every size and SHA-256. Tried from an empty folder on 2026-11-17: the
whole pack downloaded, unpacked and checked in 45 s; an offline zip of it (698 MB) imported and checked.

| Job | Model | Original source and licence | File downloaded |
|---|---|---|---|
| Voice detection | Silero VAD | github.com/snakers4/silero-vad, **MIT**, (c) 2020-present Silero Team ([text](licences/silero-vad-MIT.txt)) | sherpa-onnx `asr-models/silero_vad.onnx` |
| Speech to text | Parakeet TDT 0.6B v3, int8 | huggingface.co/nvidia/parakeet-tdt-0.6b-v3, **CC BY 4.0** ("GOVERNING TERMS: Use of this model is governed by the CC-BY-4.0 license"; not gated) ([legal code](licences/CC-BY-4.0-legalcode.txt)). Credit NVIDIA in the About box and the docs. | sherpa-onnx `asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2` (its card also says CC BY 4.0) |
| Speaker embedding | WeSpeaker ResNet34-LM, English (VoxCeleb) | github.com/wenet-e2e/wespeaker; huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM, **CC BY 4.0** (WeSpeaker's pretrained models follow their training data's licence, VoxCeleb's CC BY 4.0; not gated). The same weights as community-1's embedding (checked: identical embeddings). Credit WeSpeaker and VoxCeleb. | sherpa-onnx `speaker-recongition-models/wespeaker_en_voxceleb_resnet34_LM.onnx` (the tag is spelt that way), **then changed** (below) |
| Speaker clustering | Agglomerative, average linkage on cosine distance, outliers folded in, centroid refinement | Our own code (`crates/speech/src/cluster.rs`), Apache 2.0 | none |
| Runtime | sherpa-onnx 2.0.1 on ONNX Runtime | github.com/k2-fsa/sherpa-onnx, **Apache 2.0** ([text](licences/sherpa-onnx-Apache-2.0.txt)); ONNX Runtime **MIT** | Linked into the program (see below), not a model download |

**The change to the embedding model.** WeSpeaker models expect each utterance's mean removed from their input
features. sherpa-onnx 2.0.1 does not do that for "wespeaker" models unless the model's metadata says so, and the
published file then hardly tells voices apart (60-minute focus group: 81 % of words to the right speaker, and 1
speaker found of 8 with no count given). The installer downloads the published file, checks its SHA-256, and appends
one metadata entry, `feature_normalize_type` = `global-mean` (39 bytes: protobuf merges a field that appears again at
the end of a message, so nothing is parsed or rewritten; `install::onnx_metadata_entry`). sherpa-onnx then removes the
mean itself, and the fixtures give the same result as phase 0's graph-edited model (four-speaker clip: 4 of 4 found,
98.8 %). The result is checked against its own SHA-256 below. Because the change happens on the user's computer,
nothing changed has to be hosted; CC BY 4.0 allows the change in any case, with the note in NOTICE.
(`tools/add_mean_subtraction.py`, the graph edit phase 0 used, stays for reference.)

**Replaced in phase 0** (downloaded and measured, not in the pack): 3D-Speaker CAM++ English
(`3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx`, Apache 2.0, [text](licences/3D-Speaker-Apache-2.0.txt)), which
found 2 speakers of 8 with no count given and put 85 % of words with the right speaker even with the count; and
pyannote segmentation 3.0 (`sherpa-onnx-pyannote-segmentation-3-0.tar.bz2`, MIT, (c) 2022 CNRS,
[text](licences/pyannote-segmentation-3.0-MIT.txt); its original is behind a contact form that adds no licence terms,
and sherpa-onnx's copy is not gated), used by sherpa-onnx's own diarization pipeline, which phase 0 found much weaker
on focus groups ([PHASE0-RESULTS.md](PHASE0-RESULTS.md)). Both remain usable for comparison through
`bench --labeller pyannote` and `--emb-model`.

All files come from `https://github.com/k2-fsa/sherpa-onnx/releases/download/<tag>/<file>`.

### Downloads and their SHA-256

| Download | Bytes | SHA-256 | Matches GitHub's digest |
|---|---:|---|---|
| `silero_vad.onnx` | 643,854 | `9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6` | yes |
| `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2` | 487,211,443 | `1495e91b59d7fc53ddca91f3431758ec99f8fa253fdb0e1e851be0f72b14b0f9` | yes |
| `wespeaker_en_voxceleb_resnet34_LM.onnx` | 26,530,550 | `e9848563da86f263117134dfd7ad63c92355b37de492b55e325400c9d9c39012` | GitHub publishes none (uploaded 2024) |
| `sherpa-onnx-pyannote-segmentation-3-0.tar.bz2` (replaced) | 6,958,444 | `24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488` | GitHub publishes none |
| `3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx` (replaced) | 29,596,978 | `357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b` | GitHub publishes none |

### Files the program uses, as unpacked

| Path under the model folder | Bytes | SHA-256 |
|---|---:|---|
| `silero_vad.onnx` | 643,854 | `9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6` |
| `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/encoder.int8.onnx` | 652,282,294 | `1afbe69c1ffdabee94755f42e4daf1d93549131a3da11356bda84fcef0f2c479` |
| `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/decoder.int8.onnx` | 11,845,274 | `0222a86147299d4dee90edf95b6d13827edb42ad8b7c9cf0b2582f030937c0b5` |
| `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/joiner.int8.onnx` | 6,355,277 | `cbd02f3696e9944970111fb439d9c3fd365a80374565f13f2b9c7007d253fc98` |
| `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/tokens.txt` | 93,939 | `d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d` |
| `wespeaker_en_voxceleb_resnet34_LM.global-mean.onnx` (the download plus the 39-byte entry, above) | 26,530,589 | `ce2c5db3fb2c438f1d58de35577c951bc27ea0d41ef74968de4a480a1a59e39b` |

The Parakeet archive also holds a README and four short test recordings; the program does not use them.

### The speech runtime built into the program

sherpa-onnx's Rust crate downloads its prebuilt static library at build time:
`v2.0.1/sherpa-onnx-v2.0.1-win-x64-static-MT-Release-lib.tar.bz2`, 122,863,596 bytes, SHA-256
`23e7b986b197eda0bbc47ff3d7d5c87afd6ef2b9077214ba26b0f1ec52ef0e66` (matches GitHub's digest). For release builds,
keep a checked copy and point `SHERPA_ONNX_ARCHIVE_DIR` at it so the build never fetches it.

## Heavy pack (graphics card recommended)

| Job | Model | Licence | State |
|---|---|---|---|
| Speech to text | Whisper large-v3-turbo for whisper.cpp, `ggml-large-v3-turbo.bin`, 1,624,555,275 bytes, SHA-256 `1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69` (matches huggingface.co/ggerganov/whisper.cpp) | whisper.cpp **MIT** ([text](licences/whisper.cpp-MIT.txt)); OpenAI's Whisper code and weights **MIT** ([text](licences/openai-whisper-MIT.txt)). The Hugging Face card of the original says MIT. | Downloaded and checked. Smaller quantised versions exist (q8_0 874 MB, q5_0 574 MB) and would bring the pack nearer 1 GB. |
| Diarization | pyannote speaker-diarization-community-1 | **CC BY 4.0** for the pipeline, behind a contact-details form ("...released under the CC-BY-4.0 license and will always remain freely accessible. By providing your details, you agree that we may email you occasionally..."). Its parts: the embedding is WeSpeaker ResNet34-LM, which its README says follows the VoxCeleb data's **CC BY 4.0**; the PLDA was trained by BUT Speech@FIT, no separate licence stated. | Downloaded 2026-11-17 with Jason's account after he accepted the form; both networks exported to ONNX (below). |

### community-1 and ONNX

Downloaded on 2026-11-17 into `D:\Prism-Transcribe-Bench\community-1` (outside the repository) with a read token Jason
created after accepting the form. The token was used for that download only and is not stored anywhere; the app must
never ship one. No ONNX conversion was published by sherpa-onnx or anyone else on Hugging Face (searched the same day).
Its `config.yaml` names pyannote.audio 4.0.0, `SpeakerDiarization` with `VBxClustering` (threshold 0.6, Fa 0.07,
Fb 0.8) and embeddings that leave out overlapped speech.

| Part | File (bytes, SHA-256) | ONNX |
|---|---|---|
| Segmentation | `segmentation/pytorch_model.bin` (5,906,507, `7ad24338d844fb95985486eb1a464e32d229f6d7a03c9abe60f978bacf3f816e`): PyanNet, 3 speakers, powerset of up to 2 at once (7 classes), 10 s windows; the same shape as segmentation 3.0 | **Exported cleanly** by `tools/export_community1.py` (pyannote.audio 4.0.7, torch 2.15 CPU, opset 13): `segmentation.onnx`, 5,916,458 bytes, SHA-256 `6bfc1ff0dbb40ec7757945fbad32487da148e9b132cb8ed306dd465e779f976f`. Largest difference from PyTorch on a 10 s fixture: 0.000027. Carries sherpa-onnx's segmentation metadata, and sherpa-onnx loads it in place of 3.0. |
| Embedding | `embedding/pytorch_model.bin` (26,646,242, `6f10ff60898a1d185fa22e1d11e0bfa8a92efec811f11bca48cb8cafebefd929`): WeSpeaker ResNet34, 256 dimensions | **Exported cleanly**, taking 80-bin Kaldi fbank features with the mean subtraction inside the graph: `embedding.onnx`, 26,542,412 bytes, SHA-256 `5dd3d107c42b5f7db80df94c7a314b531f93a271c0e502bea81b34035dbe776e`. Against pyannote's own waveform path: largest difference 0.0000014, cosine 1.000000. Carries sherpa-onnx's "wespeaker" metadata, and sherpa-onnx's embedding extractor loads it. |
| Clustering | `plda/plda.npz` (133,852, `9b77bcd8...d255`), `plda/xvec_transform.npz` (134,376, `325f1ce8...be8f`) | **Not a network, so nothing to export.** PLDA scoring and VBx clustering are arithmetic on these arrays (pyannote's Python). To use them, port VBx to Rust (a few hundred lines of linear algebra) and check it against pyannote's output on the same embeddings. Phase 0 did not port it. |

So community-1 "exports to ONNX cleanly" for both networks, and the networks alone already improve speaker labels a
great deal in phase 0's tests (docs/PHASE0-RESULTS.md); the VBx clustering, the other half of what makes it better on
meetings, is a port, not an export.

### Also seen, not used

sherpa-onnx also publishes `sherpa-onnx-reverb-diarization-v1` and `v2` (Rev's fine-tuned segmentation). The
originals (huggingface.co/Revai/reverb-diarization-v1, v2) are gated and give their licence only as "other"; the
text was not read. Read it before considering them: a non-commercial licence would rule them out.
