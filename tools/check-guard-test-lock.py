#!/usr/bin/env python3
"""The guard-test lock gate: a test that asserts a guard's behaviour must take the env lock.

`tools/check-guard-parity.sh` gates the *production* side of the deployment guards —
one address predicate, one production detector, no ungated escape hatch. It skips test
files wholesale, on purpose: test code legitimately names blocked addresses. This is the
test side of the same rule.

Why it exists
-------------
`fraiseql_guard::deployment::{is_production, env_opt_in}` read process-global
environment. `temp_env` serialises its mutations through a global `ReentrantMutex` —
which is deterministic only if *both* sides take it. A test that reads a guard without
the lock races whichever sibling in the same binary is inside a `temp_env` window, and
the guard reads as bypassed.

Four tests in `fraiseql-secrets`' vault suite did exactly that (#1272). Three asserted a
refusal, so losing the race reddened `Dagger — test` at random. The fourth asserted
`Ok` — measured, it passed with the SSRF guard mutated to refuse *every* address,
because `validate_vault_addr` returns `Ok(())` from the bypass before the guard runs.
That direction never reddens anything. It is the reason this is a gate and not a
convention: the loud half announces itself, the silent half does not.

This is the second crate to ship it. The first was the OIDC guard's
`a_private_graph_url_is_refused_by_the_ssrf_guard`, same cause, different crate.

What this gate does NOT catch
-----------------------------
A test that reaches a guard *indirectly*. The rule matches a call to the function that
encloses the chokepoint call, and nothing above it. The OIDC precedent above is exactly
that shape — it calls `FacebookOAuth::with_endpoints`, three hops above
`oidc_ssrf_guards_disabled` — so this gate would not have caught it. (That test takes the
lock today; this is a coverage limit, not a live defect.)

Widening it was measured and rejected, not overlooked. Extending the entry-point set by a
single hop of a name-based call graph takes it from 9 functions to 25, and the tests it
flags from 0 to **3 150** — `new`, `build` and `from_config` all reach a guard somewhere in
their crate, so every test that constructs anything becomes a violation. Full closure
reaches 43% of the workspace's 9 619 functions. A gate at that noise level is switched off
within a week, so the narrow rule that holds is worth more than the broad one that does not.
Resolving calls properly needs a type-aware pass (rust-analyzer or a `cargo` lint), which is
the shape any future widening should take.

The rule
--------
In a test binary that mutates deployment environment, every `#[test]`/`#[tokio::test]`
function that calls a *guard entry point* must take the `temp_env` lock — directly, or
through a helper in the same binary that does.

Nothing here is hardcoded that can be discovered instead. The guard entry points are
the enclosing functions of every `insecure_bypass` / `insecure_bypass_allowed` /
`env_opt_in` / `is_production` call in non-test source, recomputed on each run, so a new
guard is covered the day it is written rather than the day someone remembers this file.

Known approximation
-------------------
Entry points are resolved per *function*, not per branch. `create_secrets_manager` reads
`is_production()` only in its `Vault` arm, so a test that drives its `File` arm is
flagged although it can never reach the read. That case is exempted below by name, with
its reason, and the exemption is verified to still resolve — an exemption that has
rotted into naming nothing is a gate hole, so it aborts rather than passing quietly.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
# Overridable so tools/tests/guard_test_lock_test.sh can prove each branch on a
# synthetic tree. A gate proved only on the live repo is proved only until the repo
# changes — and this one's whole job is to pass over a tree it just cleaned.
ROOT = Path(os.environ.get("GUARD_TEST_LOCK_ROOT", REPO))

# The chokepoint: every deployment-posture read in the workspace resolves here.
CHOKEPOINT = ("insecure_bypass", "insecure_bypass_allowed", "env_opt_in", "is_production")
CHOKEPOINT_RE = re.compile(r"\b(" + "|".join(CHOKEPOINT) + r")\s*\(")

# Where the chokepoint is *defined*. Its own callers inside this file are the
# implementation, not entry points.
CHOKEPOINT_DEFN = "fraiseql-guard/src/deployment"

# Names whose value steers a guard. The three posture variables, plus any hatch: an
# `*_ALLOW_INSECURE` / `*_ALLOW_PLAINTEXT` / `*_SKIP_VERIFY` / `*_DISABLE_TLS`, whether
# spelled as a literal or as the `*_ENV` constant that holds it.
DEPLOYMENT_ENV_RE = re.compile(
    r"FRAISEQL_ENV|FRAISEQL_PROFILE|KUBERNETES_SERVICE_HOST"
    r"|ALLOW_INSECURE|ALLOW_PLAINTEXT|SKIP_VERIFY|DISABLE_TLS"
)
# Anything that writes process env. `temp_env` is the sanctioned form; the bare calls
# are listed because a binary that uses them is *still* a binary whose readers race.
MUTATES_ENV_RE = re.compile(r"temp_env::|env::set_var|env::remove_var")
# How far past a mutation call to look for the variable it names. `temp_env::with_vars`
# takes a multi-line array of pairs; every such call in this workspace names its
# variables well inside this. A window is enough because the alternative — matching the
# file — arms the gate on crates that merely mention a hatch in unrelated source.
MUTATION_ARGS_WINDOW = 500
TEST_ATTR_RE = re.compile(r"#\[(tokio::)?test\]|#\[test\(")

# (path suffix, test fn, reason). Verified to resolve; see "Known approximation".
EXEMPTIONS = [
    (
        "fraiseql-secrets/src/secrets_manager/tests.rs",
        "test_create_secrets_manager_file_backend",
        "drives create_secrets_manager's File arm; the is_production() read is in the "
        "Vault arm and is unreachable from this config",
    ),
]


def iter_fns(text: str):
    """Yield (name, body, start_offset) for each `fn name(…) { … }`, by brace matching.

    Regex cannot find a function's end, and a body that stops at the first `}` would
    call every multi-block test lock-free.
    """
    for m in re.finditer(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*[(<]", text):
        open_brace = text.find("{", m.end() - 1)
        if open_brace < 0:
            continue
        depth, j = 0, open_brace
        while j < len(text):
            if text[j] == "{":
                depth += 1
            elif text[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        yield m.group(1), text[open_brace : j + 1], m.start()


def is_test_source(path: Path) -> bool:
    s = path.as_posix()
    return "/tests/" in s or path.name == "tests.rs" or path.name.endswith("_tests.rs")


def discover_entry_points() -> dict[str, str]:
    """Guard entry points: the enclosing fn of a chokepoint call, in non-test source.

    Returned empty only when the scan itself has broken — this workspace routes every
    posture read through the chokepoint, so zero derived entry points is never a fact
    about the tree. The caller treats it as an error rather than as a clean run, which
    is the difference between a gate and a gate-shaped no-op.
    """
    entries: dict[str, str] = {}
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        if is_test_source(path) or CHOKEPOINT_DEFN in path.as_posix():
            continue
        text = path.read_text(errors="replace")
        if not CHOKEPOINT_RE.search(text):
            continue
        for name, body, _ in iter_fns(text):
            if CHOKEPOINT_RE.search(body):
                entries.setdefault(name, path.relative_to(ROOT).as_posix())
    return entries


def test_binaries() -> list[tuple[str, list[Path]]]:
    """(label, sources) per test binary.

    A crate's `src/` tree compiles into one `--lib` test binary, so its tests share a
    process with each other. Each `crates/*/tests/<stem>.rs` is its own binary, and
    shares a process with nothing — which is why a lock-free reader there is not a race.
    """
    out: list[tuple[str, list[Path]]] = []
    for crate in sorted((ROOT / "crates").glob("*")):
        src = crate / "src"
        if src.is_dir():
            out.append((f"{crate.name} --lib", sorted(src.rglob("*.rs"))))
        for entry in sorted((crate / "tests").glob("*.rs")):
            sibling = crate / "tests" / entry.stem
            extra = sorted(sibling.rglob("*.rs")) if sibling.is_dir() else []
            out.append((f"{crate.name} tests/{entry.name}", [entry, *extra]))
    return out


def lock_taking_fns(texts: list[str]) -> set[str]:
    """Fn names that reach `temp_env::`, transitively within the binary.

    One level is not enough: a helper that wraps `with_guard_engaged` is still holding
    the lock, and flagging its callers would train people to ignore this gate.
    """
    bodies = {}
    for text in texts:
        for name, body, _ in iter_fns(text):
            bodies[name] = bodies.get(name, "") + body
    locky = {n for n, b in bodies.items() if "temp_env::" in b}
    changed = True
    while changed:
        changed = False
        for name, body in bodies.items():
            if name in locky:
                continue
            if any(re.search(rf"\b{re.escape(h)}\s*\(", body) for h in locky):
                locky.add(name)
                changed = True
    return locky


def main() -> int:
    entries = discover_entry_points()
    if not entries:
        print("ERROR: discovered zero guard entry points — the scan is broken, not the tree.")
        print(f"       Looked for {'/'.join(CHOKEPOINT)} under {ROOT}/crates.")
        return 2

    # A test may also call the chokepoint directly rather than through a named guard.
    # Added after the emptiness check above, so they cannot mask a broken scan.
    for name in CHOKEPOINT:
        entries.setdefault(name, CHOKEPOINT_DEFN)

    entry_re = re.compile(r"\b(" + "|".join(map(re.escape, sorted(entries))) + r")\s*\(")

    checked = 0
    binaries_with_mutators = 0
    offenders: list[tuple[str, str, str]] = []
    seen: set[tuple[str, str]] = set()

    for label, sources in test_binaries():
        texts = []
        for p in sources:
            try:
                texts.append((p, p.read_text(errors="replace")))
            except OSError:
                continue
        blob = "\n".join(t for _, t in texts)
        if not TEST_ATTR_RE.search(blob):
            continue
        # A lock-free read is only a race if something in the same process writes a
        # variable a guard reads. The deployment name must appear in the mutation's own
        # argument list — testing the binary for both facts separately arms the gate on
        # any crate whose *source* merely names a hatch, which is most of them.
        if not any(
            DEPLOYMENT_ENV_RE.search(text[m.start() : m.start() + MUTATION_ARGS_WINDOW])
            for _, text in texts
            for m in MUTATES_ENV_RE.finditer(text)
        ):
            continue
        binaries_with_mutators += 1
        locky = lock_taking_fns([t for _, t in texts])

        for path, text in texts:
            for name, body, off in iter_fns(text):
                if not TEST_ATTR_RE.search(text[max(0, off - 200) : off]):
                    continue
                if not entry_re.search(body):
                    continue
                checked += 1
                rel = path.relative_to(ROOT).as_posix()
                seen.add((rel, name))
                if "temp_env::" in body:
                    continue
                if any(re.search(rf"\b{re.escape(h)}\s*\(", body) for h in locky):
                    continue
                if any(rel.endswith(p) and name == f for p, f, _ in EXEMPTIONS):
                    continue
                offenders.append((rel, name, label))

    if not checked:
        print("ERROR: zero test functions call a guard entry point — the scan is broken.")
        print(f"       {len(entries)} entry points, {binaries_with_mutators} binaries scanned.")
        return 2

    # An exemption that no longer names a real test is a hole, not a no-op. It is
    # checked whenever the file it names exists under ROOT: on a synthetic tree the file
    # is absent and the exemption describes a different repository, not a rotted row.
    # (Keying this on ROOT == REPO instead would silently skip the check for any copy of
    # the gate run from elsewhere — including this gate's own red-capability pin.)
    scanned_files = {rel for rel, _ in seen}
    stale = [
        (p, f)
        for p, f, _ in EXEMPTIONS
        if any(rel.endswith(p) for rel in scanned_files)
        and not any(rel.endswith(p) and n == f for rel, n in seen)
    ]
    if stale:
        print("ERROR: an exemption in check-guard-test-lock.py names nothing:")
        for p, f in stale:
            print(f"  {p}::{f}")
        print("\n  It was renamed, deleted, or no longer calls a guard entry point.")
        print("  Drop the entry — leaving it exempts a name that may come back unlocked.")
        return 2

    if offenders:
        print("ERROR: a test asserts a guard's behaviour without taking the temp_env lock:")
        for rel, name, label in offenders:
            print(f"  {rel}::{name}   [binary: {label}]")
        print("\n  Wrap the body in a helper that goes through temp_env (the suites that")
        print("  already do this call theirs `with_guard_engaged` / `with_bypass_requested`).")
        print("  `temp_env` serialises through a global mutex, so a lock-free reader races")
        print("  the sibling that sets the bypass and the guard reads as bypassed. A test")
        print("  asserting a refusal then fails at random; one asserting `Ok` passes with")
        print("  the guard fully disabled and never reddens anything at all (#1272).")
        return 1

    print(
        f"OK: {checked} guard-asserting tests across {binaries_with_mutators} "
        f"env-mutating binaries all take the lock "
        f"({len(entries)} entry points, {len(EXEMPTIONS)} exemption(s))."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
