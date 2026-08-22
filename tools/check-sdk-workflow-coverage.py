#!/usr/bin/env python3
"""Every official SDK must be gated by a workflow that runs on a BRANCH push.

`sdk-conformance.yml` runs on every push and is a real gate — it compiles what each
SDK authors, so an SDK cannot silently stop producing a valid schema. The per-SDK
workflows are a different check: they run each SDK's own test suite and linter. For
four of the eleven, that check was decoration (#1119):

  * `elixir-sdk.yml` and `fsharp-sdk.yml` declared `push:` with `paths` and `tags`
    and no `branches`. GitHub ANDs the ref filter with the path filter, so a push to
    a branch matched no ref pattern at all and the suites ran only on a release tag.
  * `csharp-sdk.yml` restricted `push` to `[dev, main]` — post-merge only.
  * `ruby-sdk.yml` watches `sdks/community/fraiseql-ruby/**`. The *official* Ruby
    SDK's unit tests ran nowhere.

None of that was visible from a green checks list, which is the shape this gate
exists to make loud: a twelfth SDK must not be able to arrive ungated, and an
existing one must not be able to lose its branch trigger silently.

What "covered" means here, deliberately narrow:

  a workflow whose `on.push` names the SDK's directory in `paths`
  AND whose `on.push` can match a branch — i.e. it declares no `branches` key at
  all (every branch), or declares one that is not restricted to a fixed list.

A `branches` list naming specific branches fails: it is exactly the C# case. A
`tags` key alongside is fine — that is how the publish jobs are triggered — as long
as `branches` is also present, because `tags` without `branches` is the Elixir case.

Overrides, for testing:
  SDK_WORKFLOW_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

# Directories under sdks/official/ that are not SDKs.
NOT_AN_SDK = {"conformance", "tests"}


def repo_root() -> Path:
    env = os.environ.get("SDK_WORKFLOW_ROOT")
    if env:
        return Path(env)
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )


def push_block(text: str) -> str | None:
    """The `on: push:` mapping of a workflow, as raw text.

    Deliberately a small hand parser rather than a YAML dependency: these gates run
    in a minimal container with no pip install step.
    """
    lines = text.splitlines()
    in_on = False
    in_push = False
    push_indent = None
    out: list[str] = []

    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue

        indent = len(line) - len(line.lstrip())

        if re.match(r"^on:\s*$", line) or re.match(r"^on:\s*\S", line):
            in_on = True
            continue

        if in_on and indent == 0:
            # A new top-level key ends the `on:` block.
            break

        if in_on and not in_push and re.match(r"^\s*push:\s*$", line):
            in_push = True
            push_indent = indent
            continue

        if in_push:
            if indent <= push_indent:
                break
            out.append(line)

    return "\n".join(out) if in_push else None


def branch_reachable(push: str) -> bool:
    """True when a push to an arbitrary branch can trigger this workflow."""
    m = re.search(r"^\s*branches:\s*(.*)$", push, re.MULTILINE)
    if m is None:
        # No `branches` key. Every branch matches — UNLESS a `tags` key is present,
        # in which case the ref filter is tags-only and no branch push ever matches.
        return not re.search(r"^\s*tags:\s*", push, re.MULTILINE)

    inline = m.group(1).strip()
    patterns: list[str] = []
    if inline and inline != "":
        # `branches: ['**']` or `branches: [dev, main]`
        patterns = [p.strip().strip("'\"") for p in inline.strip("[]").split(",") if p.strip()]
    else:
        # Block form: subsequent `- pattern` lines.
        tail = push[m.end() :]
        for line in tail.splitlines():
            if re.match(r"^\s*-\s+", line):
                patterns.append(line.split("-", 1)[1].strip().strip("'\""))
            elif line.strip() and not line.strip().startswith("#"):
                break

    # A wildcard that can match any branch name.
    return any(p in ("**", "*") or p.endswith("**") for p in patterns)


def main() -> int:
    root = repo_root()
    sdk_dir = root / "sdks" / "official"
    wf_dir = root / ".github" / "workflows"

    if not sdk_dir.is_dir():
        print(f"sdk-workflow-coverage: FAIL — no {sdk_dir}", file=sys.stderr)
        return 1

    sdks = sorted(
        d.name for d in sdk_dir.iterdir() if d.is_dir() and d.name not in NOT_AN_SDK
    )
    if not sdks:
        print(
            "sdk-workflow-coverage: FAIL — found zero SDKs under sdks/official; "
            "the layout changed and this gate went blind",
            file=sys.stderr,
        )
        return 1

    workflows = sorted(wf_dir.glob("*.yml")) + sorted(wf_dir.glob("*.yaml"))

    uncovered: list[str] = []
    tag_only: list[tuple[str, str]] = []

    for sdk in sdks:
        needle = f"sdks/official/{sdk}/"
        covered = False
        named_but_unreachable: str | None = None

        for wf in workflows:
            text = wf.read_text(encoding="utf-8")
            push = push_block(text)
            if push is None or needle not in push:
                continue
            if branch_reachable(push):
                covered = True
                break
            named_but_unreachable = wf.name

        if covered:
            continue
        if named_but_unreachable:
            tag_only.append((sdk, named_but_unreachable))
        else:
            uncovered.append(sdk)

    if uncovered or tag_only:
        print("sdk-workflow-coverage: FAIL", file=sys.stderr)
        if uncovered:
            print(
                "\nNo workflow runs these SDKs' own tests on a branch push:\n",
                file=sys.stderr,
            )
            for sdk in uncovered:
                print(f"  sdks/official/{sdk}", file=sys.stderr)
        if tag_only:
            print(
                "\nNamed by a workflow whose `on.push` cannot match a branch "
                "(tags-only, or a fixed branch list):\n",
                file=sys.stderr,
            )
            for sdk, wf in tag_only:
                print(f"  sdks/official/{sdk}  →  {wf}", file=sys.stderr)
            print(
                "\n  Add `branches: ['**']` beside the existing filter. Keep `tags` "
                "if a publish job is gated on it.",
                file=sys.stderr,
            )
        return 1

    print(
        f"sdk-workflow-coverage: OK — all {len(sdks)} official SDKs are gated by a "
        "workflow that runs on a branch push."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
