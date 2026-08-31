#!/usr/bin/env python3
"""Assert each local mirror target runs exactly what the Dagger shard it names runs.

Two shards are mirrored, for the same reason from opposite directions: in both
cases the command CI runs is not a command a developer can arrive at.

**`integrationPostgres` ↔ `make test-integration-postgres` (#1169).** The
DB-backed suites in `fraiseql-core` and `fraiseql-db` provision fixtures in one
shared `public` schema, so they only pass serialized. CI knows that —
`.dagger/main.go` runs every one of them with `--test-threads=1` — but that
knowledge lived *only* there, and the two commands a developer naturally reaches
for both mislead:

  * `cargo test -p fraiseql-core` uses default parallelism, which CI never uses.
    It is red by construction, and the failing *names* drift between runs, so it
    reads as flakiness in whatever you just changed.
  * `make test-integration` passed `-- --ignored`, and none of those suites are
    `#[ignore]`d — they self-skip on an absent `DATABASE_URL`. `cargo test --
    --ignored` runs *only* ignored tests, so the target reported success having
    executed none of them.

**`Test` ↔ `make test-leg` (#1257).** The workspace suite is not one command: it
is `cargo test --workspace --exclude …` plus twenty-six feature-scoped invocations
carrying the `SYNC:*` lists, several naming explicit `--test` binaries. A
developer running `cargo test` runs a different, smaller thing — which is how
`config_coverage_manifest_test` reddened `Dagger — test` on `231c3a25c` after a
green preflight and 16 green branch legs. "Run the tests before you push" was
not checkable advice while no command existed to run.

A mirror that is maintained by hand in a second file drifts silently — that is
#1135's lesson, one leg over — so this gate holds each pair of lists together.

The comparison is **bidirectional**, unlike the preflight↔ShellGates gate. There
a stricter local target costs nothing; here the target's whole claim is "this is
what the CI shard runs", so a local-only extra line makes that claim false in the
other direction.

Every line of the shard script is classified. A shape the parser does not
recognise is reported as fatal rather than dropped: a gate that silently ignores
what it cannot read reports a parity it never checked.

Runs in preflight ShellGates (python3, stdlib only) and locally via
`make lint-shard-parity`. Its own red capability is pinned by
`tools/tests/shard_parity_test.sh`.
"""

from __future__ import annotations

import argparse
import re
import shlex
import subprocess
import sys
from pathlib import Path

# Dagger shard function -> the Makefile target that mirrors it locally.
#
# The workspace suite (`Test`) and the postgres integration shard are mirrored,
# deliberately, and the other fourteen integration shards are not: each of those
# needs a service a workstation may not have (Kafka, LocalStack, MailHog, an
# Apollo Router), and reaching for plain `cargo test` on them fails by *not
# connecting*, which is loud. The two here fail quietly instead — one by running
# a smaller suite than CI, one by colliding on a shared schema. Adding a shard is
# cheap: write the mirror target and add the row.
SHARDS = {
    "Test": "test-leg",
    "integrationPostgres": "test-integration-postgres",
}

# Shard lines that gate nothing and so have no local counterpart.
NOISE_PREFIXES = ("set -e", "echo ", "echo\t")

# `tools/ci-target-canary.sh -- <cargo args…>` wraps a leg's first build with the
# #880 stale-target-cache detector. What the wrapper means for parity follows from
# what the script does, not from which shard it appears in:
#
#   * `-- test …` appends `--no-run`: the canary BUILDS, and the shard's own
#     `cargo test` line further down is the run. Mirroring it locally would add a
#     build the mirror already performs, so it is CI-only.
#   * `-- build …` has no such split — the canary line IS the shard's build step,
#     and a mirror that skips it does not compile what CI compiles. It unwraps to
#     the plain `cargo build …` the mirror must run.
#
# The detector itself is CI-only either way: it exists because Dagger legs mount a
# persistent target/ volume and judge freshness by mtime across that mount. A local
# target/ has no such mount, so locally it would only ever rebuild.
CANARY = "bash tools/ci-target-canary.sh -- "

# Shard lines that are deliberately CI-only, with the reason they are.
CI_ONLY = {
    CANARY + "test": (
        "the #880 canary in its --no-run form; it builds, and the shard runs the "
        "same suite on its own line below, which the mirror does match."
    ),
}

GO_CONST_RE = re.compile(r"^\s*([a-zA-Z][a-zA-Z0-9_]*)\s*=\s*\"([^\"]*)\"\s*$")
ENV_ASSIGN_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")

# Marker for a `"…" + expr` piece that is neither a quoted literal nor a const
# this file can resolve — a `fmt.Sprintf`, a local variable, a computed feature
# list. Dropping such a piece would leave a syntactically fine cargo command
# with a silently WRONG expectation (`--features ''`), which is the one outcome
# worse than not checking: a confident green over a comparison that was never
# made. Lines carrying it are reported as unclassifiable instead.
UNRESOLVED = "\x00unresolved\x00"


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )


def go_constants(text: str) -> dict[str, str]:
    consts: dict[str, str] = {}
    for line in text.splitlines():
        m = GO_CONST_RE.match(line)
        if m:
            consts[m.group(1)] = m.group(2)
    return consts


def canonical_cargo(raw: str) -> str | None:
    """Normalise a cargo invocation so two spellings of it compare equal.

    Returns None if `raw` is not a cargo invocation. Env-var prefixes are
    dropped — the shard reaches Postgres at a bound service alias and the local
    target at a published port, which is the one difference that is *supposed*
    to exist. Feature lists are order-normalised; everything else, including
    `--test-threads=1`, compares verbatim, because that flag is the entire
    reason this target exists.
    """
    try:
        toks = shlex.split(raw)
    except ValueError:
        return None

    while toks and ENV_ASSIGN_RE.match(toks[0]):
        toks.pop(0)
    if not toks or toks[0] != "cargo":
        return None

    out: list[str] = []
    i = 0
    while i < len(toks):
        tok = toks[i]
        if tok == "--features" and i + 1 < len(toks):
            out += ["--features", ",".join(sorted(f for f in toks[i + 1].split(",") if f))]
            i += 2
            continue
        if tok.startswith("--features="):
            feats = tok.split("=", 1)[1]
            out.append("--features=" + ",".join(sorted(f for f in feats.split(",") if f)))
            i += 1
            continue
        out.append(tok)
        i += 1
    return " ".join(out)


def strip_go_comment(line: str) -> str:
    """Drop a trailing `// …` from a Go source line, ignoring `//` inside quotes."""
    in_quote = False
    escaped = False
    for idx, ch in enumerate(line):
        if escaped:
            escaped = False
            continue
        if ch == "\\":
            escaped = True
            continue
        if ch == '"':
            in_quote = not in_quote
            continue
        if not in_quote and ch == "/" and line[idx : idx + 2] == "//":
            return line[:idx]
    return line


def split_go_concat(entry: str) -> list[str]:
    """Split a Go expression on `+`, ignoring any `+` inside a string literal.

    Three of the `Test` shard's echo lines carry a `+` in their prose
    ("wire+functions", "manifest + doc"), and splitting on every `+` cut those
    literals in half — leaving two fragments that were neither quoted nor a
    const, so the gate reported a plain echo as a command it could not read. The
    same cut would mangle any cargo line whose arguments contain a `+`, and there
    it would not announce itself: the fragments would resolve to a *different*
    invocation, and the comparison would be made against something the shard
    never ran.
    """
    pieces: list[str] = []
    buf = ""
    in_quote = False
    escaped = False
    for ch in entry:
        if escaped:
            buf += ch
            escaped = False
            continue
        if ch == "\\":
            buf += ch
            escaped = True
            continue
        if ch == '"':
            in_quote = not in_quote
        elif ch == "+" and not in_quote:
            pieces.append(buf)
            buf = ""
            continue
        buf += ch
    pieces.append(buf)
    return pieces


def resolve_entry(entry: str, consts: dict[str, str]) -> str:
    """One `"…" + const + "…"` slice element, with the consts substituted in."""
    raw = ""
    for piece in split_go_concat(entry.rstrip(",")):
        piece = piece.strip()
        if piece.startswith('"') and piece.endswith('"'):
            raw += piece[1:-1].replace('\\"', '"')
        elif piece in consts:
            raw += consts[piece]
        else:
            raw += UNRESOLVED
    return raw.strip()


def shard_lines(text: str, func: str) -> list[str]:
    """The raw shell commands in a shard function's `script := …` literal.

    One element may span several source lines: Go's `+` binds across newlines, and
    the `Test` shard's workspace invocation is written as six lines with the
    `--exclude` list and the skip patterns concatenated on. Reading the literal
    line-by-line split that one command into six fragments, each of which then
    reported as unclassifiable — a gate that fails loudly, but for the wrong
    reason, and would have pushed whoever met it toward exempting real lines.
    """
    marker = f"func (m *FraiseqlCi) {func}("
    if marker not in text:
        raise LookupError(f"no `{func}` function in .dagger/main.go")
    body = text[text.index(marker) :]
    start = body.index("script := strings.Join([]string{")
    end = body.index('}, "\\n")', start)
    literal = body[start:end]

    consts = go_constants(text)
    commands: list[str] = []
    entry = ""
    for line in literal.splitlines():
        stripped = strip_go_comment(line).strip()
        if not stripped:
            continue
        # Outside an entry, anything not opening with a quote is the `script :=`
        # line itself or a comment; inside one, a trailing `+` says the element
        # continues on the next source line (comments may sit between the two).
        if not entry and not stripped.startswith('"'):
            continue
        entry += stripped
        if entry.endswith("+"):
            continue
        commands.append(resolve_entry(entry, consts))
        entry = ""
    if entry:
        commands.append(resolve_entry(entry, consts))
    return commands


# `target: export FOO := bar` and `target: FOO = bar` declare a target-specific
# variable, not the rule. The mirror target uses several to carry the local
# service URLs, and a parser that mistook the first of them for the rule would
# read an empty recipe and report the whole list missing.
TSV_RE = re.compile(r"^\s*(?:export\s+|unexport\s+|override\s+)*[A-Za-z_][A-Za-z0-9_]*\s*[:+?!]?=")


def makefile_recipe(text: str, target: str) -> list[str] | None:
    """The recipe lines of one Makefile target, with continuations joined."""
    lines = text.splitlines()
    target_re = re.compile(rf"^{re.escape(target)}\s*:(?!=)(.*)$")

    i = 0
    found = False
    while i < len(lines):
        m = target_re.match(lines[i])
        if m and not TSV_RE.match(m.group(1)):
            found = True
            break
        i += 1
    if not found:
        return None

    # Step over a prerequisite list that itself uses continuations.
    while lines[i].rstrip().endswith("\\") and i + 1 < len(lines):
        i += 1

    recipe: list[str] = []
    i += 1
    # Blank lines and whole-line comments may sit among recipe lines — make ignores
    # them and keeps reading the recipe. A parser that stopped at the first one read
    # a truncated recipe, or none at all: writing a comment above one line of the
    # mirror made this gate report "parsed zero cargo invocations", which is loud but
    # blames the wrong thing and teaches the next person to keep the explanation out
    # of the file it explains.
    while i < len(lines) and (
        lines[i].startswith("\t") or not lines[i].strip() or lines[i].lstrip().startswith("#")
    ):
        if not lines[i].strip() or lines[i].lstrip().startswith("#"):
            i += 1
            continue
        cmd = lines[i][1:]
        while cmd.rstrip().endswith("\\") and i + 1 < len(lines):
            i += 1
            cmd = cmd.rstrip()[:-1] + " " + lines[i].strip()
        recipe.append(cmd.lstrip("@-+").strip())
        i += 1
    return recipe


def unwrap_canary(raw: str) -> str:
    """Rewrite a `-- build` canary line as the cargo build it performs.

    Left alone, `bash tools/ci-target-canary.sh -- build --all-features` is
    unclassifiable, and exempting the whole wrapper (which is what CI_ONLY did
    while `integrationPostgres` was the only shard) would drop the `Test` shard's
    only build step out of the comparison — so a mirror that never compiled the
    workspace under `--all-features` would read as parity. See CANARY above.
    """
    if not raw.startswith(CANARY):
        return raw
    args = raw[len(CANARY) :].strip()
    return "cargo " + args if args else raw


def classify(commands: list[str]) -> tuple[list[str], list[str]]:
    """(canonical cargo invocations, unclassifiable lines)."""
    cargo: list[str] = []
    unknown: list[str] = []
    for raw in commands:
        if not raw or raw.startswith(NOISE_PREFIXES):
            continue
        if any(raw.startswith(prefix) for prefix in CI_ONLY):
            continue
        raw = unwrap_canary(raw)
        canon = canonical_cargo(raw)
        if canon is not None and UNRESOLVED not in canon:
            cargo.append(canon)
        else:
            unknown.append(raw.replace(UNRESOLVED, "<unresolved Go expression>"))
    return cargo, unknown


def check_shard(root: Path, func: str, target: str) -> list[str]:
    """Problems found for one shard/target pair; empty means parity holds."""
    makefile_text = (root / "Makefile").read_text(encoding="utf-8")
    dagger_text = (root / ".dagger" / "main.go").read_text(encoding="utf-8")
    problems: list[str] = []

    try:
        raw_shard = shard_lines(dagger_text, func)
    except (LookupError, ValueError) as exc:
        return [f"cannot read the `{func}` shard script: {exc}"]

    shard_cargo, shard_unknown = classify(raw_shard)
    if shard_unknown:
        problems.append(
            f"`{func}` runs {len(shard_unknown)} command(s) this gate cannot classify, so it "
            f"would compare a list it never read:\n"
            + "\n".join(f"    {c}" for c in shard_unknown)
            + "\n  Teach the gate the shape, or add it to CI_ONLY with a reason."
        )
    if not shard_cargo:
        problems.append(
            f"parsed zero cargo invocations out of `{func}` — the script literal's shape "
            "changed and this gate went blind"
        )

    recipe = makefile_recipe(makefile_text, target)
    if recipe is None:
        problems.append(f"no `{target}` target in the Makefile")
        return problems

    local_cargo, local_unknown = classify(recipe)
    del local_unknown  # a local target may print/wrap; only its cargo lines are compared
    if not local_cargo:
        problems.append(f"parsed zero cargo invocations out of `{target}`")

    if problems:
        return problems

    missing = [c for c in shard_cargo if c not in local_cargo]
    extra = [c for c in local_cargo if c not in shard_cargo]

    if missing:
        problems.append(
            f"`make {target}` does not run {len(missing)} invocation(s) the `{func}` shard "
            f"runs:\n" + "\n".join(f"    {c}" for c in missing)
        )
    if extra:
        problems.append(
            f"`make {target}` runs {len(extra)} invocation(s) the `{func}` shard does not, so "
            f"it no longer mirrors CI:\n" + "\n".join(f"    {c}" for c in extra)
        )
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="tree to check; defaults to the git toplevel. Used by the self-test in "
        "tools/tests/shard_parity_test.sh to prove the gate goes red.",
    )
    args = parser.parse_args()
    root = args.root if args.root is not None else repo_root()

    failed = False
    for func, target in sorted(SHARDS.items()):
        problems = check_shard(root, func, target)
        if problems:
            failed = True
            print(f"shard-parity: FAIL — {func} vs `make {target}`\n", file=sys.stderr)
            for problem in problems:
                print(f"  {problem}\n", file=sys.stderr)

    if failed:
        print(
            "A local target that claims to mirror a CI shard and does not is worse than no\n"
            "target: it reports a green over suites it never ran. Reconcile the two lists.",
            file=sys.stderr,
        )
        return 1

    print(
        "shard-parity: OK — "
        + ", ".join(f"`make {t}` mirrors {f}" for f, t in sorted(SHARDS.items()))
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
