"""Make a two-microphone test recording from fixtures/two-speaker.wav and its truth file.

    python -I tools/make_tracks_fixture.py fixtures/two-speaker.wav OUT.wav

Each speaker's turns go on their own channel (left: S1, right: S2), and each microphone also hears the other voice
about 18 dB down, as a real second microphone in the same room would ("bleed"). One extra overlap is added: S2's
"Sure." is copied onto the right channel in the middle of S1's question at 25 s, so the transcript should show
[crosstalk] there. Writes OUT.wav (16 kHz, 2 channels) and OUT.truth.json (the overlap's time).

Only numpy and the standard library; the venv used for the other fixtures works.
"""

import json
import sys
import wave
from pathlib import Path

import numpy as np

BLEED = 0.125  # about -18 dB


def main(src: str, out: str) -> None:
    truth = json.loads(Path(src[: -len(".wav")] + ".truth.json").read_text(encoding="utf-8"))
    with wave.open(src) as w:
        sr = w.getframerate()
        x = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768.0
    left = np.zeros_like(x)
    right = np.zeros_like(x)
    for t in truth["turns"]:
        a, b = int(t["start"] * sr), int(t["end"] * sr)
        seg = x[a:b]
        own, other = (left, right) if t["speaker"] == "S1" else (right, left)
        own[a:b] += seg
        other[a:b] += seg * BLEED
    # A deliberate overlap: S2's first word ("Sure.") laid over S1's question at 25.5 s.
    s2 = next(t for t in truth["turns"] if t["speaker"] == "S2")
    sure = s2["words"][0]
    a, b = int(sure["start"] * sr), int((sure["end"] + 0.15) * sr)
    at = int(25.5 * sr)
    right[at : at + (b - a)] += x[a:b]
    left[at : at + (b - a)] += x[a:b] * BLEED
    noise = np.random.default_rng(7).normal(0, 0.0015, size=(2, len(x))).astype(np.float32)
    st = np.stack([left, right]) + noise
    pcm = (np.clip(st.T, -1, 1) * 32767).astype(np.int16)
    with wave.open(out, "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(pcm.tobytes())
    Path(out[: -len(".wav")] + ".truth.json").write_text(
        json.dumps({"made_from": src, "left": "S1", "right": "S2", "overlap": {"speaker": "S2", "text": sure["text"], "start": 25.5, "end": 25.5 + (b - a) / sr}}, indent=2),
        encoding="utf-8",
    )
    print(f"wrote {out}: {len(x) / sr:.1f} s, 2 channels")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
