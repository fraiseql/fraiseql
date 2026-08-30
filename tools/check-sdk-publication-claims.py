#!/usr/bin/env python3
"""The SDKs the repository says it publishes are exactly the ones it can publish.

Background (issue #1130). Eight of the eleven official SDKs sat frozen at 2.1.6 while
`README.md` advertised two of them — Java and Go — as **Tier 1 (Supported)**. None of the
eight had ever reached a registry (NuGet, Hex, RubyGems, Packagist and pub.dev all
answered 404), yet six carried a real tag-triggered publish step. `tools/release.sh` bumps
only the Rust, Python and TypeScript manifests, so a `csharp-sdk/v2.15.0` tag would have
pushed **2.1.6** to NuGet under a tag saying 2.15.0 — to a registry that does not allow
re-using a version number.

The founder's call (2026-08-30): publish Rust, Python and TypeScript; carry the other
eight as source-only. This gate is what stops the claim and the machinery drifting apart
again.

## What it checks

Three artifacts, none of which exists for this gate's benefit, must name the same set:

1. **`tools/release.sh`'s `RELEASE_FILES`** — whose manifests a release actually bumps.
2. **the publish jobs in `.github/workflows/`** — who can actually push to a registry.
3. **`README.md`'s SDK table** — what a reader is told.

Any one of the three drifting fails. That is the point: the previous arrangement failed
because the claim lived only in prose, where nothing could contradict it.

A fourth property is checked because it is the irreversible one: **every publisher must
assert its manifest matches the tag** (`assert_sdk_version_matches`). Three publishers
existed without it — `rust-sdk.yml`, `python-sdk.yml` and the community `ruby-sdk.yml`
(#1237) — so a direct SDK tag bypassed the H30 gate that release.yml applies.

And a fifth, which the first four could not see: **a published SDK must have a publisher
that the release tag actually reaches.** Agreement between the three artifacts says an
SDK is meant to be published; it does not say a release publishes it. The Rust SDK passed
all four checks above while its only publisher fired on a `rust-sdk/v*` tag that
`tools/release.sh` does not create and that has never existed, so `fraiseql-rust` was 404
on crates.io under a README row naming crates.io as its registry — #1130's own shape,
inside #1130's fix.

Reachability is decided by GitHub's documented ref-filter rules, not by the job existing:

* a `push:` block with `branches` and no `tags` does not run on a tag push at all — the
  ref filter and the path filter are ANDed and a tag matches no branch pattern (#1119);
* `paths` filters are NOT evaluated for tag pushes, so a `push: paths:` block with
  neither `branches` nor `tags` runs on every tag;
* a job's `if` may still exclude the release tag by ref prefix.

The check demands POSITIVE proof: an `if` this gate cannot evaluate is not counted as a
reachable publisher, so a shape it does not understand fails loudly instead of passing.

## Deliberately not checked

Whether the registry actually holds the version. That needs the network, and this gate
runs in the Dagger ShellGates container, which has none. `tools/consume-published-artifacts.sh`
is the release-time check that a published artifact can be obtained.

Overrides, for testing:
  SDK_CLAIMS_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

import fnmatch
import importlib.util
import os
import re
import subprocess
import sys
from pathlib import Path

NOT_AN_SDK = {"conformance", "tests"}

# A step that pushes to a public registry.
#
# A `--dry-run` variant is deliberately NOT excluded. It reads like something that
# should be, but a dry-run-only publish job is itself the defect (#1223: a job named
# "Publish to pub.dev" whose only publish step was `--dry-run`, reporting success for
# a release that did not happen). Treating it as a publisher makes the gate demand
# that it be backed by a bump-set entry and a README row, which is the question worth
# asking about it.
PUBLISH_STEP = re.compile(
    r"gem push|npm publish|uv publish|twine upload|dotnet nuget push|"
    r"mix hex\.publish|mvn -B deploy|dart pub publish|cargo publish"
)


def root() -> Path:
    if "SDK_CLAIMS_ROOT" in os.environ:
        return Path(os.environ["SDK_CLAIMS_ROOT"]).resolve()
    out = subprocess.run(["git", "rev-parse", "--show-toplevel"],
                         capture_output=True, text=True, check=True)
    return Path(out.stdout.strip())


ROOT = root()


def official_sdks() -> set[str]:
    """Directory names under sdks/official/, minus the shared harnesses."""
    base = ROOT / "sdks" / "official"
    if not base.is_dir():
        sys.exit(f"ERROR: {base} not found — this gate cannot discover its subjects "
                 "and is refusing to pass vacuously.")
    names = {d.name.removeprefix("fraiseql-") for d in base.iterdir()
             if d.is_dir() and d.name not in NOT_AN_SDK}
    if not names:
        sys.exit(f"ERROR: no SDK directories under {base} — refusing to pass vacuously.")
    return names


def bumped_sdks() -> set[str]:
    """SDKs whose manifest tools/release.sh bumps."""
    text = (ROOT / "tools" / "release.sh").read_text()
    m = re.search(r"RELEASE_FILES=\((.*?)\n\)", text, re.S)
    if not m:
        sys.exit("ERROR: no RELEASE_FILES array in tools/release.sh — refusing to pass "
                 "vacuously.")
    return set(re.findall(r"sdks/official/fraiseql-([a-z0-9]+)/", m.group(1)))


def publishing_workflows() -> dict[str, list[Path]]:
    """SDK → the workflow files that can push it to a registry."""
    found: dict[str, list[Path]] = {}
    for wf in sorted((ROOT / ".github" / "workflows").glob("*.yml")):
        text = wf.read_text()
        # Only look at text inside a job, so a comment at the top of the file
        # explaining that publishing happens elsewhere is not read as publishing.
        if not PUBLISH_STEP.search(re.sub(r"^\s*#.*$", "", text, flags=re.M)):
            continue
        for sdk in set(re.findall(r"sdks/(?:official|community)/fraiseql-([a-z0-9]+)", text)):
            if wf not in found.setdefault(sdk, []):
                found[sdk].append(wf)
    return found


def readme_published() -> set[str]:
    """SDKs the README's table marks as published to a registry.

    The table is read rather than a hand-kept list, so the prose a reader sees is the
    thing under test. Rows are `| Name | ... | registry-or-dash | ...`.
    """
    text = (ROOT / "README.md").read_text()
    m = re.search(r"<!-- sdk-table:start -->(.*?)<!-- sdk-table:end -->", text, re.S)
    if not m:
        sys.exit("ERROR: README.md has no `<!-- sdk-table:start -->` marker. The SDK "
                 "publication table is what this gate compares against; without it the "
                 "gate would pass over whatever the README happens to say.")
    published = set()
    for line in m.group(1).splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 3 or cells[0].lower() in ("sdk", "") or set(cells[0]) <= set("-: "):
            continue
        if cells[2] and cells[2] not in ("—", "-", "source-only"):
            published.add(cells[0].strip("`").removeprefix("fraiseql-").lower())
    return published


def ungated_publishers(pub: dict[str, list[Path]]) -> list[tuple[str, Path]]:
    """Publishers that do not assert the manifest version matches the tag."""
    bad = []
    for sdk, files in sorted(pub.items()):
        for wf in files:
            if "assert_sdk_version_matches" not in wf.read_text():
                bad.append((sdk, wf))
    return bad



def _yaml_module():
    """`parse_yaml` from tools/check-suite-coverage.py.

    One hand-written YAML-subset parser serves the gates that need one. The ShellGates
    container is bare Ubuntu plus python3 — no PyYAML — and a gate that skipped when its
    parser was missing would pass vacuously, so an unloadable sibling is fatal.
    """
    path = ROOT / "tools" / "check-suite-coverage.py"
    spec = importlib.util.spec_from_file_location("_fraiseql_suite_coverage", path)
    if spec is None or spec.loader is None:
        sys.exit(f"ERROR: cannot load the YAML parser from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def release_tag() -> str:
    """A concrete instance of the tag `tools/release.sh` creates, e.g. `v9.9.9`.

    Read rather than hardcoded: if the release ever tags something else, this gate must
    re-decide reachability against the new shape instead of checking a stale constant.
    """
    text = (ROOT / "tools" / "release.sh").read_text()
    m = re.search(r'git tag -a "([^"]+)"', text)
    if not m:
        sys.exit("ERROR: no `git tag -a \"...\"` in tools/release.sh — this gate cannot "
                 "learn which tag a release creates and is refusing to pass vacuously.")
    return re.sub(r"\$\{[A-Za-z_][A-Za-z0-9_]*\}", "9.9.9", m.group(1))


def _tag_push_runs_workflow(on: object, tag: str) -> bool:
    """Does pushing `refs/tags/<tag>` start this workflow?"""
    if not isinstance(on, dict):
        return False
    push = on.get("push")
    if not isinstance(push, dict):
        return False
    has_branches = "branches" in push or "branches-ignore" in push
    has_tags = "tags" in push or "tags-ignore" in push
    # #1119: GitHub ANDs the ref filter with the path filter. With `branches` present and
    # `tags` absent, a tag push matches no ref pattern and the workflow never starts.
    if has_branches and not has_tags:
        return False
    for pattern in push.get("tags-ignore") or []:
        if fnmatch.fnmatch(tag, str(pattern)):
            return False
    include = push.get("tags")
    if include is not None:
        return any(fnmatch.fnmatch(tag, str(pattern)) for pattern in include)
    # Neither filter: every ref matches. `paths` is deliberately not consulted — GitHub
    # does not evaluate path filters for tag pushes, which is why the six publishers
    # ADR-0019 deleted were reachable at all.
    return True


# The two `if` shapes this repository's publish jobs use. Anything else is not proof.
_REF_PREFIX = re.compile(r"startsWith\(\s*github\.ref\s*,\s*'refs/tags/([^']*)'\s*\)")
_EVENT_NAME = re.compile(r"github\.event_name\s*==\s*'([a-z_]+)'")


def _job_if_admits_tag_push(condition: object, tag: str) -> bool:
    """Can this job's `if` be true for a push of `refs/tags/<tag>`?

    Returns False for any expression this gate cannot evaluate. The caller needs positive
    proof that a publisher runs, so "not understood" must read as "not proven", never as
    "probably fine" — that is the difference between this check and the claim it replaces.
    """
    if condition is None:
        return True
    text = str(condition)
    for event in _EVENT_NAME.findall(text):
        if event != "push":
            return False
    prefixes = _REF_PREFIX.findall(text)
    if prefixes and not any(f"refs/tags/{tag}".startswith(f"refs/tags/{p}") for p in prefixes):
        return False
    # Only conjunctions of the two understood forms count. An `||`, a `contains()`, or
    # anything else leaves the job unproven.
    residue = _REF_PREFIX.sub("X", _EVENT_NAME.sub("X", text))
    if not re.fullmatch(r"[\sX]*(?:&&[\sX]*)*", residue):
        return False
    return True


def _step_text(step: object) -> str:
    if not isinstance(step, dict):
        return ""
    return " ".join(str(v) for v in step.values())


def release_reachable_publishers(tag: str) -> dict[str, list[str]]:
    """SDK → the `<workflow>:<job>` publishers a push of `<tag>` provably runs."""
    yaml = _yaml_module()
    found: dict[str, list[str]] = {}
    for wf in sorted((ROOT / ".github" / "workflows").glob("*.yml")):
        try:
            doc = yaml.parse_yaml(wf.read_text())
        except Exception as exc:  # noqa: BLE001 — an unparsable workflow must not pass
            sys.exit(f"ERROR: cannot parse {wf.relative_to(ROOT)}: {exc}")
        if not isinstance(doc, dict) or not _tag_push_runs_workflow(doc.get("on"), tag):
            continue
        for job_name, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            body = " ".join(_step_text(s) for s in (job.get("steps") or []))
            if not PUBLISH_STEP.search(body):
                continue
            if not _job_if_admits_tag_push(job.get("if"), tag):
                continue
            for sdk in set(re.findall(r"sdks/(?:official|community)/fraiseql-([a-z0-9]+)", body)):
                found.setdefault(sdk, []).append(f"{wf.name}:{job_name}")
    return found


def main() -> int:
    sdks = official_sdks()
    bumped = bumped_sdks()
    pub = publishing_workflows()
    claimed = readme_published()
    publishable = set(pub)
    failed = 0

    for name, got in (("bumped by release.sh", bumped),
                      ("claimed published by README.md", claimed)):
        unknown = got - sdks
        if unknown:
            print(f"ERROR: {name} names non-existent SDK(s): {sorted(unknown)}",
                  file=sys.stderr)
            failed = 1

    if bumped != claimed or bumped != publishable:
        print("ERROR: the three artifacts disagree about which SDKs are published.\n",
              file=sys.stderr)
        for label, got in (("tools/release.sh bumps    ", bumped),
                           ("workflows can publish     ", publishable),
                           ("README.md claims published", claimed)):
            print(f"  {label}: {sorted(got) or '(none)'}", file=sys.stderr)
        print("\nAn SDK that is published must be in all three: its manifest bumped in "
              "lockstep\nwith the release, a publish job that can push it, and a README "
              "row saying so.\nAn SDK that is not published must be in none of them — a "
              "publish job for an SDK\nno release bumps pushes a stale version to a "
              "registry that will not take it back\n(#1130).", file=sys.stderr)
        failed = 1

    ungated = ungated_publishers(pub)
    if ungated:
        print("\nERROR: publisher(s) that do not assert the manifest matches the tag:",
              file=sys.stderr)
        for sdk, wf in ungated:
            print(f"  {sdk}: {wf.relative_to(ROOT)}", file=sys.stderr)
        print("\nCall `assert_sdk_version_matches` (tools/lib/release_helpers.sh) before "
              "the\npublish step. Without it a tag publishes whatever the manifest holds "
              "— which is\nhow eight SDKs sat at 2.1.6 behind green checkmarks (#1130).",
              file=sys.stderr)
        failed = 1

    tag = release_tag()
    reachable = release_reachable_publishers(tag)
    stranded = sorted(sdk for sdk in bumped if not reachable.get(sdk))
    if stranded:
        print(f"\nERROR: published SDK(s) with no publisher a `{tag}` tag reaches:",
              file=sys.stderr)
        for sdk in stranded:
            have = [f"{wf.name}" for wf in pub.get(sdk, [])] or ["(none)"]
            print(f"  {sdk}: publish step(s) live in {', '.join(have)}; no job there "
                  f"runs on that tag", file=sys.stderr)
        print("\n`tools/release.sh` creates one tag and only that tag fires the release "
              "pipeline.\nA publisher gated on a `<sdk>/v*` tag nobody pushes leaves the "
              "SDK at 404 while\nrelease.sh bumps its manifest and the README names its "
              "registry — the Rust SDK sat\nexactly there. Publish it from a job the "
              "release tag runs, or move the SDK to\nsource-only in all three artifacts "
              "(#1130, ADR-0019).", file=sys.stderr)
        failed = 1

    if not failed:
        lockstep = ", ".join(f"{sdk} via {'/'.join(reachable[sdk])}" for sdk in sorted(bumped))
        print(f"OK: {len(sdks)} official SDKs; {sorted(bumped)} published and agreed "
              f"across release.sh, workflows and README; "
              f"{len(sdks) - len(bumped)} source-only with no publisher; "
              f"every publisher asserts its version; "
              f"every published SDK ships on the `{tag}` release tag ({lockstep}).")
    return failed


if __name__ == "__main__":
    sys.exit(main())
