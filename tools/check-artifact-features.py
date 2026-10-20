#!/usr/bin/env python3
"""Assert every platform-artifact recipe compiles the feature set the docs promise.

The "platform" artifacts are the ones a deployment downloads to run the whole
product rather than a plain GraphQL server: the `fraiseql-full-<target>` release
tarball (its umbrella `fraiseql` binary and its standalone `fraiseql-server`) and
the `fraiseql-server-platform` image. Their feature lists are written by hand in
six places, and #1621 is what that produced: the release binaries shipped without
REST, Arrow Flight or CDC outbound while the image had two of the three, and the
umbrella never forwarded `federation` or `arrow` to the server at all.

The authority is the `fraiseql-full-<target>` row of `docs/releases.md`, the one
place an adopter reads. Its Features cell names features of the `fraiseql` crate
in backticks. Each recipe below must compile exactly that surface:

  * the `fraiseql` crate's `release-full` bundle (`crates/fraiseql/Cargo.toml`);
  * every featured `fraiseql-server` build in `release.yml` (the `-full` tarball's
    second binary) and in `release-smoke.yml` (the builds that boot it before the
    tag);
  * the `fraiseql-server-platform` row of `.dagger/image.go`, which
    `check-image-parity.py` already holds equal to both `docker-build.yml`
    matrices.

The comparison is on the **fraiseql-server feature closure**: each list is
expanded through the feature tables in both `Cargo.toml`s (an umbrella feature
reaches the server only through a `fraiseql-server?/x` entry, so a pass-through
that forgets one is visible), and two recipes agree when they compile the same
server features. `inbound-email` and `inbound, inbound-email` are the same build;
`federation` on the umbrella without `fraiseql-server?/federation` is not.

Anything this gate cannot read is FATAL rather than skipped: a recipe that moved
out of the shape parsed here would otherwise report a parity never checked.

Runs in preflight and ShellGates (python3, stdlib only). Locally:
`make lint-artifact-features`. Its red capability is pinned by
`tools/tests/artifact_features_test.sh`.
"""

from __future__ import annotations

import argparse
import importlib.util
import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

UMBRELLA_MANIFEST = "crates/fraiseql/Cargo.toml"
SERVER_MANIFEST = "crates/fraiseql-server/Cargo.toml"
DOC = "docs/releases.md"
DOC_ROW = "| `fraiseql-full-<target>`"
BUNDLE = "release-full"
IMAGE_VARIANT = "fraiseql-server-platform"
# Workflows whose featured `fraiseql-server` builds are platform builds. Each must
# carry at least one: a file whose builds all vanished has stopped being checked.
WORKFLOWS = (
    ".github/workflows/release.yml",
    ".github/workflows/release-smoke.yml",
)

SERVER_BUILD = re.compile(r"cargo\s+(?:zig)?build\b.*?(?:-p|--package)\s+fraiseql-server\b")
FEATURES_ARG = re.compile(r"--features[\s=]+(\"[^\"]*\"|'[^']*'|\S+)")
BACKTICKED = re.compile(r"`([^`]+)`")


def die(message: str) -> None:
    print(f"FATAL: {message}", file=sys.stderr)
    raise SystemExit(2)


def _sibling(name: str, mod: str):
    path = REPO / "tools" / name
    spec = importlib.util.spec_from_file_location(mod, path)
    if spec is None or spec.loader is None:
        die(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    # Registered before execution: @dataclass in an imported module resolves its own
    # sys.modules[__module__], and an unregistered module makes that lookup return None.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


# ── Feature tables ───────────────────────────────────────────────────────────


def feature_table(path: Path) -> dict[str, list[str] | None]:
    """The `[features]` table, refusing a manifest that leaves Cargo an implicit feature.

    Every optional dependency is named `dep:<name>` by some feature, no feature lists an
    optional dependency bare (`"rmcp"` rather than `"dep:rmcp"`), and no feature exists
    only to re-enable a dependency of its own name (`prost = ["dep:prost"]`). Each of those
    is an implicit dependency feature: a name downstream manifests can depend on that
    enables no feature of this crate.
    """
    if not path.is_file():
        die(f"{path} does not exist")
    try:
        manifest = tomllib.loads(path.read_text())
    except tomllib.TOMLDecodeError as exc:
        die(f"{path}: {exc}")
    features = manifest.get("features")
    if not isinstance(features, dict) or not features:
        die(f"{path} has no [features] table")
    optional = {
        name
        for name, spec in manifest.get("dependencies", {}).items()
        if isinstance(spec, dict) and spec.get("optional")
    }
    entries = [entry for listed in features.values() for entry in listed]
    for name in sorted(optional & features.keys()):
        if features[name] == [f"dep:{name}"]:
            die(f"{path}: the feature `{name}` only re-enables the optional dependency `{name}`")
    for name in sorted(optional - features.keys()):
        if name in entries:
            die(f"{path}: a feature names the optional dependency `{name}` without `dep:`")
        if f"dep:{name}" not in entries:
            die(f"{path}: the optional dependency `{name}` is named by no feature as `dep:{name}`")
    return features


def server_closure(
    names: set[str], server: dict[str, list[str] | None], origin: str
) -> frozenset[str]:
    """`names` plus every server feature they enable, transitively.

    `default` is included: none of the recipes passes `--no-default-features` to the
    server, and the umbrella depends on it with default features on.
    """
    pending = [*names, "default"]
    seen: set[str] = set()
    while pending:
        name = pending.pop()
        if name in seen:
            continue
        if name not in server:
            die(f"{origin}: `{name}` is not a fraiseql-server feature")
        seen.add(name)
        # `dep:x` and `crate/feat` entries enable no server feature of their own.
        entries = server[name] or []
        pending.extend(e for e in entries if not e.startswith("dep:") and "/" not in e)
    return frozenset(n for n in seen if n != "default")


def umbrella_closure(
    names: set[str],
    umbrella: dict[str, list[str] | None],
    server: dict[str, list[str] | None],
    origin: str,
) -> frozenset[str]:
    """The server features a set of `fraiseql` crate features compiles in."""
    pending = list(names)
    seen: set[str] = set()
    reached: set[str] = set()
    while pending:
        name = pending.pop()
        if name in seen:
            continue
        if name not in umbrella:
            die(f"{origin}: `{name}` is not a feature of the fraiseql crate")
        seen.add(name)
        for entry in umbrella[name] or []:
            if entry.startswith(("fraiseql-server/", "fraiseql-server?/")):
                reached.add(entry.split("/", 1)[1])
            elif not entry.startswith("dep:") and "/" not in entry:
                pending.append(entry)
    if "server" not in seen:
        # Without `server` the umbrella compiles no server at all, so there is no
        # surface to compare; every platform recipe ships one.
        die(f"{origin}: does not enable `server`")
    return server_closure(reached, server, origin)


def doc_closure(
    names: set[str],
    umbrella: dict[str, list[str] | None],
    server: dict[str, list[str] | None],
) -> frozenset[str]:
    """The server surface the docs row promises.

    A name that is a fraiseql-server feature means that server feature, read
    directly. Reading it through the umbrella instead would let a broken
    pass-through (`federation` without `fraiseql-server?/federation`) shrink the
    promise itself, and the gate would then blame every recipe that kept it. Only a
    name the server lacks (`run-server`) is expanded through the umbrella.
    """
    direct = {n for n in names if server.get(n, None) is not None}
    rest = names - direct
    reached = umbrella_closure(rest | {"server"}, umbrella, server, DOC) if rest else frozenset()
    return server_closure(direct, server, DOC) | reached


# ── Recipes ──────────────────────────────────────────────────────────────────


def doc_features(path: Path) -> set[str]:
    if not path.is_file():
        die(f"{path} does not exist — it is the authority this gate compares against")
    rows = [line for line in path.read_text().splitlines() if line.startswith(DOC_ROW)]
    if len(rows) != 1:
        die(f"{path}: expected exactly one table row starting {DOC_ROW!r}, found {len(rows)}")
    cells = [cell.strip() for cell in rows[0].strip().strip("|").split("|")]
    if len(cells) <= 1:
        die(f"{path}: the {DOC_ROW!r} row has no Features cell")
    names = set(BACKTICKED.findall(cells[1])) - {BUNDLE}
    if not names:
        die(f"{path}: the {DOC_ROW!r} Features cell names no feature in backticks")
    return names


def logical_lines(text: str) -> list[tuple[int, str]]:
    """Lines with shell `\\` continuations joined, each with its first line number."""
    out: list[tuple[int, str]] = []
    start, parts = 0, []
    for lineno, line in enumerate(text.splitlines(), 1):
        if not parts:
            start = lineno
        if line.rstrip().endswith("\\"):
            parts.append(line.rstrip()[:-1])
            continue
        parts.append(line)
        out.append((start, " ".join(part.strip() for part in parts)))
        parts = []
    if parts:
        out.append((start, " ".join(part.strip() for part in parts)))
    return out


def workflow_builds(path: Path) -> list[tuple[int, set[str]]]:
    if not path.is_file():
        die(f"{path} does not exist")
    builds: list[tuple[int, set[str]]] = []
    for lineno, line in logical_lines(path.read_text()):
        if not SERVER_BUILD.search(line):
            continue
        flags = FEATURES_ARG.search(line)
        if flags is None:
            # A default-features build is a different artifact (the lean boot
            # smoke); it is not a platform recipe.
            continue
        if "${{" in flags.group(1):
            die(f"{path}:{lineno}: the build takes its features from an expression, "
                "which this gate cannot read")
        names = {n for n in flags.group(1).strip("\"'").split(",") if n}
        builds.append((lineno, names))
    if not builds:
        die(f"{path}: no `cargo build -p fraiseql-server --features …` found — "
            "the platform builds moved or were dropped")
    return builds


def image_features(repo: Path) -> set[str]:
    parity = _sibling("check-image-parity.py", "_fraiseql_image_parity")
    table = parity.parse_go_table(repo / ".dagger" / "image.go")
    if IMAGE_VARIANT not in table:
        die(f".dagger/image.go has no {IMAGE_VARIANT!r} variant")
    args = str(table[IMAGE_VARIANT]["buildArgs"])
    prefix = "CARGO_FEATURES="
    if not args.startswith(prefix) or " " in args:
        die(f".dagger/image.go: {IMAGE_VARIANT!r} buildArgs is {args!r}, "
            f"expected a single {prefix}<list>")
    return {n for n in args[len(prefix):].split(",") if n}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=REPO)
    args = parser.parse_args()
    repo = args.repo.resolve()

    umbrella = feature_table(repo / UMBRELLA_MANIFEST)
    server = feature_table(repo / SERVER_MANIFEST)

    promised = doc_closure(doc_features(repo / DOC), umbrella, server)
    recipes: list[tuple[str, frozenset[str]]] = [
        (f"{UMBRELLA_MANIFEST} `{BUNDLE}`",
         umbrella_closure({BUNDLE}, umbrella, server, UMBRELLA_MANIFEST)),
        (f".dagger/image.go `{IMAGE_VARIANT}`",
         server_closure(image_features(repo), server, ".dagger/image.go")),
    ]
    for workflow in WORKFLOWS:
        for lineno, names in workflow_builds(repo / workflow):
            origin = f"{workflow}:{lineno}"
            recipes.append((origin, server_closure(names, server, origin)))

    problems: list[str] = []
    for origin, compiled in recipes:
        missing = sorted(promised - compiled)
        extra = sorted(compiled - promised)
        if missing:
            problems.append(f"{origin} does not compile: {', '.join(missing)}")
        if extra:
            problems.append(f"{origin} compiles what {DOC} does not promise: {', '.join(extra)}")

    if problems:
        print("artifact feature parity FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        print(
            f"\nThe {DOC_ROW.strip('| ')} row of {DOC} is what adopters are told the\n"
            "platform artifacts carry. A recipe that drops a feature ships a binary or\n"
            "image without it (#1621); one that adds a feature ships it undocumented.",
            file=sys.stderr,
        )
        return 1

    print(f"OK: {len(recipes)} platform recipe(s) compile the {len(promised)} fraiseql-server "
          f"features {DOC} promises ({', '.join(sorted(promised))}).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
