"""Phase 0 stand-in for the heavy pack on a graphics card: Whisper large-v3-turbo through PyTorch and Hugging Face
transformers, half precision, on CUDA.

This is NOT the heavy pack the app will ship (that is whisper.cpp with Vulkan or CUDA). whisper.cpp's CUDA build
could not be made on the build machine (CUDA 12.8 refuses Visual Studio 2026's compiler; see docs/PHASE0-RESULTS.md),
so this measures the same model on the same card by another runtime, to see whether the "under 10 minutes for a
60-minute file on a 6 GB card" target is in reach. whisper.cpp on CUDA is usually as fast or faster.

  python -I tools/heavy_gpu_proxy.py <recording.wav> [--truth file.truth.json] [--minutes N] [--batch 8] [--words]

Uses only the copy of openai/whisper-large-v3-turbo already in the Hugging Face cache (HF_HUB_OFFLINE=1): it never
downloads anything.
"""

import argparse
import json
import os
import re
import sys
import time

os.environ["HF_HUB_OFFLINE"] = "1"

import numpy as np
import soundfile as sf
import torch
from transformers import AutoModelForSpeechSeq2Seq, AutoProcessor, pipeline


def norm(s):
    s = s.lower()
    s = re.sub(r"[-—–/]", " ", s)
    out = []
    for w in s.split():
        w = "".join(c for c in w if c.isalnum() or c == "'").strip("'")
        if w:
            out.append(w)
    return out


def wer(ref, hyp):
    n, m = len(ref), len(hyp)
    prev = list(range(m + 1))
    for i in range(1, n + 1):
        cur = [i] + [0] * m
        for j in range(1, m + 1):
            cur[j] = min(prev[j - 1] + (ref[i - 1] != hyp[j - 1]), prev[j] + 1, cur[j - 1] + 1)
        prev = cur
    return prev[m] / max(1, n)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wav")
    ap.add_argument("--truth")
    ap.add_argument("--minutes", type=float)
    ap.add_argument("--batch", type=int, default=8)
    ap.add_argument("--words", action="store_true", help="ask for word timestamps (slower)")
    a = ap.parse_args()

    audio, sr = sf.read(a.wav, dtype="float32")
    if audio.ndim > 1:
        audio = audio.mean(axis=1)
    if a.minutes:
        audio = audio[: int(a.minutes * 60 * sr)]
    dur = len(audio) / sr

    t0 = time.time()
    model_id = "openai/whisper-large-v3-turbo"
    model = AutoModelForSpeechSeq2Seq.from_pretrained(model_id, torch_dtype=torch.float16, low_cpu_mem_usage=True).to("cuda:0")
    proc = AutoProcessor.from_pretrained(model_id)
    asr = pipeline("automatic-speech-recognition", model=model, tokenizer=proc.tokenizer, feature_extractor=proc.feature_extractor,
                   torch_dtype=torch.float16, device="cuda:0", chunk_length_s=30, batch_size=a.batch)
    load = time.time() - t0
    torch.cuda.reset_peak_memory_stats()
    t1 = time.time()
    out = asr({"raw": audio, "sampling_rate": sr}, return_timestamps="word" if a.words else True,
              generate_kwargs={"language": "en", "task": "transcribe"})
    run = time.time() - t1
    peak = torch.cuda.max_memory_allocated() / 2**20

    report = {"file": a.wav, "audio_minutes": round(dur / 60, 2), "gpu": torch.cuda.get_device_name(0), "batch": a.batch,
              "word_timestamps": a.words, "seconds_load": round(load, 1), "seconds_transcribe": round(run, 1),
              "real_time_factor": round(run / dur, 4), "gpu_peak_mb": round(peak)}
    if a.truth:
        t = json.load(open(a.truth, encoding="utf-8"))
        turns = sorted(t["turns"], key=lambda x: x["start"])
        ref = [w for x in turns if x["start"] < dur for w in norm(x["text"])]
        report["wer"] = round(wer(ref, norm(out["text"])), 4)
    print(json.dumps(report, indent=1))


if __name__ == "__main__":
    main()
