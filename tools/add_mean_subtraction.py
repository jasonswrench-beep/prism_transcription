"""Add per-utterance mean subtraction to the front of a WeSpeaker embedding model (ONNX), for sherpa-onnx.

  python -I tools/add_mean_subtraction.py <in.onnx> <out.onnx>

Why: WeSpeaker models expect their fbank features with each utterance's mean removed. sherpa-onnx 2.0.1's speaker
embedding extractor gives "wespeaker" models the features without removing it, and their embeddings then barely
tell voices apart (phase 0: the focus group fell from 99 % of words to the right speaker to 81 %, and to one speaker
found when no count was given). Putting the subtraction inside the graph fixes that without changing sherpa-onnx.
Doing it twice is harmless (the mean of features whose mean was removed is zero).

The output is an adaptation of the input model: under CC BY 4.0 it may be shared with credit and a note that it was
changed (docs/MODELS.md records both).
"""

import sys

import onnx
from onnx import helper, numpy_helper
import numpy as np


def main():
    src, dst = sys.argv[1], sys.argv[2]
    m = onnx.load(src)
    g = m.graph
    feats = g.input[0].name
    opset = max(o.version for o in m.opset_import if o.domain in ("", "ai.onnx"))
    raw = feats + "_before_mean_subtraction"
    # Every node that read the input now reads the centred features.
    for n in g.node:
        for i, name in enumerate(n.input):
            if name == feats:
                n.input[i] = "centred_feats"
    g.input[0].name = raw
    if opset >= 18:
        axes = numpy_helper.from_array(np.array([1], dtype=np.int64), name="mean_axes")
        g.initializer.append(axes)
        mean = helper.make_node("ReduceMean", [raw, "mean_axes"], ["feats_mean"], keepdims=1)
    else:
        mean = helper.make_node("ReduceMean", [raw], ["feats_mean"], axes=[1], keepdims=1)
    sub = helper.make_node("Sub", [raw, "feats_mean"], ["centred_feats"])
    g.node.insert(0, sub)
    g.node.insert(0, mean)
    # Keep the input's public name, which sherpa-onnx looks up.
    g.input[0].name = feats
    for n in g.node[:2]:
        for i, name in enumerate(n.input):
            if name == raw:
                n.input[i] = feats
    meta = {p.key: p.value for p in m.metadata_props}
    meta["comment"] = (meta.get("comment", "") + " | Prism Transcribe: per-utterance mean subtraction added at the input "
                       "(tools/add_mean_subtraction.py)").strip(" |")
    del m.metadata_props[:]
    for k, v in meta.items():
        p = m.metadata_props.add()
        p.key, p.value = k, v
    onnx.checker.check_model(m)
    onnx.save(m, dst)
    print("wrote", dst, "opset", opset)


if __name__ == "__main__":
    main()
