#!/usr/bin/env python3
"""The reachability gate: every `crates/*/src/**/*.rs` is part of some compiled target.

rustc compiles a file only when a `mod` declaration reaches it from a target root.
A file nothing declares is never type-checked, never linted, never tested — and it
still reads as code to everyone who opens the tree, to every grep that counts lines,
and to every reviewer who finds a defect in it. Eleven such files (2 652 lines) sat
in this workspace (#1640): a whole `observers/src/tracing/` subsystem, an HTTP
metrics endpoint, and a wasm limiter test module, all with their `mod` lines gone.

The rule
--------
Starting from every target root of every crate — `src/lib.rs`, `src/main.rs`,
`src/bin/*.rs`, `src/bin/*/main.rs`, `build.rs`, `tests/*.rs`, `benches/*.rs`,
`examples/*.rs` and any `path = "…"` in the crate's Cargo.toml — follow `mod`
declarations the way rustc resolves them, and fail on any file under `src/` the
walk never reaches.

Resolution follows the Rust reference:

* `mod x;` in a mod-rs file (a crate root, a `mod.rs`, or a file loaded through
  `#[path]`) loads `<dir>/x.rs` or `<dir>/x/mod.rs`; in any other file `foo.rs` it
  loads `<dir>/foo/x.rs` or `<dir>/foo/x/mod.rs`.
* An inline `mod outer { … }` adds `outer/` to the directory for declarations
  inside its braces.
* `#[path = "p"] mod x;` loads `p` relative to the declaring file's directory
  (or, inside inline modules, relative to the directory those modules imply).
* `#[cfg(…)]` is ignored: a declaration compiled under some feature still makes
  its file part of the crate, and the feature-matrix legs compile it.
* Comments and string literals are stripped first, so `// mod tracing;` — the
  shape a disconnected module usually leaves behind — reaches nothing.

The walk reads the filesystem, never git: the preflight container has no index
(`git ls-files` is empty there), and a gate that reads it passes vacuously.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ROOT = Path(os.environ.get("MODULE_REACHABILITY_ROOT", REPO))

MOD_DECL = re.compile(r"\bmod\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)\s*([;{])")
PATH_ATTR = re.compile(r'#\s*\[\s*path\s*=\s*"([^"]*)"\s*\]')
# What may sit between a `#[path]` attribute and the `mod` it belongs to: other
# attributes, visibility, whitespace.
ATTR_GAP = re.compile(r"\s*(?:#\s*\[[^\]]*\]\s*|pub(?:\s*\([^)]*\))?\s+)*$")
MANIFEST_PATH = re.compile(r'^\s*path\s*=\s*"([^"]+\.rs)"', re.MULTILINE)


def strip(text: str) -> str:
    """Replace comments with spaces; keep string literals only where they are attribute values.

    Positions are preserved so brace depth stays aligned. A string's contents are
    blanked except its quotes, which leaves `#[path = "…"]` intact: the attribute
    regex needs the value, and nothing else in a string can declare a module.
    """
    out = list(text)
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j == -1 else j
            out[i:j] = " " * (j - i)
            i = j
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            out[i:j] = [ch if ch == "\n" else " " for ch in text[i:j]]
            i = j
        elif c == "r" and re.match(r'r#*"', text[i:]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = len(re.match(r"r(#*)", text[i:]).group(1))
            start = i + 2 + hashes
            end = text.find('"' + "#" * hashes, start)
            end = n if end == -1 else end
            out[start:end] = [ch if ch == "\n" else " " for ch in text[start:end]]
            i = end + 1 + hashes
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            if not _is_path_attr_value(text, i):
                out[i + 1 : j] = [ch if ch == "\n" else " " for ch in text[i + 1 : j]]
            i = j + 1
        elif c == "'":
            if i + 1 < n and text[i + 1] == "\\":  # escaped char literal
                j = text.find("'", i + 2)
                i = n if j == -1 else j + 1
            elif i + 2 < n and text[i + 2] == "'":  # plain char literal, incl. '{'
                out[i + 1] = " "
                i += 3
            else:  # a lifetime
                i += 1
        else:
            i += 1
    return "".join(out)


def _is_path_attr_value(text: str, quote: int) -> bool:
    return re.search(r"#\s*\[\s*path\s*=\s*$", text[max(0, quote - 40) : quote]) is not None


def is_mod_rs(path: Path, roots: set[Path], path_loaded: set[Path]) -> bool:
    return path.name == "mod.rs" or path in roots or path in path_loaded


def declared_files(path: Path, mod_rs: bool) -> list[tuple[Path, bool]]:
    """Every file one source file declares, each flagged True when loaded via `#[path]`."""
    text = strip(path.read_text(errors="replace"))
    file_dir = path.parent
    base = file_dir if mod_rs else file_dir / path.stem
    # Directory in force at each brace depth: (dir for plain `mod x;`, dir for `#[path]`).
    stack: list[tuple[Path, Path]] = [(base, file_dir)]
    found: list[tuple[Path, bool]] = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        m = MOD_DECL.match(text, i) if c == "m" and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")) else None
        if m:
            name, terminator = m.group(1), m.group(2)
            attr = _path_attr_before(text, i)
            plain_dir, path_dir = stack[-1]
            if terminator == "{":
                inner = plain_dir / (attr if attr else name)
                stack.append((inner, inner))
            elif attr:
                found.append(((path_dir / attr).resolve(), True))
            else:
                for cand in (plain_dir / f"{name}.rs", plain_dir / name / "mod.rs"):
                    if cand.is_file():
                        found.append((cand.resolve(), False))
                        break
            i = m.end()
        elif c == "{":
            stack.append(stack[-1])
            i += 1
        elif c == "}":
            if len(stack) > 1:
                stack.pop()
            i += 1
        else:
            i += 1
    return found


def _path_attr_before(text: str, pos: int) -> str | None:
    """The `#[path = "…"]` attached to the `mod` at `pos`, if any."""
    window = text[max(0, pos - 400) : pos]
    matches = list(PATH_ATTR.finditer(window))
    if matches and ATTR_GAP.fullmatch(window[matches[-1].end() :]):
        return matches[-1].group(1)
    return None


def crate_roots(crate: Path) -> set[Path]:
    roots: set[Path] = set()
    for rel in ("src/lib.rs", "src/main.rs", "build.rs"):
        if (crate / rel).is_file():
            roots.add((crate / rel).resolve())
    for pattern in ("src/bin/*.rs", "src/bin/*/main.rs", "tests/*.rs", "benches/*.rs", "examples/*.rs", "examples/*/main.rs"):
        roots.update(p.resolve() for p in crate.glob(pattern))
    manifest = crate / "Cargo.toml"
    if manifest.is_file():
        for rel in MANIFEST_PATH.findall(manifest.read_text(errors="replace")):
            if (crate / rel).is_file():
                roots.add((crate / rel).resolve())
    return roots


def unreachable(root: Path) -> list[Path]:
    orphans: list[Path] = []
    for crate in sorted(p.parent for p in root.glob("crates/*/Cargo.toml")):
        roots = crate_roots(crate)
        reached: set[Path] = set()
        path_loaded: set[Path] = set()
        pending = list(roots)
        while pending:
            path = pending.pop()
            if path in reached or not path.is_file():
                continue
            reached.add(path)
            for child, via_path in declared_files(path, is_mod_rs(path, roots, path_loaded)):
                if via_path:
                    path_loaded.add(child)
                pending.append(child)
        src = crate / "src"
        orphans.extend(sorted(p for p in src.rglob("*.rs") if p.resolve() not in reached))
    return orphans


def main() -> int:
    if not any(ROOT.glob("crates/*/src/*.rs")):
        print(f"module-reachability: FAIL — no crates/*/src under {ROOT}; refusing to pass vacuously", file=sys.stderr)
        return 1
    orphans = unreachable(ROOT)
    if not orphans:
        print("module-reachability: OK — every crates/*/src/**/*.rs is declared by a target")
        return 0
    total = 0
    for path in orphans:
        lines = sum(1 for _ in path.open(errors="replace"))
        total += lines
        print(f"✗ {path.relative_to(ROOT)} ({lines} lines): no `mod` declaration reaches it from any target root")
    print(
        f"module-reachability: FAIL — {len(orphans)} file(s), {total} lines, are never compiled. "
        "Declare each from its parent module, or delete it.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
