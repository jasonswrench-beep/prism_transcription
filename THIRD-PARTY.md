# Third-party components

Everything built into Prism Transcribe that others wrote, each with its license as its authors publish it. Where a component offers a choice, Prism Transcribe uses it under the MIT or Apache 2.0 terms. Test-only and build-tool components that do not ship are not listed. This file is written by `python tools/third_party.py`, which also checks that every license is permissive. The speech models are listed in NOTICE and docs/MODELS.md.

One exception to the permissive rule, by name: the crates Tauri's window needs (cssparser, cssparser-macros, dtoa-short, option-ext, selectors) and the symphonia audio decoder (symphonia, symphonia-bundle-flac, symphonia-bundle-mp3, symphonia-codec-aac, symphonia-codec-adpcm, symphonia-codec-pcm, symphonia-codec-vorbis, symphonia-common, symphonia-core, symphonia-format-isomp4, symphonia-format-ogg, symphonia-format-riff, symphonia-metadata) are under MPL-2.0, which asks only that changes to those files be shared. Prism Transcribe uses them unchanged; their source is at crates.io.

sherpa-onnx and ONNX Runtime are linked into the program (Apache 2.0 and MIT; see NOTICE). The de-identification suggestions use the spell checker built into Windows, called through the `windows` crate; nothing is bundled for it. The window is drawn by the web view built into Windows; the installer fetches it only if the computer lacks it.

## Rust crates in the engine and command-line tool

| Crate | Version | License |
|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| aead | 0.5.2 | MIT OR Apache-2.0 |
| aes | 0.9.4 | MIT OR Apache-2.0 |
| aho-corasick | 1.1.5 | Unlicense OR MIT |
| argon2 | 0.5.3 | MIT OR Apache-2.0 |
| audio-codec-algorithms | 0.8.1 | 0BSD OR Apache-2.0 |
| audioadapter | 5.0.0 | MIT OR Apache-2.0 |
| audioadapter-buffers | 5.2.0 | MIT OR Apache-2.0 |
| audioadapter-sample | 5.2.0 | MIT OR Apache-2.0 |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| base64 | 0.23.1 | MIT OR Apache-2.0 |
| base64ct | 1.8.3 | Apache-2.0 OR MIT |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| blake2 | 0.10.6 | MIT OR Apache-2.0 |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 |
| bumpalo | 3.20.4 | MIT OR Apache-2.0 |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| bytes | 1.12.1 | MIT |
| bzip2 | 0.4.4 | MIT/Apache-2.0 |
| bzip2 | 0.6.1 | MIT OR Apache-2.0 |
| bzip2-sys | 0.1.13+1.0.8 | MIT/Apache-2.0 |
| cc | 1.11.2 | MIT OR Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| chacha20 | 0.9.1 | Apache-2.0 OR MIT |
| chacha20poly1305 | 0.10.1 | Apache-2.0 OR MIT |
| chrono | 0.4.45 | MIT OR Apache-2.0 |
| cipher | 0.4.4 | MIT OR Apache-2.0 |
| cipher | 0.5.2 | MIT OR Apache-2.0 |
| cmov | 0.5.4 | Apache-2.0 OR MIT |
| const-oid | 0.10.3 | Apache-2.0 OR MIT |
| constant_time_eq | 0.6.1 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| cpubits | 0.1.1 | MIT OR Apache-2.0 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 |
| crc32fast | 1.5.5 | MIT OR Apache-2.0 |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 |
| ctutils | 0.4.8 | Apache-2.0 OR MIT |
| deflate64 | 0.1.12 | MIT |
| deranged | 0.5.9 | MIT OR Apache-2.0 |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| digest | 0.11.3 | MIT OR Apache-2.0 |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 |
| equivalent | 1.0.3 | Apache-2.0 OR MIT |
| extended | 0.1.0 | MIT |
| filetime | 0.2.30 | MIT/Apache-2.0 |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 |
| flate2 | 1.1.11 | MIT OR Apache-2.0 |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 |
| generic-array | 0.14.7 | MIT |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| hmac | 0.13.0 | MIT OR Apache-2.0 |
| hound | 3.5.1 | Apache-2.0 |
| http | 1.5.0 | MIT OR Apache-2.0 |
| httparse | 1.10.1 | MIT OR Apache-2.0 |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 |
| icu_collections | 2.4.0 | Unicode-3.0 |
| icu_locale_core | 2.4.0 | Unicode-3.0 |
| icu_normalizer | 2.4.0 | Unicode-3.0 |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 |
| icu_properties | 2.4.0 | Unicode-3.0 |
| icu_properties_data | 2.4.0 | Unicode-3.0 |
| icu_provider | 2.4.0 | Unicode-3.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| inout | 0.1.4 | MIT OR Apache-2.0 |
| inout | 0.2.6 | MIT OR Apache-2.0 |
| itoa | 1.0.19 | MIT OR Apache-2.0 |
| jobserver | 0.1.35 | MIT OR Apache-2.0 |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 |
| libbz2-rs-sys | 0.2.6 | bzip2-1.0.6 |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| litemap | 0.8.3 | Unicode-3.0 |
| log | 0.4.35 | MIT OR Apache-2.0 |
| lzma-rust2 | 0.25.0 | Apache-2.0 |
| memchr | 2.8.3 | Unlicense OR MIT |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| num-complex | 0.4.6 | MIT OR Apache-2.0 |
| num-conv | 0.2.2 | MIT OR Apache-2.0 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| opaque-debug | 0.3.1 | MIT OR Apache-2.0 |
| password-hash | 0.5.0 | MIT OR Apache-2.0 |
| pbkdf2 | 0.13.0 | MIT OR Apache-2.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| poly1305 | 0.8.0 | Apache-2.0 OR MIT |
| potential_utf | 0.1.6 | Unicode-3.0 |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 |
| ppmd-rust | 1.5.2 | CC0-1.0 OR MIT-0 |
| primal-check | 0.3.4 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| rand_core | 0.6.4 | MIT OR Apache-2.0 |
| realfft | 3.5.1 | MIT |
| regex | 1.13.1 | MIT OR Apache-2.0 |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 |
| regex-lite | 0.1.9 | MIT OR Apache-2.0 |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 |
| ring | 0.17.14 | Apache-2.0 AND ISC |
| rubato | 5.1.0 | MIT OR Apache-2.0 |
| rustfft | 6.4.1 | MIT OR Apache-2.0 |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT |
| rustls-pki-types | 1.16.0 | MIT OR Apache-2.0 |
| rustls-webpki | 0.103.17 | ISC |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| sha1 | 0.11.0 | MIT OR Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| sha2 | 0.11.1 | MIT OR Apache-2.0 |
| sherpa-onnx | 2.0.1 | Apache-2.0 |
| sherpa-onnx-sys | 2.0.1 | Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| simd-adler32 | 0.3.10 | MIT |
| smallvec | 1.16.5 | MIT OR Apache-2.0 |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| strength_reduce | 0.2.4 | MIT OR Apache-2.0 |
| subtle | 2.6.1 | BSD-3-Clause |
| symphonia | 0.6.1 | MPL-2.0 |
| symphonia-bundle-flac | 0.6.1 | MPL-2.0 |
| symphonia-bundle-mp3 | 0.6.1 | MPL-2.0 |
| symphonia-codec-aac | 0.6.1 | MPL-2.0 |
| symphonia-codec-adpcm | 0.6.1 | MPL-2.0 |
| symphonia-codec-pcm | 0.6.1 | MPL-2.0 |
| symphonia-codec-vorbis | 0.6.1 | MPL-2.0 |
| symphonia-common | 0.6.1 | MPL-2.0 |
| symphonia-core | 0.6.1 | MPL-2.0 |
| symphonia-format-isomp4 | 0.6.1 | MPL-2.0 |
| symphonia-format-ogg | 0.6.1 | MPL-2.0 |
| symphonia-format-riff | 0.6.1 | MPL-2.0 |
| symphonia-metadata | 0.6.1 | MPL-2.0 |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.10 | MIT OR Apache-2.0 |
| synstructure | 0.14.0 | MIT |
| tar | 0.4.46 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| time | 0.3.55 | MIT OR Apache-2.0 |
| time-core | 0.1.9 | MIT OR Apache-2.0 |
| tinystr | 0.8.5 | Unicode-3.0 |
| transpose | 0.2.3 | MIT OR Apache-2.0 |
| typed-path | 0.13.0 | MIT OR Apache-2.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| universal-hash | 0.5.1 | MIT OR Apache-2.0 |
| untrusted | 0.9.0 | ISC |
| ureq | 2.12.1 | MIT OR Apache-2.0 |
| ureq | 3.5.2 | MIT OR Apache-2.0 |
| ureq-proto | 0.7.1 | MIT OR Apache-2.0 |
| url | 2.5.8 | MIT OR Apache-2.0 |
| utf8-zero | 0.8.1 | MIT OR Apache-2.0 |
| utf8_iter | 1.1.0 | Apache-2.0 OR MIT |
| utf_types | 1.0.0 | Apache-2.0 OR MIT |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| visibility | 0.1.1 | Zlib OR MIT OR Apache-2.0 |
| webpki-roots | 0.26.11 | CDLA-Permissive-2.0 |
| webpki-roots | 1.0.12 | CDLA-Permissive-2.0 |
| windowfunctions | 0.1.1 | MIT |
| windows | 0.62.2 | MIT OR Apache-2.0 |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 |
| windows-future | 0.3.2 | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 |
| writeable | 0.6.4 | Unicode-3.0 |
| yoke | 0.8.4 | Unicode-3.0 |
| yoke-derive | 0.8.4 | Unicode-3.0 |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 |
| zeroize | 1.9.1 | Apache-2.0 OR MIT |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT |
| zerotrie | 0.2.5 | Unicode-3.0 |
| zerovec | 0.11.9 | Unicode-3.0 |
| zerovec-derive | 0.11.7 | Unicode-3.0 |
| zip | 9.6.1 | MIT |
| zlib-rs | 0.6.8 | Zlib |
| zmij | 1.0.23 | MIT |
| zopfli | 0.8.4 | Apache-2.0 |
| zstd | 0.14.4 | BSD-3-Clause |
| zstd-safe | 8.3.0 | BSD-3-Clause |
| zstd-sys | 2.1.3+zstd.1.5.7 | BSD-3-Clause AND BSD-2-Clause |

## Rust crates the desktop window adds

| Crate | Version | License |
|---|---|---|
| alloc-no-stdlib | 3.0.0 | BSD-3-Clause |
| alloc-stdlib | 0.3.0 | BSD-3-Clause |
| anyhow | 1.0.104 | MIT OR Apache-2.0 |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT |
| bit-set | 0.8.0 | Apache-2.0 OR MIT |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT |
| brotli | 9.1.3 | BSD-3-Clause AND MIT |
| brotli-decompressor | 6.3.2 | BSD-3-Clause/MIT |
| byteorder | 1.5.0 | Unlicense OR MIT |
| camino | 1.2.6 | MIT OR Apache-2.0 |
| cargo-platform | 0.1.9 | MIT OR Apache-2.0 |
| cargo_metadata | 0.19.2 | MIT |
| cargo_toml | 1.0.1 | Apache-2.0 OR MIT |
| cfb | 0.14.0 | MIT |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 |
| cookie | 0.18.2 | MIT OR Apache-2.0 |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 |
| cssparser | 0.37.0 | MPL-2.0 |
| cssparser-macros | 0.7.1 | MPL-2.0 |
| ctor | 1.0.14 | Apache-2.0 OR MIT |
| darling | 0.24.1 | MIT |
| darling_core | 0.24.1 | MIT |
| darling_macro | 0.24.1 | MIT |
| derive_more | 2.1.1 | MIT |
| derive_more-impl | 2.1.1 | MIT |
| dirs | 7.0.0 | MIT OR Apache-2.0 |
| dirs-sys | 0.5.0 | MIT OR Apache-2.0 |
| dom_query | 0.28.0 | MIT |
| dpi | 0.1.2 | Apache-2.0 AND MIT |
| dtoa | 1.0.11 | MIT OR Apache-2.0 |
| dtoa-short | 0.3.5 | MPL-2.0 |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 |
| embed-resource | 3.0.12 | MIT |
| erased-serde | 0.4.11 | MIT OR Apache-2.0 |
| fastrand | 2.5.0 | Apache-2.0 OR MIT |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| fnv | 1.0.7 | Apache-2.0 / MIT |
| foldhash | 0.2.0 | Zlib |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| glob | 0.3.4 | MIT OR Apache-2.0 |
| hashbrown | 0.12.3 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| html5ever | 0.39.0 | MIT OR Apache-2.0 |
| http-body | 1.1.0 | MIT |
| http-body-util | 0.1.5 | MIT |
| hyper | 1.12.0 | MIT |
| hyper-tls | 0.6.0 | MIT/Apache-2.0 |
| hyper-util | 0.1.21 | MIT |
| ico | 0.6.0 | MIT |
| ident_case | 1.0.1 | MIT/Apache-2.0 |
| indexmap | 1.9.3 | Apache-2.0 OR MIT |
| infer | 0.22.0 | MIT |
| ipnet | 2.12.2 | MIT OR Apache-2.0 |
| json-patch | 4.2.0 | MIT/Apache-2.0 |
| jsonptr | 0.7.1 | MIT OR Apache-2.0 |
| keyboard-types | 0.8.3 | MIT OR Apache-2.0 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| markup5ever | 0.39.0 | MIT OR Apache-2.0 |
| mime | 0.3.18 | MIT OR Apache-2.0 |
| minisign-verify | 0.3.0 | MIT |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| mio | 1.2.5 | MIT |
| muda | 0.20.0 | Apache-2.0 OR MIT |
| native-tls | 0.2.18 | MIT OR Apache-2.0 |
| new_debug_unreachable | 1.0.6 | MIT |
| notify-rust | 4.19.0 | MIT OR Apache-2.0 |
| option-ext | 0.2.0 | MPL-2.0 |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| phf | 0.13.1 | MIT |
| phf_codegen | 0.13.1 | MIT |
| phf_generator | 0.13.1 | MIT |
| phf_macros | 0.13.1 | MIT |
| phf_shared | 0.13.1 | MIT |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| plist | 1.10.4 | MIT |
| png | 0.18.1 | MIT OR Apache-2.0 |
| precomputed-hash | 0.1.1 | MIT |
| quick-xml | 0.42.0 | MIT |
| rand | 0.10.4 | MIT OR Apache-2.0 |
| rand_core | 0.10.1 | MIT OR Apache-2.0 |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib |
| reqwest | 0.13.5 | MIT OR Apache-2.0 |
| rfd | 0.17.2 | MIT |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 |
| same-file | 1.0.6 | Unlicense/MIT |
| schannel | 0.1.30 | MIT |
| schemars | 0.8.22 | MIT |
| schemars_derive | 0.8.22 | MIT |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| selectors | 0.38.0 | MPL-2.0 |
| semver | 1.0.28 | MIT OR Apache-2.0 |
| serde-untagged | 0.1.9 | MIT OR Apache-2.0 |
| serde_derive_internals | 0.29.1 | MIT OR Apache-2.0 |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 |
| serde_spanned | 1.1.3 | MIT OR Apache-2.0 |
| serde_with | 3.24.0 | MIT OR Apache-2.0 |
| serde_with_macros | 3.24.0 | MIT OR Apache-2.0 |
| serialize-to-javascript | 0.1.2 | MIT OR Apache-2.0 |
| serialize-to-javascript-impl | 0.1.2 | MIT OR Apache-2.0 |
| servo_arc | 0.4.3 | MIT OR Apache-2.0 |
| siphasher | 1.0.4 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| socket2 | 0.6.5 | MIT OR Apache-2.0 |
| softbuffer | 0.4.8 | MIT OR Apache-2.0 |
| static_vcruntime | 3.0.0 | MIT OR Apache-2.0 OR Zlib |
| string_cache | 0.9.0 | MIT OR Apache-2.0 |
| string_cache_codegen | 0.6.1 | MIT OR Apache-2.0 |
| strsim | 0.11.1 | MIT |
| sync_wrapper | 1.0.2 | Apache-2.0 |
| tao | 0.37.2 | Apache-2.0 |
| tauri | 2.12.6 | Apache-2.0 OR MIT |
| tauri-build | 2.7.5 | Apache-2.0 OR MIT |
| tauri-codegen | 2.7.5 | Apache-2.0 OR MIT |
| tauri-macros | 2.7.5 | Apache-2.0 OR MIT |
| tauri-plugin | 2.7.3 | Apache-2.0 OR MIT |
| tauri-plugin-dialog | 2.8.4 | Apache-2.0 OR MIT |
| tauri-plugin-fs | 2.7.1 | Apache-2.0 OR MIT |
| tauri-plugin-notification | 2.8.1 | Apache-2.0 OR MIT |
| tauri-plugin-updater | 2.14.1 | Apache-2.0 OR MIT |
| tauri-runtime | 2.12.3 | Apache-2.0 OR MIT |
| tauri-runtime-wry | 2.12.4 | Apache-2.0 OR MIT |
| tauri-utils | 2.10.3 | Apache-2.0 OR MIT |
| tauri-winres | 0.3.7 | MIT |
| tauri-winrt-notification | 0.8.1 | MIT OR Apache-2.0 |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |
| tendril | 0.5.1 | MIT OR Apache-2.0 |
| time-macros | 0.2.32 | MIT OR Apache-2.0 |
| tokio | 1.54.1 | MIT |
| tokio-native-tls | 0.3.1 | MIT |
| tokio-util | 0.7.20 | MIT |
| toml | 1.2.1+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_datetime | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_writer | 1.2.0+spec-1.1.0 | MIT OR Apache-2.0 |
| tower | 0.5.3 | MIT |
| tower-http | 0.6.11 | MIT |
| tower-layer | 0.3.3 | MIT |
| tower-service | 0.3.3 | MIT |
| tracing | 0.1.44 | MIT |
| tracing-core | 0.1.36 | MIT |
| try-lock | 0.2.5 | MIT |
| typeid | 1.0.3 | MIT OR Apache-2.0 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 |
| urlpattern | 0.6.0 | MIT |
| uuid | 1.29.3 | Apache-2.0 OR MIT |
| vswhom | 0.1.0 | MIT |
| vswhom-sys | 0.1.3 | MIT |
| walkdir | 2.5.0 | Unlicense/MIT |
| want | 0.3.2 | MIT |
| web-time | 1.1.0 | MIT OR Apache-2.0 |
| web_atoms | 0.2.6 | MIT OR Apache-2.0 |
| webview2-com | 0.39.1 | MIT |
| webview2-com-macros | 0.8.1 | MIT |
| webview2-com-sys | 0.39.1 | MIT |
| winapi-util | 0.1.11 | Unlicense OR MIT |
| window-vibrancy | 0.8.1 | Apache-2.0 OR MIT |
| windows-registry | 0.6.1 | MIT OR Apache-2.0 |
| windows-version | 0.1.7 | MIT OR Apache-2.0 |
| winnow | 1.0.5 | MIT |
| winreg | 0.56.0 | MIT |
| wry | 0.57.2 | Apache-2.0 OR MIT |

## npm packages in the interface

| Package | Version | License |
|---|---|---|
| @internationalized/date | 3.12.4 | Apache-2.0 |
| @internationalized/number | 3.6.9 | Apache-2.0 |
| @internationalized/string | 3.2.10 | Apache-2.0 |
| @react-types/shared | 3.37.0 | Apache-2.0 |
| @swc/helpers | 0.5.24 | Apache-2.0 |
| @tauri-apps/api | 2.12.5 | Apache-2.0 OR MIT |
| @tauri-apps/plugin-dialog | 2.8.4 | MIT OR Apache-2.0 |
| aria-hidden | 1.2.6 | MIT |
| client-only | 0.0.1 | MIT |
| clsx | 2.1.1 | MIT |
| react | 19.3.1 | MIT |
| react-aria | 3.53.1 | Apache-2.0 |
| react-aria-components | 1.22.1 | Apache-2.0 |
| react-dom | 19.3.1 | MIT |
| react-stately | 3.51.0 | Apache-2.0 |
| scheduler | 0.28.0 | MIT |
| tslib | 2.9.1 | 0BSD |
| use-sync-external-store | 1.7.0 | MIT |
