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

## Deliberately not checked

Whether the registry actually holds the version. That needs the network, and this gate
runs in the Dagger ShellGates container, which has none. `tools/consume-published-artifacts.sh`
is the release-time check that a published artifact can be obtained.

Overrides, for testing:
  SDK_CLAIMS_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

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

    if not failed:
        print(f"OK: {len(sdks)} official SDKs; {sorted(bumped)} published and agreed "
              f"across release.sh, workflows and README; "
              f"{len(sdks) - len(bumped)} source-only with no publisher; "
              f"every publisher asserts its version.")
    return failed


if __name__ == "__main__":
    sys.exit(main())
