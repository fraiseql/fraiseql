#!/usr/bin/env python3
"""Assert the Dagger image leg builds exactly the images `docker-build.yml` publishes.

`.dagger/image.go` carries a table of image variants, and `docker-build.yml`
carries two build matrices — one publishing to ghcr.io, one to Docker Hub. Three
copies of the same list. #1135's lesson, one leg over: a list maintained by hand
in a second place drifts silently, and here the drift is invisible in the
direction that matters most — a variant published by the tag workflow but absent
from the pre-tag leg is an image that ships having never been built by any gate.
That is the exact hole #1205 fell through.

So the comparison is **bidirectional across all three**:

  * a variant published by either matrix and missing from the Dagger table fails
    (the pre-tag leg would not build something we ship);
  * a variant in the Dagger table that neither matrix publishes fails (the leg
    would spend runner minutes on an artifact nobody ships, and the table would
    stop being a description of what we ship);
  * the two matrices must agree with each other (a variant added to one alone
    ships to one registry and not the other);
  * and for each variant, `dockerfile`, `build-context` and `build-args` must be
    identical everywhere — a leg that builds the right NAME with the wrong
    Dockerfile or without `CARGO_FEATURES` proves nothing about the image that
    ships.

A row this parser cannot read is FATAL rather than skipped. A parity gate that
silently drops what it does not understand reports a parity it never checked.

Runs in preflight and ShellGates (python3, stdlib only — it does NOT build an
image and so is safe there; the build itself belongs on the heavy trigger).
Locally: `make lint-image-parity`. Its red capability is pinned by
`tools/tests/image_parity_test.sh`.
"""

from __future__ import annotations

import argparse
import importlib.util
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# The workflow keys that must agree, mapped to the Go struct field carrying them.
# `optional` is deliberately absent: only the ghcr matrix declares it, so it is
# compared separately against that matrix alone.
COMPARED = {
    "dockerfile": "dockerfile",
    "build-context": "buildContext",
    "build-args": "buildArgs",
}


def die(message: str) -> None:
    print(f"FATAL: {message}", file=sys.stderr)
    raise SystemExit(2)


def _yaml_module():
    """`parse_yaml` / `YamlError` from tools/check-suite-coverage.py.

    Imported rather than duplicated: the ShellGates container is bare Ubuntu plus
    python3 — no PyYAML, no pip step — so the alternative is a third copy of a
    hand-written parser, in a gate whose entire subject is copies drifting. A
    missing or unloadable sibling is FATAL, never a skip.
    """
    path = REPO / "tools" / "check-suite-coverage.py"
    spec = importlib.util.spec_from_file_location("_fraiseql_suite_coverage", path)
    if spec is None or spec.loader is None:
        die(f"cannot load the YAML parser from {path}")
    module = importlib.util.module_from_spec(spec)
    # Registered before execution: `@dataclass` in an imported module resolves its
    # own `sys.modules[__module__]`, and an unregistered module makes that lookup
    # return None (AttributeError on 3.14, not an import error naming the cause).
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


# ── The Go table ─────────────────────────────────────────────────────────────

TABLE_START = re.compile(r"^var imageVariants = \[\]imageVariant\{$")
ROW = re.compile(
    r'^\s*\{name:\s*"(?P<name>[^"]*)",\s*'
    r'dockerfile:\s*"(?P<dockerfile>[^"]*)",\s*'
    r'buildContext:\s*"(?P<buildContext>[^"]*)",\s*'
    r'buildArgs:\s*"(?P<buildArgs>[^"]*)",\s*'
    r"optional:\s*(?P<optional>true|false)\},\s*$"
)


def parse_go_table(path: Path) -> dict[str, dict[str, str | bool]]:
    """Read `imageVariants` out of .dagger/image.go as source text.

    Source text rather than a Go toolchain because this gate runs in a container
    that has neither Go nor `go list`. That makes the literal's SHAPE load-bearing,
    which is why an unreadable row is fatal and why image.go says so at the table.
    """
    if not path.is_file():
        die(f"{path} does not exist — the Dagger image leg is the thing being compared")

    lines = path.read_text().splitlines()
    starts = [i for i, line in enumerate(lines) if TABLE_START.match(line)]
    if len(starts) != 1:
        die(f"{path}: expected exactly one `var imageVariants = []imageVariant{{`, found {len(starts)}")

    variants: dict[str, dict[str, str | bool]] = {}
    for lineno in range(starts[0] + 1, len(lines)):
        line = lines[lineno]
        if line.startswith("}"):
            break
        if not line.strip() or line.strip().startswith("//"):
            continue
        match = ROW.match(line)
        if not match:
            die(
                f"{path}:{lineno + 1}: cannot read this imageVariants row.\n"
                f"    {line.strip()}\n"
                "    Expected: {name: \"…\", dockerfile: \"…\", buildContext: \"…\", "
                "buildArgs: \"…\", optional: true|false},\n"
                "    This gate reads the table as text (no Go toolchain in the gate "
                "container), so the field order and quoting are part of the contract."
            )
        row = match.groupdict()
        name = row.pop("name")
        if name in variants:
            die(f"{path}: variant {name!r} is listed twice")
        variants[name] = {**row, "optional": row.pop("optional") == "true"}
    else:
        die(f"{path}: the imageVariants table is not closed by a line starting with `}}`")

    if not variants:
        die(f"{path}: the imageVariants table is empty — a leg that builds nothing passes everything")
    return variants


# ── The workflow matrices ────────────────────────────────────────────────────


def parse_matrix(doc: dict, job: str, path: Path) -> dict[str, dict[str, str | bool]]:
    jobs = doc.get("jobs")
    if not isinstance(jobs, dict) or job not in jobs:
        die(f"{path}: no job {job!r} — this gate's premise is that it publishes images")
    try:
        rows = jobs[job]["strategy"]["matrix"]["service"]
    except (KeyError, TypeError):
        die(f"{path}: job {job!r} has no strategy.matrix.service")
    if not isinstance(rows, list) or not rows:
        die(f"{path}: job {job!r} has an empty or unreadable matrix")

    variants: dict[str, dict[str, str | bool]] = {}
    for row in rows:
        if not isinstance(row, dict) or "name" not in row:
            die(f"{path}: job {job!r} has a matrix entry with no `name`: {row!r}")
        name = row["name"]
        if name in variants:
            die(f"{path}: job {job!r} lists variant {name!r} twice")
        variants[name] = {
            "dockerfile": str(row.get("dockerfile", "")),
            "buildContext": str(row.get("build-context", "")),
            "buildArgs": str(row.get("build-args", "")),
            "optional": bool(row.get("optional", False)),
        }
    return variants


def compare(
    label_a: str, a: dict, label_b: str, b: dict, keys: list[str], *, membership: bool = True
) -> list[str]:
    """Every difference between two variant tables, in both directions.

    `membership=False` compares only the values of variants present in both. The
    `optional` field lives on one matrix alone, so it is compared in a second pass
    over the same pair — and without this the missing-variant finding would be
    reported by both passes, which reads as two problems and erodes the report.
    """
    problems: list[str] = []
    if membership:
        for name in sorted(set(a) - set(b)):
            problems.append(f"{name!r} is in {label_a} but not in {label_b}")
        for name in sorted(set(b) - set(a)):
            problems.append(f"{name!r} is in {label_b} but not in {label_a}")
    for name in sorted(set(a) & set(b)):
        for key in keys:
            if a[name][key] != b[name][key]:
                problems.append(
                    f"{name!r}: {key} is {a[name][key]!r} in {label_a} "
                    f"but {b[name][key]!r} in {label_b}"
                )
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=REPO)
    args = parser.parse_args()
    repo = args.repo.resolve()

    go_path = repo / ".dagger" / "image.go"
    wf_path = repo / ".github" / "workflows" / "docker-build.yml"

    yaml = _yaml_module()
    if not wf_path.is_file():
        die(f"{wf_path} does not exist — it is the authority this gate compares against")
    try:
        doc = yaml.parse_yaml(wf_path.read_text())
    except yaml.YamlError as exc:
        die(f"{wf_path}: {exc}")

    go = parse_go_table(go_path)
    ghcr = parse_matrix(doc, "build-and-push", wf_path)
    hub = parse_matrix(doc, "publish-to-docker-hub", wf_path)

    fields = list(COMPARED.values())
    problems: list[str] = []
    problems += compare("docker-build.yml build-and-push", ghcr,
                        "docker-build.yml publish-to-docker-hub", hub, fields)
    problems += compare("docker-build.yml build-and-push", ghcr,
                        ".dagger/image.go", go, fields)
    # `optional` exists only on the ghcr matrix, so it is compared there alone.
    problems += compare("docker-build.yml build-and-push", ghcr,
                        ".dagger/image.go", go, ["optional"], membership=False)

    if problems:
        print("image parity FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        print(
            "\nThe Dagger leg builds before the tag; docker-build.yml builds after it.\n"
            "A variant they disagree about is one that ships without ever having been\n"
            "built by a gate (#1205), or a leg spending runner minutes on nothing.",
            file=sys.stderr,
        )
        return 1

    names = ", ".join(sorted(go))
    print(f"OK: {len(go)} image variant(s) agree across both docker-build.yml "
          f"matrices and .dagger/image.go ({names}).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
