#!/usr/bin/env python3
"""Gate: every publishable workspace crate is actually published, in one agreed order.

The release ships crates.io artifacts from `.github/workflows/release.yml`, but the
publish *order* is authored twice — once as the ordered `cargo publish --package`
steps in that workflow, and once as `legacyPublishOrder` in `.dagger/release.go`,
which is what the pre-tag gate (`make release-validate`) dry-runs and topologically
self-tests. Nothing compared the two, and they drifted:

`fraiseql-cdc-sinks` (#382) was added to the workspace and to `fraiseql-server`'s
optional `cdc-outbound` dependency after v2.14.1. It reached `legacyPublishOrder`
but never reached `release.yml`. Because an optional dependency still has to resolve
on crates.io, that omission does not skip one crate — it fails `fraiseql-server`
outright ("no matching package named `fraiseql-cdc-sinks` found") and takes
`fraiseql-cli` and the `fraiseql` umbrella with it.

The pre-tag gate could not see it: `dry_run_failure_is_tolerable` forgives an
unresolved sibling *when the sibling is in the list it was handed*, and
`legacyPublishOrder` did contain it. So the dry-run went green on the promise that
the ordered publish would ship it first — a promise `release.yml` did not keep.

This gate is that missing comparison. It is pure text parsing: no cargo, no git, so
it runs in the toolchain-free ShellGates container.

Checks:
  1. The publishable workspace crates, `legacyPublishOrder`, the ordered publish
     steps, and every `CRATES=` list in release.yml all name the same set.
  2. The publish steps run in exactly `legacyPublishOrder`'s order — this is what
     transfers PublishOrderSelftest's topological proof onto what actually ships.
  3. Every published crate except the last is waited on by
     `tools/wait-for-crates-index.sh` after it is published.
  4. The publish-outcome roll-up names every publish step, so a failed publish
     cannot be reported as a success.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

def repo_root() -> Path:
    """The tree to check. argv[1] overrides it so the self-test can use fixtures."""
    if len(sys.argv) > 1:
        return Path(sys.argv[1]).resolve()
    return Path(__file__).resolve().parent.parent


def fail(msg: str) -> None:
    print(f"FAIL: {msg}", file=sys.stderr)


def section_of(line: str, current: str) -> str:
    """Track the current TOML table header; returns the new current section."""
    stripped = line.strip()
    if stripped.startswith("[") and not stripped.startswith("[["):
        return stripped
    if stripped.startswith("[["):
        return stripped
    return current


def workspace_members(manifest: Path) -> list[str]:
    """Quoted paths inside the root [workspace] members array."""
    text = manifest.read_text(encoding="utf-8")
    match = re.search(r"^\[workspace\]\s*$(.*?)^\[", text, re.S | re.M)
    block = match.group(1) if match else text
    members = re.search(r"members\s*=\s*\[(.*?)\]", block, re.S)
    if not members:
        return []
    return re.findall(r'"([^"]+)"', members.group(1))


def package_name_and_publishable(manifest: Path) -> tuple[str | None, bool]:
    """Read [package].name and whether the crate is publishable."""
    name: str | None = None
    publishable = True
    current = ""
    for line in manifest.read_text(encoding="utf-8").splitlines():
        current = section_of(line, current)
        if current != "[package]":
            continue
        if name is None:
            m = re.match(r'\s*name\s*=\s*"([^"]+)"', line)
            if m:
                name = m.group(1)
        m = re.match(r"\s*publish\s*=\s*(.+)", line)
        if m:
            value = m.group(1).split("#")[0].strip()
            publishable = value not in ("false", "[]")
    return name, publishable


def publishable_workspace_crates(root: Path) -> list[str]:
    found = []
    for member in workspace_members(root / "Cargo.toml"):
        manifest = root / member / "Cargo.toml"
        if not manifest.is_file():
            fail(f"workspace member {member!r} has no Cargo.toml at {manifest}")
            sys.exit(1)
        name, publishable = package_name_and_publishable(manifest)
        if name is None:
            fail(f"{manifest} has no [package] name")
            sys.exit(1)
        if publishable:
            found.append(name)
    return found


def legacy_publish_order(release_go: Path) -> list[str]:
    text = release_go.read_text(encoding="utf-8")
    match = re.search(r"legacyPublishOrder\s*=\s*\[\]string\{(.*?)\n\}", text, re.S)
    if not match:
        fail(f"could not find legacyPublishOrder in {release_go}")
        sys.exit(1)
    body = re.sub(r"//[^\n]*", "", match.group(1))
    return re.findall(r'"([^"]+)"', body)


def yml_publish_order(text: str) -> list[str]:
    return re.findall(r"cargo publish --package ([A-Za-z0-9_-]+)", text)


def yml_crates_lists(text: str) -> list[tuple[int, list[str]]]:
    """Every CRATES="..." assignment, with its 1-based line number.

    The value spans continuation lines, so read to the closing quote.
    """
    out: list[tuple[int, list[str]]] = []
    for match in re.finditer(r'CRATES="([^"]*)"', text, re.S):
        line_no = text.count("\n", 0, match.start()) + 1
        value = match.group(1).replace("\\\n", " ")
        out.append((line_no, value.split()))
    return out


def crate_publish_step_ids(text: str) -> dict[str, str]:
    """Map each published crate to the step id that publishes it.

    The owning step of a `cargo publish --package X` command is the one whose `id:`
    most recently precedes it, which pairs the two without assuming the id spells the
    crate name (`fraiseql` is published by `publish-root`).
    """
    ids = [(m.start(), m.group(1)) for m in re.finditer(r"id:\s*(publish-[A-Za-z0-9_-]+)", text)]
    out: dict[str, str] = {}
    for match in re.finditer(r"cargo publish --package ([A-Za-z0-9_-]+)", text):
        preceding = [step_id for pos, step_id in ids if pos < match.start()]
        if preceding:
            out[match.group(1)] = preceding[-1]
    return out


def yml_wait_lines(text: str) -> list[tuple[int, list[str]]]:
    """Every wait-for-crates-index.sh invocation, with its 1-based line number.

    The crate names are the bare trailing arguments. The leading version argument is
    always quoted (`"${GITHUB_REF#refs/tags/v}"`), so filtering to bare identifiers
    drops it without hardcoding a crate-name prefix.
    """
    out: list[tuple[int, list[str]]] = []
    for match in re.finditer(r"wait-for-crates-index\.sh([^\n]*)", text):
        line_no = text.count("\n", 0, match.start()) + 1
        names = [
            token
            for token in match.group(1).split()
            if re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]*", token)
        ]
        out.append((line_no, names))
    return out


def describe(label: str, got: set[str], want: set[str]) -> list[str]:
    problems = []
    missing = sorted(want - got)
    extra = sorted(got - want)
    if missing:
        problems.append(f"{label} is missing: {', '.join(missing)}")
    if extra:
        problems.append(f"{label} names non-publishable/unknown crates: {', '.join(extra)}")
    return problems


def main() -> int:
    root = repo_root()
    release_yml = root / ".github/workflows/release.yml"
    release_go = root / ".dagger/release.go"
    for path in (release_yml, release_go, root / "Cargo.toml"):
        if not path.is_file():
            fail(f"missing {path}")
            return 1

    text = release_yml.read_text(encoding="utf-8")
    crates = publishable_workspace_crates(root)
    want = set(crates)
    legacy = legacy_publish_order(release_go)
    published = yml_publish_order(text)

    problems: list[str] = []
    problems += describe(f"{release_go.name} legacyPublishOrder", set(legacy), want)
    problems += describe("release.yml publish steps", set(published), want)

    crates_lists = yml_crates_lists(text)
    if not crates_lists:
        problems.append('release.yml has no CRATES="..." list — the parser or the file changed')
    for line_no, names in crates_lists:
        problems += describe(f"release.yml CRATES list at line {line_no}", set(names), want)

    # Order parity: the ordered publish steps ARE legacyPublishOrder. PublishOrderSelftest
    # proves that order is topologically valid against the real dependency graph; this
    # equality is what carries that proof over to the job that actually publishes.
    if set(published) == set(legacy) and published != legacy:
        problems.append(
            "release.yml publishes in a different order than legacyPublishOrder:\n"
            f"    release.yml: {' -> '.join(published)}\n"
            f"    release.go : {' -> '.join(legacy)}"
        )

    # Index waits: a crate published without a later wait can be missing from the sparse
    # index when a dependent tries to resolve it. The final crate has no dependents.
    waits = yml_wait_lines(text)
    publish_lines = {
        name: text.count("\n", 0, m.start()) + 1
        for m in re.finditer(r"cargo publish --package ([A-Za-z0-9_-]+)", text)
        for name in [m.group(1)]
    }
    for name in published[:-1]:
        if not any(name in names and line_no > publish_lines[name] for line_no, names in waits):
            problems.append(
                f"{name} is published but never waited on by wait-for-crates-index.sh afterwards"
            )

    # The roll-up that decides whether the publish job succeeded must name every crate
    # step. Scope this to that one step: the human-readable summary below it lists the
    # same ids, and unioning the two would let the deciding list stay incomplete.
    # `publish-pypi` lives in another job and is deliberately not in scope.
    step_ids = crate_publish_step_ids(text)
    verify = re.search(
        r"- name: Verify all crates published\n(.*?)\n      - name: ", text, re.S
    )
    if not verify:
        problems.append("release.yml has no 'Verify all crates published' step")
    else:
        outcomes = set(re.findall(r"steps\.(publish-[A-Za-z0-9_-]+)\.outcome", verify.group(1)))
        unreported = sorted(set(step_ids.values()) - outcomes)
        if unreported:
            problems.append(
                "publish steps whose outcome the 'Verify all crates published' roll-up never "
                "checks (a failed publish would read as success): " + ", ".join(unreported)
            )

    if problems:
        print("Publish parity gate FAILED:\n", file=sys.stderr)
        for problem in problems:
            fail(problem)
        print(
            "\nEvery publishable workspace crate must appear, in one agreed order, in:\n"
            "  - .dagger/release.go        legacyPublishOrder\n"
            "  - .github/workflows/release.yml   the ordered `cargo publish --package` steps,\n"
            "                                   every CRATES= list, and an index wait\n"
            "A crate that is only in one of them passes the pre-tag dry-run and fails the\n"
            "real publish — see this file's docstring for how #382 did exactly that.",
            file=sys.stderr,
        )
        return 1

    print(
        f"OK: {len(crates)} publishable crates; release.yml and legacyPublishOrder agree "
        f"on membership and order, every crate but the last is index-waited, and all "
        f"{len(step_ids)} crate publish steps are outcome-checked."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
