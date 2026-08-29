#!/usr/bin/env python3
"""check-public-api-reexports.py — fail when a published crate's public API names a
third-party type the crate does not re-export.

Background (issue #1198): `fraiseql_auth::JwtValidator::new(issuer, algorithm)` takes a
`jsonwebtoken::Algorithm`, and `fraiseql-auth` re-exported neither the type nor the crate.
The documented first line of the crate's own example therefore does not compile: a
downstream must add `jsonwebtoken` as a direct dependency AND guess the major version
`fraiseql-auth` builds against. A mismatch there is a type error, not a version warning,
and it appears the day this workspace bumps the dependency — in code the downstream did
not touch.

The fix is one line per dependency: `pub use jsonwebtoken;` at the crate root. It pins
nothing extra, and it lets a caller name the exact version the signature was built with.
This gate is what keeps the next such signature from shipping unreachable.

## What counts

- **Published crates only.** The list is discovered from the `cargo publish --package X`
  steps in `.github/workflows/release.yml`. An unpublished crate's public API is an
  internal seam; nobody outside this workspace can call it.
- **Third-party types only.** A workspace sibling is published in lockstep at the same
  version, so `fraiseql-server = "2.15"` + `fraiseql-error = "2.15"` is discoverable and
  consistent. `jsonwebtoken = "?"` is not.
- **Publicly reachable items only.** A `pub fn` inside a private module is not public API.
  The module tree is walked from the crate root, and an item counts only when every
  ancestor module is `pub` — `pub(crate)` and bare `mod` stop the walk.

## Exemptions

`tools/public-api-reexports.allow`, one `crate dep::Type reason` per line. Use it when a
type is genuinely unnameable by a caller (a sealed trait's parameter) — not to retire a
dependency that is merely inconvenient to re-export.

## What it reads

Two spellings, because Rust has two. A type written by its path (`serde_json::Value`) is
attributed to the crate the path names. A type written by the name it was imported under
(`use serde_json::Value;` … `-> Value`) is attributed through that `use`. The second is the
ordinary idiom and was, for one revision, unreachable: `USE_STMT` was compiled without
`re.M`, so its `^` anchored at byte 0 of the file and matched a `use` only in a file that
opened with one. No file in this workspace does — they open with a doc comment — so the
whole branch matched nothing and the gate reported OK over 46 crate→dependency pairs in
12 crates, including the `sqlx::PgPool` that #1198 calls the worst case in fraiseql-auth
and that the #1198 commit reported as fixed. See #1234.

Signatures include enum variant payloads. A variant carries types the same way a field
does, and a downstream that matches on the enum, or constructs one, has to name them.

## Limits, stated

This reads source text, not a resolved API graph. It can over-report (a local `Router` in
a file that also imports `axum::Router`), which the allowlist absorbs, and under-report a
type reached only through a re-export chain it cannot follow. It is a ratchet against the
common shape, not a proof of completeness; the proof that a specific API is callable is a
test that compiles against it.

Every property here is pinned in `tools/tests/public_api_reexports_gate_test.sh`, whose
cases are each shown to be RED against the revision of the gate that could not see them
(`PUBLIC_API_GATE=<other-copy> bash tools/tests/public_api_reexports_gate_test.sh`).
"""
from __future__ import annotations

import re
import subprocess
import sys
import tomllib
from pathlib import Path

# `--root DIR` runs the gate against another workspace. That is what
# tools/tests/public_api_reexports_gate_test.sh uses to prove this gate can fail:
# it builds fixture workspaces in a temp dir rather than mutating this one, so an
# interrupted run cannot leave the repository half-edited.
ROOT = Path(sys.argv[sys.argv.index("--root") + 1]).resolve() if "--root" in sys.argv else Path(
    subprocess.run(["git", "rev-parse", "--show-toplevel"],
                   capture_output=True, text=True, check=True).stdout.strip())
ALLOWLIST = ROOT / "tools" / "public-api-reexports.allow"
RELEASE_YML = ROOT / ".github" / "workflows" / "release.yml"

PUB_ITEM = re.compile(r"^\s*(?:#\[[^\]]*\]\s*)?pub\s+"
                      r"(?:async\s+|const\s+|unsafe\s+|extern\s+\"[^\"]*\"\s+)*"
                      r"(fn|struct|enum|type|trait|const|static)\s")
PUB_FIELD = re.compile(r"^\s*pub\s+[a-z_][a-z0-9_]*\s*:\s")
MOD_DECL = re.compile(r"^\s*(pub(?:\s*\(\s*(?P<scope>[^)]*)\s*\))?\s+)?mod\s+([a-z_][a-z0-9_]*)\s*[;{]")
USE_STMT = re.compile(r"^\s*(?:pub\s+)?use\s+([a-z_][a-z0-9_]*)\s*(?:::\s*)?([^;]*);",
                      re.S | re.M)
TYPE_NAME = re.compile(r"\b([A-Z][A-Za-z0-9_]*)\b")
# A name preceded by `::` is spelled by its path and is read by the qualified-path
# scan below, which attributes it to the crate the path names. Matching it here too
# would attribute `anyhow::Error` to whichever crate the file happens to `use` an
# `Error` from — thiserror, in every file that derives one.
BARE_NAME = re.compile(r"(?<![:\w])([A-Z][A-Za-z0-9_]*)\b")
# A variant line, split into the variant's own identifier and its payload. The
# identifier is a name this crate chose and never a reference to a dependency.
VARIANT = re.compile(r"^([A-Z][A-Za-z0-9_]*)")


def published_crates() -> list[str]:
    """The crates release.yml actually publishes, in its own order."""
    if not RELEASE_YML.is_file():
        sys.exit(f"ERROR: {RELEASE_YML} not found — this gate reads the publish steps from it.")
    names = []
    for m in re.finditer(r"cargo publish --package (\S+)", RELEASE_YML.read_text()):
        if m.group(1) not in names:
            names.append(m.group(1))
    if not names:
        sys.exit("ERROR: no `cargo publish --package` step found in .github/workflows/release.yml. "
                 "The publish steps were renamed or removed; this gate cannot discover its "
                 "subjects and is refusing to pass vacuously.")
    return names


def workspace_packages() -> dict[str, dict]:
    """name → {dir, manifest} for every workspace member.

    Manifests are read with `tomllib` rather than `cargo metadata`: this gate runs in
    the Dagger ShellGates container, which carries make/git/awk/python3 and no Rust
    toolchain at all. A `cargo` call there is not slow, it is absent.
    """
    root_manifest = tomllib.loads((ROOT / "Cargo.toml").read_bytes().decode())
    members: dict[str, dict] = {}
    for pattern in root_manifest.get("workspace", {}).get("members", []):
        dirs = sorted(ROOT.glob(pattern)) if "*" in pattern else [ROOT / pattern]
        for d in dirs:
            manifest_path = d / "Cargo.toml"
            if not manifest_path.is_file():
                continue
            manifest = tomllib.loads(manifest_path.read_bytes().decode())
            name = manifest.get("package", {}).get("name")
            if name:
                members[name] = {"dir": d, "manifest": manifest}
    if not members:
        sys.exit(f"ERROR: no workspace members read from {ROOT / 'Cargo.toml'} — this gate "
                 "cannot discover its subjects and is refusing to pass vacuously.")
    return members


def dependency_tables(manifest: dict) -> list[dict]:
    """Every normal-dependency table: `[dependencies]` and `[target.*.dependencies]`.

    `[dev-dependencies]` and `[build-dependencies]` are excluded — neither reaches a
    caller's build, so a type from one cannot appear in this crate's public API.
    """
    tables = [manifest.get("dependencies", {})]
    for cfg in manifest.get("target", {}).values():
        tables.append(cfg.get("dependencies", {}))
    return [t for t in tables if t]


def third_party_deps(manifest: dict, members: set[str]) -> dict[str, str]:
    """Extern-crate identifier → dependency name, for normal deps outside the workspace."""
    deps = {}
    for table in dependency_tables(manifest):
        for key, spec in table.items():
            # `serde-x = {package = "serde"}` renames: the key is what the code says.
            real = spec.get("package", key) if isinstance(spec, dict) else key
            if real in members or key in members:
                continue
            deps[key.replace("-", "_")] = real
    return deps


def public_module_files(src: Path, root_file: Path) -> list[Path]:
    """Files whose items are reachable from the crate root through `pub mod` only."""
    found, queue = [], [(root_file, True)]
    seen = set()
    while queue:
        path, public = queue.pop()
        if path in seen or not path.is_file():
            continue
        seen.add(path)
        if public:
            found.append(path)
        text = path.read_text(errors="replace")
        base = path.parent if path.name in ("lib.rs", "mod.rs", "main.rs") else path.with_suffix("")
        for line in text.splitlines():
            m = MOD_DECL.match(line)
            if not m:
                continue
            is_pub = m.group(1) is not None and m.group("scope") is None
            name = m.group(3)
            child_public = public and is_pub
            for cand in (base / f"{name}.rs", base / name / "mod.rs"):
                queue.append((cand, child_public))
    return found


def signatures(text: str) -> list[str]:
    """Publicly declared signatures: the declaration line through to its `{` or `;`.

    Inline modules are scoped the same way file modules are. `mod tests { … }` and any
    other private inline module is skipped: a `pub fn` inside one is not public API, and
    reporting it would push a contributor towards an allowlist entry for a type no caller
    can reach. Brace depth is counted textually, so a brace inside a string literal can
    close a module early — it costs coverage, never a false report.
    """
    lines, out, i = text.splitlines(), [], 0
    mod_stack: list[tuple[int, bool]] = []          # (depth at which the module opened, public)
    depth = 0
    while i < len(lines):
        line = lines[i]
        m = MOD_DECL.match(line)
        inline_mod = m is not None and "{" in line
        if inline_mod:
            is_pub = m.group(1) is not None and m.group("scope") is None
            mod_stack.append((depth, is_pub))
        visible = all(pub for _, pub in mod_stack)

        if visible and not inline_mod:
            item = PUB_ITEM.match(line)
            if PUB_FIELD.match(line):
                out.append(line)
            elif item:
                sig, j = line, i
                while "{" not in sig and ";" not in sig and j + 1 < len(lines) and j - i < 20:
                    j += 1
                    sig += " " + lines[j].strip()
                opened = depth
                for k in range(i, j + 1):
                    depth += lines[k].count("{") - lines[k].count("}")
                i = j + 1
                if item.group(1) == "enum" and "{" in sig:
                    # The body may be on the declaration line (`enum E { A, B }`) or
                    # follow it; both reach enum_variants as one text.
                    head, _, rest = sig.partition("{")
                    out.append(head)
                    body = [rest]
                    k = j
                    while k + 1 < len(lines) and depth > opened:
                        k += 1
                        body.append(lines[k])
                        depth += lines[k].count("{") - lines[k].count("}")
                    i = k + 1
                    out.extend(enum_variants("\n".join(body)))
                else:
                    out.append(sig)
                while mod_stack and depth <= mod_stack[-1][0]:
                    mod_stack.pop()
                continue

        depth += line.count("{") - line.count("}")
        while mod_stack and depth <= mod_stack[-1][0]:
            mod_stack.pop()
        i += 1
    return out


def enum_variants(body: str) -> list[str]:
    """The payload of each variant in an enum body, with the variant's own name removed.

    A variant carries types the same way a field does — `Recorded(serde_json::Value)`,
    `Failed { source: url::ParseError }` — and a downstream that matches on the enum, or
    constructs one, has to name them. `signatures()` reads a declaration only as far as
    its opening brace, so before this the whole body was invisible: `pub enum Handled {
    Recorded(Value) }` read as `pub enum Handled {` and named nothing.

    Two things in a body are deliberately not payload:

    - **Doc comments and attributes.** `/// Statement is safe to execute` is prose, and
      `Statement` in it is an English word; reading it attributed a finding to
      `sqlparser` for a type the enum does not mention.
    - **The variant's own identifier.** A unit variant named `Aes256Gcm` in a file that
      imports `aes_gcm::Aes256Gcm` is the enum naming itself, not the dependency.

    Variants are split at top-level commas so a comma inside `Map<String, Value>` or
    inside a struct variant's braces does not cut one in half.
    """
    text = re.sub(r"#\[[^\]]*\]", " ", body)
    text = "\n".join(line.split("//")[0] for line in text.splitlines())
    parts: list[str] = []
    buf: list[str] = []
    depth = 0
    for ch in text:
        if ch in "([{<":
            depth += 1
        elif ch in ")]}>":
            if depth == 0 and ch == "}":
                break                       # the brace that closes the enum body
            depth = max(0, depth - 1)
        elif ch == "," and depth == 0:
            parts.append("".join(buf))
            buf = []
            continue
        buf.append(ch)
    parts.append("".join(buf))
    return [VARIANT.sub("", part.strip(), count=1) for part in parts if part.strip()]


def imported_names(text: str, deps: dict[str, str]) -> dict[str, str]:
    """Type name → extern-crate identifier, from this file's `use` statements."""
    names = {}
    for m in USE_STMT.finditer(text):
        crate, rest = m.group(1), m.group(2)
        if crate not in deps:
            continue
        for n in TYPE_NAME.findall(rest):
            names[n] = crate
    return names


def reexported(root_text: str, deps: dict[str, str]) -> tuple[set[str], set[str]]:
    """(crates re-exported wholesale, type names re-exported by name) at the crate root."""
    crates, types = set(), set()
    for m in re.finditer(r"^\s*pub use\s+([a-z_][a-z0-9_]*)\s*(::\s*)?([^;]*);", root_text, re.M | re.S):
        crate, sep, rest = m.group(1), m.group(2), m.group(3)
        if crate not in deps:
            continue
        if not sep or rest.strip() in ("", "self"):
            crates.add(crate)
            continue
        if "self" in re.split(r"[{},\s:]+", rest):
            crates.add(crate)
        types.update(TYPE_NAME.findall(rest))
    for m in re.finditer(r"^\s*pub extern crate\s+([a-z_][a-z0-9_]*)\s*;", root_text, re.M):
        crates.add(m.group(1))
    return crates, types


def load_allowlist() -> set[tuple[str, str, str]]:
    entries = set()
    if not ALLOWLIST.is_file():
        return entries
    for raw in ALLOWLIST.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(None, 2)
        if len(parts) < 3:
            sys.exit(f"ERROR: {ALLOWLIST}: every entry needs `crate dep::Type reason`: {raw!r}")
        crate, ref, _reason = parts
        if "::" not in ref:
            sys.exit(f"ERROR: {ALLOWLIST}: {ref!r} is not `dep::Type`: {raw!r}")
        dep, ty = ref.split("::", 1)
        entries.add((crate, dep, ty))
    return entries


def main() -> int:
    packages = workspace_packages()
    members = set(packages)
    allow = load_allowlist()
    used_allow: set[tuple[str, str, str]] = set()
    findings: list[tuple[str, str, str, str]] = []

    crates = published_crates()
    for name in crates:
        pkg = packages.get(name)
        if pkg is None:
            sys.exit(f"ERROR: release.yml publishes {name!r}, which is not a workspace member.")
        manifest = pkg["manifest"]
        lib_path = manifest.get("lib", {}).get("path", "src/lib.rs")
        root_file = pkg["dir"] / lib_path
        if not root_file.is_file():
            continue
        deps = third_party_deps(manifest, members)
        root_text = root_file.read_text(errors="replace")
        re_crates, re_types = reexported(root_text, deps)

        for path in public_module_files(root_file.parent, root_file):
            if path.name.endswith("tests.rs") or "/tests/" in str(path):
                continue
            text = path.read_text(errors="replace")
            names = imported_names(text, deps)
            if not names and not deps:
                continue
            for sig in signatures(text):
                hits = {(names[n], n) for n in BARE_NAME.findall(sig) if n in names}
                # (?<![:\w]) so `crate::cache::redis::RedisCacheInvalidator` does not
                # read as the `redis` crate: a segment preceded by `::` is not a crate root.
                for m in re.finditer(r"(?<![:\w])([a-z_][a-z0-9_]*)::([A-Z][A-Za-z0-9_]*)", sig):
                    if m.group(1) in deps:
                        hits.add((m.group(1), m.group(2)))
                for crate, ty in hits:
                    if crate in re_crates or ty in re_types:
                        continue
                    key = (name, crate, ty)
                    if key in allow:
                        used_allow.add(key)
                        continue
                    rel = path.relative_to(ROOT)
                    findings.append((name, f"{crate}::{ty}", str(rel), sig.strip()[:100]))

    stale = allow - used_allow
    failed = 0

    if findings:
        seen = set()
        print("ERROR: published crates whose public API names a third-party type they do not "
              "re-export:", file=sys.stderr)
        for crate, ref, rel, sig in findings:
            if (crate, ref) in seen:
                continue
            seen.add((crate, ref))
            print(f"  {crate}: {ref}\n      {rel}\n      {sig}", file=sys.stderr)
        print(f"\n{len(seen)} unreachable references across "
              f"{len({c for c, _ in seen})} crates.\n"
              "A caller cannot name these without adding the dependency and guessing the\n"
              "version this workspace builds against. Add `pub use <dep>;` at the crate root,\n"
              "or record a reason in tools/public-api-reexports.allow. See issue #1198.",
              file=sys.stderr)
        failed = 1

    if stale:
        print("ERROR: stale entries in tools/public-api-reexports.allow — these are now "
              "reachable, so the exemption is a claim about nothing:", file=sys.stderr)
        for crate, dep, ty in sorted(stale):
            print(f"  {crate} {dep}::{ty}", file=sys.stderr)
        failed = 1

    if not failed:
        print(f"OK: {len(crates)} published crates; every third-party type in a public "
              f"signature is reachable from its crate root ({len(allow)} exempt).")
    return failed


if __name__ == "__main__":
    sys.exit(main())
