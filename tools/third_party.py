"""Writes THIRD-PARTY.md: every Rust crate and npm package that ships in Prism Transcribe, with its licence, and fails
when one is not under a permissive licence (MIT, BSD, Apache 2.0, ISC, Zlib and the like). Adapted from Prism Survey's
tools/third_party.py.

    python tools/third_party.py            write THIRD-PARTY.md
    python tools/third_party.py --check    only check, and say what is not permissive

Rust crates come from `cargo tree` for Windows, normal and build dependencies (test-only crates are left out), for the
engine and command-line tool, and for the desktop app. npm packages come from `npm ls --omit=dev` in app/ui, with the
licence each package's own package.json gives. The speech models are not code and are listed in NOTICE and
docs/MODELS.md.
"""

import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = "x86_64-pc-windows-msvc"
PERMISSIVE = {
    "MIT", "MIT-0", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause", "0BSD", "ISC",
    "Zlib", "Unlicense", "BSL-1.0", "CC0-1.0", "Unicode-3.0", "Unicode-DFS-2016", "BlueOak-1.0.0", "Python-2.0",
    # bzip2's own BSD-style licence (libbz2-rs-sys), and Mozilla's CA roots' data licence (webpki-roots, used by the
    # model download).
    "bzip2-1.0.6", "CDLA-Permissive-2.0",
}
OWN = re.compile(r"^prism-transcribe")
# MPL-2.0 crates that Tauri's window needs, used unchanged: the same exception Jason accepted by name for Prism Survey
# (its docs/DECISIONS.md 55). Listed here so the check names them; they still need his word for this app (README,
# "Releasing").
EXCEPTIONS = {("cssparser", "MPL-2.0"), ("cssparser-macros", "MPL-2.0"), ("dtoa-short", "MPL-2.0"), ("selectors", "MPL-2.0"), ("option-ext", "MPL-2.0")}


def permissive(expr):
    if not expr:
        return False
    e = expr.replace("/", " OR ").replace("(", " ").replace(")", " ")
    for part in re.split(r"\s+AND\s+", e):
        choices = [c.strip() for c in re.split(r"\s+OR\s+", part) if c.strip()]
        if not any(c in PERMISSIVE for c in choices):
            return False
    return True


def crates(packages):
    args = ["cargo", "tree", "--target", TARGET, "-e", "normal,build", "--prefix", "none", "--format", "{p}\t{l}"]
    for p in packages:
        args += ["-p", p]
    out = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", check=True, cwd=ROOT).stdout
    found = {}
    for line in out.splitlines():
        line = line.replace(" (*)", "").strip()
        if "\t" not in line:
            continue
        p, lic = line.split("\t", 1)
        bits = p.split()
        if len(bits) < 2 or OWN.match(bits[0]):
            continue
        found[(bits[0], bits[1].lstrip("v"))] = lic.strip()
    return found


def npm_packages():
    ui = os.path.join(ROOT, "app", "ui")
    out = subprocess.run("npm ls --omit=dev --all --json", shell=True, capture_output=True, text=True, encoding="utf-8", cwd=ui).stdout
    tree = json.loads(out or "{}")
    found = {}

    def walk(deps):
        for name, info in (deps or {}).items():
            ver = info.get("version", "")
            pj = os.path.join(ui, "node_modules", *name.split("/"), "package.json")
            lic = ""
            if os.path.exists(pj):
                lic = json.load(open(pj, encoding="utf-8")).get("license", "")
                if isinstance(lic, dict):
                    lic = lic.get("type", "")
            found[(name, ver)] = lic
            walk(info.get("dependencies"))

    walk(tree.get("dependencies"))
    return found


def main():
    check = "--check" in sys.argv
    engine = crates(["prism-transcribe-engine"])
    app = crates(["prism-transcribe-app"])
    window_only = {k: v for k, v in app.items() if k not in engine}
    npm = npm_packages()
    bad = [(k, v) for d in (engine, window_only, npm) for k, v in d.items() if not permissive(v) and (k[0], v) not in EXCEPTIONS]
    excepted = sorted({k[0] for d in (engine, window_only) for k, v in d.items() if (k[0], v) in EXCEPTIONS})
    for (n, ver), lic in sorted(bad):
        print(f"not permissive or not stated: {n} {ver}: {lic or '(none)'}")
    print(f"{len(engine)} engine crates, {len(window_only)} more for the window, {len(npm)} npm packages; {len(bad)} to look at; MPL-2.0 by name: {', '.join(excepted) or 'none'}")
    if check:
        sys.exit(1 if bad else 0)

    def table(d, head):
        rows = [f"| {head} | Version | License |", "|---|---|---|"]
        rows += [f"| {n} | {v} | {lic} |" for (n, v), lic in sorted(d.items(), key=lambda x: (x[0][0].lower(), x[0][1]))]
        return rows

    lines = [
        "# Third-party components",
        "",
        "Everything built into Prism Transcribe that others wrote, each with its license as its authors publish it. Where a component offers a choice, Prism Transcribe uses it under the MIT or Apache 2.0 terms. Test-only and build-tool components that do not ship are not listed. This file is written by `python tools/third_party.py`, which also checks that every license is permissive. The speech models are listed in NOTICE and docs/MODELS.md.",
        "",
        f"One exception to the permissive rule, by name: crates the window needs ({', '.join(excepted)}) are under MPL-2.0, which asks only that changes to those files be shared. Prism Transcribe uses them unchanged; their source is at crates.io." if excepted else "",
        "",
        "sherpa-onnx and ONNX Runtime are linked into the program (Apache 2.0 and MIT; see NOTICE). The de-identification suggestions use the spell checker built into Windows, called through the `windows` crate; nothing is bundled for it. The window is drawn by the web view built into Windows; the installer fetches it only if the computer lacks it.",
        "",
        "## Rust crates in the engine and command-line tool",
        "",
        *table(engine, "Crate"),
        "",
        "## Rust crates the desktop window adds",
        "",
        *table(window_only, "Crate"),
        "",
        "## npm packages in the interface",
        "",
        *table(npm, "Package"),
        "",
    ]
    open(os.path.join(ROOT, "THIRD-PARTY.md"), "w", encoding="utf-8", newline="\n").write("\n".join(lines))
    print("wrote THIRD-PARTY.md")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
