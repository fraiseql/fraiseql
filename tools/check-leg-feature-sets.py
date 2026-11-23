#!/usr/bin/env python3
"""Each integration leg builds a package with one feature list (#1886, #1902).

Every distinct `--features` list a leg gives a package is another build of it, and of
`fraiseql-core` and `fraiseql-db` beneath it: a feature or dev-dependency difference
anywhere below changes their build too. `integration (postgres)` named ten
`fraiseql-server` lists, run one after another, and compiled `fraiseql-core` eleven
ways; a runner that had not built the tree before could not finish the leg (#1906).

The rule, per leg and per package:

* Every cargo line without an annotation uses the same feature list: the leg's set.
* A line that needs features OFF carries, in the comment block directly above it,
  one `// feature-off: <flag> — <suite>` per flag. The annotation is checked where it
  stands, not counted:
  - `<flag>` is a feature of the package, ON in the leg's set and OFF on this line;
  - the line's list is the leg's set minus every feature that turns an annotated
    flag on, and nothing else;
  - `<suite>` exists (a `src/` module path for a `--lib` line, a `tests/` target for
    a `--test` line), is selected by this line's own filter or `--test`, and its
    source names `feature = "<flag>"`: the arm the line keeps running.
  An annotation whose suite was removed or renamed, or that sits on a line building
  the leg's set, or above no cargo line, fails.

It also holds `serverUnifiedFeatures` to the lists it is built from (`COVERS`), so a feature
added to either reaches both legs.

Runs in preflight ShellGates and locally via `make lint-leg-feature-sets`. Its red
capability is pinned by `tools/tests/leg_feature_sets_test.sh`. `LEG_FEATURE_SETS_ROOT`
points it at another tree (the test's fixtures).
"""

from __future__ import annotations

import os
import re
import shlex
import sys
import tomllib
from pathlib import Path

ROOT = Path(
    os.environ.get("LEG_FEATURE_SETS_ROOT") or Path(__file__).resolve().parent.parent
)
MAIN_GO = ROOT / ".dagger" / "main.go"

LEGS = ("integrationPostgres", "integrationObservers")
COVERS = {
    "serverUnifiedFeatures": ("serverTestFeatures", "capabilityFixtureExtraFeatures")
}

CONST = re.compile(r'^\s*([A-Za-z]\w*)\s*=\s*"([^"]*)"\s*$', re.MULTILINE)
COMPUTED = re.compile(
    r'(\w+) := "([^"]*)"\s*\n\s*for _, f := range strings\.Split\(([\w+",]+?), ","\) \{\s*\n'
    r'\s*\1 \+= ",([\w-]+/)" \+ f'
)
ANNOTATION = re.compile(r"^//\s*feature-off:\s*([\w-]+)\s+(?:—|--|-)\s+(\S+)\s*$")
ALL_FEATURES = "<all-features>"


class Line:
    def __init__(
        self, leg: str, lineno: int, command: str, annotations: list[tuple[str, str]]
    ):
        self.leg, self.lineno, self.command, self.annotations = (
            leg,
            lineno,
            command,
            annotations,
        )
        argv = shlex.split(command)
        cut = argv.index("--") if "--" in argv else len(argv)
        self.args, self.post = argv[:cut], argv[cut + 1 :]
        packages = [argv[i + 1] for i, a in enumerate(self.args) if a == "-p"]
        if len(packages) != 1:
            fatal(f"{self.where}: one `-p` per line, found {packages or 'none'}")
        self.package = packages[0]
        self.features = self._features()
        self.tests = [
            self.args[i + 1] for i, a in enumerate(self.args) if a == "--test"
        ]
        self.lib = "--lib" in self.args
        # libtest filters: before `--`, the positional after `--lib`; after it, every
        # argument that is not an option.
        positional = [
            a
            for i, a in enumerate(self.args[1:], 1)
            if not a.startswith("-")
            and self.args[i - 1] not in ("-p", "--features", "--test")
        ]
        self.filters = positional[1:] + [a for a in self.post if not a.startswith("-")]

    @property
    def where(self) -> str:
        return f".dagger/main.go:{self.lineno} ({self.leg})"

    def _features(self) -> frozenset[str]:
        if "--all-features" in self.args:
            return frozenset({ALL_FEATURES})
        lists = [self.args[i + 1] for i, a in enumerate(self.args) if a == "--features"]
        return frozenset(f for lst in lists for f in re.split(r"[ ,]+", lst) if f)

    def selects(self, suite: str) -> bool:
        if suite in self.tests:
            return True
        if not self.lib:
            return False
        # libtest matches a filter as a substring of the test path; a filter naming the
        # suite's module, or a module above it, selects every test in the suite.
        path = suite.removesuffix("::").split("::")
        return any(
            f.removesuffix("::").split("::")
            == path[: len(f.removesuffix("::").split("::"))]
            for f in self.filters
        )


def fatal(message: str) -> None:
    print(f"leg-feature-sets: FATAL — {message}", file=sys.stderr)
    sys.exit(2)


def leg_lines(
    src: str, consts: dict[str, str], leg: str
) -> tuple[list[Line], list[str]]:
    """The leg's cargo lines, and the annotations that stand above no cargo line."""
    m = re.search(
        rf"^func \(m \*FraiseqlCi\) {leg}\(.*?^}}", src, re.DOTALL | re.MULTILINE
    )
    if not m:
        fatal(f"no `{leg}` function in .dagger/main.go")
    body, base = m.group(0), src[: m.start()].count("\n") + 1
    local = dict(consts)
    for cm in COMPUTED.finditer(body):
        names = re.split(r'\+","\+', cm.group(3))
        missing = [n for n in names if n not in local]
        if missing:
            fatal(f"{leg}: `{cm.group(1)}` splits unknown consts {missing}")
        feats = ",".join(local[n] for n in names).split(",")
        local[cm.group(1)] = cm.group(2) + "".join(f",{cm.group(4)}{f}" for f in feats)

    lines: list[Line] = []
    dangling: list[str] = []
    pending: list[tuple[str, str, int]] = []

    def strand() -> None:
        dangling.extend(
            f".dagger/main.go:{n} ({leg}): `feature-off: {f} — {s}` stands above "
            "no cargo line"
            for f, s, n in pending
        )

    for offset, raw in enumerate(body.splitlines()):
        text = raw.strip()
        lineno = base + offset
        if text.startswith("//"):
            am = ANNOTATION.match(text)
            if am:
                pending.append((am.group(1), am.group(2), lineno))
            elif "feature-off" in text:
                fatal(
                    f".dagger/main.go:{lineno}: malformed annotation, expected "
                    "`// feature-off: <flag> — <suite>`: {text}"
                )
            continue
        if not text.startswith('"'):
            if pending and text:
                strand()
                pending = []
            continue
        command = re.sub(r",\s*//.*$", "", text).rstrip(",")

        for name in re.findall(r'"\s*\+\s*(\w+)\s*\+\s*"', command):
            if name not in local:
                fatal(f".dagger/main.go:{lineno}: cannot resolve `{name}`")
        command = re.sub(
            r'"\s*\+\s*(\w+)\s*\+\s*"', lambda mm: local[mm.group(1)], command
        )
        if not command.startswith('"') or not command.endswith('"'):
            fatal(f".dagger/main.go:{lineno}: cannot read `{text}`")
        command = command[1:-1]
        command = command.removeprefix("bash tools/ci-target-canary.sh -- ")
        if command.startswith(("cargo test ", "cargo build ", "test ")):
            if not command.startswith("cargo "):
                command = "cargo " + command
            lines.append(Line(leg, lineno, command, [(f, s) for f, s, _ in pending]))
        else:
            strand()
        pending = []
    strand()
    return lines, dangling


def package_features(package: str) -> dict[str, list[str]]:
    manifest = ROOT / "crates" / package / "Cargo.toml"
    if not manifest.is_file():
        fatal(f"no manifest for `{package}` at crates/{package}/Cargo.toml")
    return tomllib.loads(manifest.read_text()).get("features", {})


def closure(table: dict[str, list[str]], feature: str) -> set[str]:
    seen: set[str] = set()
    stack = [feature]
    while stack:
        f = stack.pop()
        if f in seen or f not in table:
            continue
        seen.add(f)
        stack += [d for d in table[f] if "/" not in d and not d.startswith("dep:")]
    return seen


def suite_source(package: str, suite: str) -> Path | None:
    crate = ROOT / "crates" / package
    if "::" in suite.removesuffix("::") or suite.endswith("::"):
        parts = suite.removesuffix("::").split("::")
        for candidate in (
            crate / "src" / Path(*parts).with_suffix(".rs"),
            crate / "src" / Path(*parts) / "mod.rs",
        ):
            if candidate.is_file():
                return candidate
        return None
    candidate = crate / "tests" / f"{suite}.rs"
    return candidate if candidate.is_file() else None


def check_annotated(line: Line, leg_set: frozenset[str], errors: list[str]) -> None:
    table = package_features(line.package)
    off = {flag for flag, _ in line.annotations}
    for flag, suite in line.annotations:
        tag = f"{line.where}: `feature-off: {flag} — {suite}`"
        if flag not in table:
            errors.append(f"{tag}: `{flag}` is not a feature of {line.package}")
            continue
        if not any(flag in closure(table, f) for f in leg_set):
            errors.append(
                f"{tag}: `{flag}` is not ON in the leg's set, so nothing is turned off"
            )
        if any(flag in closure(table, f) for f in line.features):
            errors.append(f"{tag}: the line still turns `{flag}` on")
        source = suite_source(line.package, suite)
        if source is None:
            errors.append(f"{tag}: no suite `{suite}` in crates/{line.package}")
        elif f'feature = "{flag}"' not in source.read_text():
            errors.append(
                f'{tag}: {source.relative_to(ROOT)} has no `feature = "{flag}"` arm'
            )
        if not line.selects(suite):
            errors.append(
                f"{tag}: the line does not select `{suite}` (filters: "
                f"{line.filters or 'none'}, tests: {line.tests or 'none'})"
            )
    expected = frozenset(f for f in leg_set if not (closure(table, f) & off))
    if line.features != expected:
        extra = sorted(line.features - expected)
        missing = sorted(expected - line.features)
        errors.append(
            f"{line.where}: the line's set must be the leg's minus what turns "
            f"{sorted(off)} on; differs by +{extra} -{missing}"
        )


def main() -> int:
    if not MAIN_GO.is_file():
        fatal(f"{MAIN_GO} not found")
    src = MAIN_GO.read_text()
    consts = dict(CONST.findall(src))
    errors: list[str] = []

    for name, sources in COVERS.items():
        if name not in consts:
            errors.append(f"`{name}` is not a const of .dagger/main.go")
            continue
        have = set(consts[name].split(","))
        for source in sources:
            missing = sorted(set(consts.get(source, "").split(",")) - have - {""})
            if source not in consts:
                errors.append(f"`{source}` is not a const of .dagger/main.go")
            elif missing:
                errors.append(f"`{name}` lacks {missing} from `{source}`")

    checked = 0
    for leg in LEGS:
        lines, dangling = leg_lines(src, consts, leg)
        errors += dangling
        if not lines:
            fatal(
                f"{leg}: no cargo line read; a gate that reads nothing passes everything"
            )
        by_package: dict[str, list[Line]] = {}
        for ln in lines:
            by_package.setdefault(ln.package, []).append(ln)
        for package, plines in sorted(by_package.items()):
            plain = {ln.features for ln in plines if not ln.annotations}
            if len(plain) > 1:
                shown = "\n".join(
                    f"    {ln.where}: {','.join(sorted(ln.features)) or '(default)'}"
                    for ln in plines
                    if not ln.annotations
                )
                errors.append(
                    f"{leg}: `{package}` is built with {len(plain)} feature lists; "
                    f"use the leg's one set, or annotate a feature-off line:\n{shown}"
                )
                continue
            if not plain:
                errors.append(
                    f"{leg}: every `{package}` line is annotated; the leg has no set "
                    "to turn features off from"
                )
                continue
            leg_set = next(iter(plain))
            for ln in plines:
                if not ln.annotations:
                    continue
                if ln.features == leg_set:
                    errors.append(
                        f"{ln.where}: annotated feature-off, but builds the leg's set"
                    )
                check_annotated(ln, leg_set, errors)
            checked += len(plines)

    if errors:
        print("leg-feature-sets: FAIL", file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        return 1
    print(
        f"leg-feature-sets: OK — {checked} cargo lines in {len(LEGS)} legs, one feature "
        "list per package, every feature-off line holds"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
