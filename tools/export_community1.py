"""Phase 0: export pyannote speaker-diarization-community-1's two networks to ONNX, and check them against PyTorch.

  python -I tools/export_community1.py <community-1 folder> <output folder> [<test.wav>]

The folder is the model repository as downloaded (it needs a Hugging Face account that accepted pyannote's terms;
never commit or ship the token). Needs pyannote.audio 4.0, torch, onnx and onnxruntime, in an environment of its own.

What it writes:
  segmentation.onnx  pyannote segmentation network, same input and output as sherpa-onnx's segmentation 3.0 export
                     (x: N x 1 x samples at 16 kHz; y: N x frames x 7 powerset classes), with sherpa-onnx's metadata,
                     so the speech adapter can load it in place of segmentation 3.0.
  embedding.onnx     WeSpeaker ResNet34-LM speaker embedding, taking 80-bin Kaldi fbank features (N x T x 80, from
                     samples scaled to 16-bit range) and giving N x 256, with sherpa-onnx's "wespeaker" metadata. The
                     per-utterance mean subtraction pyannote applies is inside the graph.

Not exported, because it is not a network: the PLDA and VBx clustering (plda/*.npz). That is arithmetic to port.
"""

import sys
import wave
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import torch
import torchaudio
from pyannote.audio import Model


def add_meta(filename, meta):
    m = onnx.load(filename)
    del m.metadata_props[:]
    for k, v in meta.items():
        p = m.metadata_props.add()
        p.key, p.value = k, str(v)
    onnx.save(m, filename)


def max_diff(a, b):
    return float(np.max(np.abs(np.asarray(a) - np.asarray(b))))


@torch.no_grad()
def main():
    src, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    test = sys.argv[3] if len(sys.argv) > 3 else None

    if test:
        # The standard library's reader: torchaudio.load needs FFmpeg's shared libraries, which are not here.
        with wave.open(test, "rb") as f:
            assert f.getframerate() == 16000 and f.getsampwidth() == 2 and f.getnchannels() == 1
            pcm = np.frombuffer(f.readframes(16000 * 10), dtype="<i2").astype(np.float32) / 32768.0
        wav = torch.from_numpy(pcm)[None, None, :]  # 1 x 1 x up to 10 s
    else:
        wav = torch.randn(1, 1, 16000 * 10) * 0.05

    # --- segmentation -------------------------------------------------------------------------------------
    seg = Model.from_pretrained(str(src / "segmentation" / "pytorch_model.bin"))
    seg.eval()
    spec = seg.specifications
    print("segmentation:", type(seg).__name__, "classes", spec.classes, "powerset_max", spec.powerset_max_classes,
          "duration", spec.duration, "dimension", seg.dimension)
    print("  receptive field step", seg.receptive_field.step, "duration", seg.receptive_field.duration)
    x = torch.zeros(1, 1, int(spec.duration * 16000))
    x[..., : wav.shape[-1]] = wav[..., : x.shape[-1]]
    seg_file = str(out / "segmentation.onnx")
    torch.onnx.export(seg, x, seg_file, opset_version=13, input_names=["x"], output_names=["y"],
                      dynamic_axes={"x": {0: "N", 2: "T"}, "y": {0: "N", 1: "T"}}, dynamo=False)
    add_meta(seg_file, {
        "num_speakers": len(spec.classes),
        "powerset_max_classes": spec.powerset_max_classes,
        "num_classes": seg.dimension,
        "sample_rate": 16000,
        "window_size": int(spec.duration * 16000),
        "receptive_field_size": int(round(seg.receptive_field.duration * 16000)),
        "receptive_field_shift": int(round(seg.receptive_field.step * 16000)),
        "model_type": "pyannote-segmentation-3.0",
        "version": "1",
        "model_author": "pyannote",
        "url": "https://huggingface.co/pyannote/speaker-diarization-community-1",
        "license": "CC-BY-4.0",
    })
    y_pt = seg(x).numpy()
    y_ox = ort.InferenceSession(seg_file).run(None, {"x": x.numpy()})[0]
    print("  exported", seg_file, "output", y_ox.shape, "max |onnx - torch|", max_diff(y_pt, y_ox))

    # --- embedding ----------------------------------------------------------------------------------------
    emb = Model.from_pretrained(str(src / "embedding" / "pytorch_model.bin"))
    emb.eval()
    print("embedding:", type(emb).__name__, "dimension", emb.dimension)
    resnet = emb.resnet

    class FbankToEmbedding(torch.nn.Module):
        def __init__(self, r):
            super().__init__()
            self.r = r

        def forward(self, feats):  # N x T x 80
            feats = feats - feats.mean(dim=1, keepdim=True)
            return self.r(feats)[1]

    wrap = FbankToEmbedding(resnet).eval()
    # Features exactly as pyannote's WeSpeakerResNet34 makes them (see its compute_fbank).
    w = wav[0] * (1 << 15)
    fb = torchaudio.compliance.kaldi.fbank(w, num_mel_bins=80, frame_length=25, frame_shift=10, dither=0.0,
                                           sample_frequency=16000, window_type="hamming", use_energy=False).unsqueeze(0)
    emb_file = str(out / "embedding.onnx")
    torch.onnx.export(wrap, fb, emb_file, opset_version=13, input_names=["feats"], output_names=["embs"],
                      dynamic_axes={"feats": {0: "N", 1: "T"}, "embs": {0: "N"}}, dynamo=False)
    dim = int(wrap(fb).shape[-1])
    add_meta(emb_file, {
        "framework": "wespeaker",
        "language": "English",
        "url": "https://huggingface.co/pyannote/speaker-diarization-community-1 (embedding: WeSpeaker ResNet34-LM)",
        "comment": "Exported by Prism Transcribe tools/export_community1.py; mean subtraction inside the graph",
        "sample_rate": 16000,
        "output_dim": dim,
        "normalize_samples": 0,
    })
    e_full = emb(wav).numpy()  # pyannote's own path, from waveform
    e_ox = ort.InferenceSession(emb_file).run(None, {"feats": fb.numpy()})[0]
    cos = float(np.dot(e_full[0], e_ox[0]) / (np.linalg.norm(e_full[0]) * np.linalg.norm(e_ox[0])))
    print("  exported", emb_file, "dim", dim, "max |onnx - pyannote|", max_diff(e_full, e_ox), "cosine", round(cos, 6))

    for f in (seg_file, emb_file):
        print(f, Path(f).stat().st_size, "bytes")


if __name__ == "__main__":
    main()
