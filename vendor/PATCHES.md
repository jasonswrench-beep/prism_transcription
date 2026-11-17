# Vendored crates and what was changed

## sherpa-onnx 2.0.1 (Apache-2.0, k2-fsa/sherpa-onnx)

Copied unchanged from crates.io, apart from one struct:

- `src/offline_asr.rs`, `OfflineRecognizerResult`: two fields added, `ys_log_probs` and `lang`, both
  `#[serde(default)]`. The C API's JSON result already carries them (`SherpaOnnxGetOfflineStreamResultAsJson`);
  the upstream wrapper drops them when it parses. `ys_log_probs` is one log-probability per token for
  transducer models such as Parakeet, which the speech adapter turns into a confidence score per word (T2).

Wired in through `[patch.crates-io]` in the workspace `Cargo.toml`. When upstream adds these fields, delete this
folder and the patch line. Worth offering upstream as a small pull request; that is Jason's call, not done.
