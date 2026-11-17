# Phase 0 results: proving the speech stack

Run on 2026-11-17 on the build machine. Every figure here was measured, except where a row says "stand-in" or
"estimate". **The biggest limit:** every test recording is a computer voice reading a script (made by
`tools/make_fixtures.py`, so the true words, speakers and times are known). Real interviews will do worse, above all on
speakers; see "What phase 0 did not cover".

## The verdict, component by component

| Component | Spec's plan | Result | Go or change |
|---|---|---|---|
| Runtime | sherpa-onnx on ONNX Runtime, from Rust, no Python | Official `sherpa-onnx` 2.0.1 crate, linked statically; one small vendored patch to keep token probabilities | **Go** |
| Voice detection | Silero VAD | Works, but on these files barely splits at pauses between speakers (it called 58.7 of 59.8 minutes speech); nothing below relies on it for that | **Go, with tuning in phase 1** |
| Speech to text | Parakeet TDT 0.6B v3, int8 | 1.0 % word errors on the interview, word times within 0.06 s (median); sometimes silently skips the end of a chunk, now caught and re-heard | **Go, with the re-hearing guard** |
| Speaker segmentation | pyannote segmentation 3.0 | Inside sherpa-onnx's pipeline: 67 % of words to the right speaker on the interview, 21 % on the focus group | **Change: dropped** |
| Speaker embedding | 3D-Speaker CAM++ | 85 % on the focus group with the count given; 2 speakers found of 8 without it | **Change: WeSpeaker ResNet34-LM** (with a two-node fix to the model file) |
| Clustering | Our own agglomerative, cosine threshold, optional count | 99.5 % (interview) and 99.1 % (focus group, no count given, 8 of 8 found) | **Go** |
| Heavy pack, speech | Whisper large-v3-turbo on whisper.cpp | Model downloaded and checked; on the CPU about 28 minutes per hour; the graphics-card build could not be made here; a PyTorch stand-in on the same card did 60 minutes in 7.6 | **Go, provisionally** (whisper.cpp GPU build still to measure) |
| Heavy pack, speakers | pyannote community-1 | Both networks export to ONNX cleanly; its embedding is the same model as WeSpeaker's ungated one, which is what the light pack now uses; its clustering (VBx) is a port, not done | **Change: no rehosting needed** (see open decision 3 below) |

## Speed

Build machine: Intel Core i9-13900K (24 cores, 32 threads), 128 GB memory, NVIDIA RTX 3050 (8 GB), Windows 11.
Release build of `bench.exe`; the light pack as it now stands; 8 threads unless shown.

| Setup | Spec's target, 60-minute file | Measured | Verdict |
|---|---|---|---|
| Light pack, 8 threads (stands in for an 8-core desktop) | under 10 minutes | **9.1 min** for the interview and for the focus group (real-time factor 0.15) | **Meets it, narrowly.** A true 8-core desktop is slower per core than this one (see below) |
| Light pack, 4 threads on this computer | (stand-in for the laptop) | 9.7 min (earlier build, CAM++) | Not the target: this computer's cores are much faster than a laptop's |
| Light pack, 4-core laptop, no graphics card | under 20 minutes | **Not measured** | **Open.** TESTING-TODO item. Estimate: near the line |
| Heavy pack, graphics card with 6 GB or more | under 10 minutes | **7.6 min**, stand-in: Whisper large-v3-turbo in PyTorch, half precision, on the RTX 3050; 3.4 GB of graphics memory; no word times | Likely meets it; whisper.cpp's own GPU build is to measure |
| Heavy pack, CPU only | allowed, with a warning it may run longer than the recording | whisper.cpp v1.9.5 (Author's CPU build), 8 threads: 5 minutes in 140 s, about **28 min** per hour, recognition only | Shorter than the recording here; the warning still belongs on slower computers |

Where the light pack's time goes (60-minute interview, 8 threads): voice detection 36 s, recognition 378 s, speaker
labels 131 s, everything else under 2 s. Peak memory 2.0 GB (the whole recording is held in memory at 16 kHz: 230 MB
an hour).

Easy savings for phase 1, not yet taken: voice detection runs twice (once to cut chunks, once inside the speaker
labeller) and on one thread, so sharing it saves about 35 s an hour; recognition did not get faster from 4 to 8
threads in proportion (399 s to 339 s), so running two chunks at once may help more than more threads per chunk.

## Accuracy (60-minute files, final settings)

| | Interview (2 voices, count given) | Focus group (8 voices, **no** count given) |
|---|---|---|
| Length, turns | 59.8 min, 658 turns | 59.7 min, 936 turns |
| Word error rate | **0.96 %** (38 wrong, 77 missing, 2 extra, of 12,133 words) | **5.70 %** (158 wrong, 466 missing, 5 extra, of 11,042) |
| Speakers found | 2 of 2 | **8 of 8** |
| Words given to the right speaker | **99.5 %** | **99.1 %** |
| Word start times | median 0.056 s off; 100 % within 0.5 s | median 0.054 s off; 100 % within 0.5 s |
| Words below the 0.6 confidence tint | 0.2 % | 0.7 % |

The focus group's missing words are mostly whole short comments between other people's turns (261 words) and the
"Mm-hm"s spoken over someone else (73 words, which a one-speaker-per-word transcript cannot hold anyway). See the
recognizer problem below. Re-hearing shorter wordless stretches (0.8 s instead of 1.5 s) barely helped (5.58 %, and
about 100 s more an hour), so the remaining misses need another look in phase 1: shorter chunks once the voice
detector splits at pauses (problem 2), and checking where exactly the lost comments fall.

The diarization error rate (16 %) is almost all "false alarm": the speaker labeller gives a speaker to the pauses
that the voice detector did not split out. It does not touch the words or their speakers, and phase 1's pause tuning
should remove it.

The two one-minute fixtures, final settings, no count given: two-speaker 2 of 2 found, 99.4 % of words to the right
speaker; four-speaker 4 of 4, 98.8 %. The two-speaker clip's 8.5 % word error rate is all spelling, not hearing: the
script spells out "five five five, two one eight" and "eleventh", which the recognizer writes as "555 218" and "11th".

## How the speaker labelling was chosen

All on the same recognized words, so only the labelling differs. Share of words given to the right speaker:

| Labeller | Interview (count 2) | Focus group (count 8) | Focus group (no count) | Time per hour |
|---|---|---|---|---|
| sherpa-onnx pipeline: pyannote segmentation 3.0 + CAM++ (the spec's plan) | 67.2 % | 21.3 % | | 6.5 min |
| sherpa-onnx pipeline: community-1's segmentation + embedding | 99.9 % | 64.8 % | 46.3 % (85 found) | 9 min |
| Ours: pauses + CAM++ | 99.4 % | 85.1 % | 27.5 % (2 found) | 2.2 min |
| Ours: pauses + WeSpeaker ResNet34-LM as published | 99.4 % | 80.7 % | 14.1 % (1 found) | 2.3 min |
| Ours: pauses + community-1's embedding (exported here) | 99.5 % | 99.2 % | 99.1 % (8 found) | 2.3 min |
| **Ours: pauses + WeSpeaker ResNet34-LM with mean subtraction (chosen)** | **99.5 %** | **99.2 %** | **99.1 % (8 found)** | **2.3 min** |

"Ours" (`crates/speech`, `PauseLabeller` and `cluster.rs`): cut speech at the voice detector's pauses, embed windows
of up to 1.5 s (stepping 0.75 s), cluster by average linkage on cosine distance (threshold 0.6, or the user's count),
treat clusters smaller than 1 % of windows as noise and fold them in, refine against the cluster centres, and where
the label changes inside a stretch of speech, put the change at the quietest moment. Average linkage is computed with
the nearest-neighbour chain and checked against the textbook algorithm in the tests.

Why the published WeSpeaker file failed and the fixed one works: WeSpeaker models expect each utterance's feature mean
removed; sherpa-onnx 2.0.1 does not remove it for them (it does for 3D-Speaker's models). Given mean-removed features,
the published file and community-1's copy give identical embeddings (cosine 1.0000 on three test windows).
`tools/add_mean_subtraction.py` puts the subtraction inside the model (docs/MODELS.md).

## Problems found and what was done

1. **Parakeet skips the end of some chunks.** On the 60-minute interview, whole answers were missing: given 4:28.3 to
   4:51.1 it stopped at 4:41 and returned nothing for the last 9 seconds, while 4:28.0 to 4:52.0 came back complete.
   Fix (`pipeline::recognize_chunk`): any stretch of 1.5 s or more inside a chunk with no words is heard again on its
   own. Word error rate: interview 4.7 % to 1.0 %; focus group 10.5 % to 5.7 %. Re-hearing cost 13 s (interview) and 30 s (focus group) an hour.
2. **The voice detector hardly splits at the pauses between speakers** on these recordings (spans up to 42 s despite
   the 20 s cap). The pauses sit at -50 dBFS under speech at about -31 dBFS. Chunks are therefore long, which makes
   problem 1 likelier. Phase 1: tune it, or find pauses from the sound level as Author's "quiet gaps" method does.
3. **sherpa-onnx's Rust wrapper drops each token's probability**, which word confidence (T2) needs. Vendored the crate
   with two fields added (`vendor/PATCHES.md`).
4. **Two C runtimes in one program.** sherpa-onnx's prebuilt library uses the static C runtime; the whole program now
   does (`.cargo/config.toml`), which also means no Visual C++ redistributable to install.
5. **whisper.cpp would not build for the graphics card**: CUDA 12.8 refuses Visual Studio 2026, and with the override
   its headers fail against the new standard library. Needs the Vulkan SDK or a newer CUDA (TESTING-TODO).
6. **sherpa-onnx does not remove the feature mean for WeSpeaker models** (above). Worth reporting upstream.

## community-1

Downloaded with Jason's account on 2026-11-17 after he accepted pyannote's form. `tools/export_community1.py` exported
its segmentation network (largest difference from PyTorch 0.000027) and its embedding network (cosine 1.000000
against pyannote's own path) to ONNX, with sherpa-onnx's metadata so the adapter loads both. Its PLDA and VBx
clustering are arithmetic on two small arrays, not networks; they were not ported.

What it showed: the embedding is what matters, and it is the same model WeSpeaker publishes without any form. Inside
sherpa-onnx's pipeline (pyannote-style segmentation with fast clustering) the networks did well on two voices and badly
on eight. So for open decision 3 the recommendation changes: **do not rehost community-1**; the light pack already has
its embedding. Port VBx only if real focus groups show the pause labeller's clustering falling short.

## What phase 0 did not cover

- **Real people.** No real recording was tested. Synthetic voices are cleaner and more distinct than real ones; the
  speaker figures above are an upper bound. TESTING-TODO asks for one real interview.
- **A laptop.** The 4-core laptop target is unmeasured (TESTING-TODO).
- **A true 8-core desktop.** 8 threads of this computer's 24 cores stood in for one.
- **Languages other than English**, accents, poor microphones, phone or video-call audio, and long stretches of
  crosstalk.
- **whisper.cpp on the graphics card**, Whisper's word times, and Whisper with the voice detector in front of it (the
  stand-in's 10 % word error rate was without it, on 30-second windows, and is not comparable).
- **FFmpeg** for video (phase 1). Note for Jason: OpenStage Author's ADR 0002 says "OpenStage has a rule against
  shipping FFmpeg", while this spec plans an LGPL-only FFmpeg as a separate program. Worth settling before phase 1.

## Reproducing

```
cargo build --release -p prism-transcribe-bench
python -I tools/make_fixtures.py long --speakers 2 --minutes 60 --out D:/Prism-Transcribe-Bench/interview-60.wav
python -I tools/make_fixtures.py long --speakers 8 --minutes 60 --out D:/Prism-Transcribe-Bench/focus-60.wav
target\release\bench.exe D:\Prism-Transcribe-Bench\interview-60.wav --truth D:\Prism-Transcribe-Bench\interview-60.truth.json --speakers 2 --threads 8 --verify
target\release\bench.exe D:\Prism-Transcribe-Bench\focus-60.wav --truth D:\Prism-Transcribe-Bench\focus-60.truth.json --threads 8 --verify
```

(`make_fixtures.py` needs a Python with kokoro, numpy, scipy and soundfile; the seed is fixed, so the files are the
same each time.) `BENCH_WORDS=<file>` reuses recognized words between runs to compare speaker labellers only;
`--labeller pyannote`, `--emb-model` and `--seg-model` select the alternatives in the tables. The heavy-pack stand-in
is `tools/heavy_gpu_proxy.py`. Raw JSON reports from these runs are in `D:\Prism-Transcribe-Bench\` (not committed).
