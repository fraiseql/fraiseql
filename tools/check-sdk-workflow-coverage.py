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
  AND whose `on.push` can match a branch — i.e. it imposes no branch allow-list
  (every branch), or one that is not restricted to a fixed list.

A `branches` list naming specific branches fails: it is exactly the C# case. A
`tags` key alongside is fine — that is how the publish jobs are triggered — as long
as the branch half is also defined, because `tags` without it is the Elixir case.

Which halves a `push:` defines is not decided here. It is GitHub's rule, four gates
need it, and this one had its own wrong copy until #1301: `branches-ignore:` and
`tags-ignore:` matched neither of its regexes, inverting the verdict in both
directions. `push_ref_filter` in tools/check-suite-coverage.py is the one
implementation, and `tools/check-trigger-rule-copies.py` refuses a second.

Overrides, for testing:
  SDK_WORKFLOW_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

import importlib.util
import os
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


_YAML_MODULE = None


def _yaml_module():
    """`parse_yaml` / `push_ref_filter` from tools/check-suite-coverage.py.

    One hand-written YAML-subset parser and one copy of GitHub's push ref-filter
    rule serve every gate that reads a workflow; this is the fourth to import
    them, by the pattern check-workflow-job-reachability.py established. Until
    #1301 this gate regexed the `push:` block out of raw text and applied its own
    copy of the rule, and both halves were wrong in ways no fixture covered:

      * `branches-ignore:` and `tags-ignore:` matched neither of its two regexes,
        so a `branches-ignore` push read as having no branch key (verdict False
        where the truth is True) and a `tags-ignore` push read as having no tag
        key (True where the truth is False).
      * a flow-style `push: {tags: ['v*']}` produced no push block at all, so the
        workflow was silently skipped rather than judged.

    A missing or unloadable sibling is FATAL, never a skip: a coverage gate that
    quietly scans nothing is the failure it exists to prevent.
    """
    global _YAML_MODULE
    if _YAML_MODULE is not None:
        return _YAML_MODULE
    # Relative to THIS FILE, not to `repo_root()`: `SDK_WORKFLOW_ROOT` names the
    # tree being checked, which is data, while the parser is this gate's own code.
    # The two coincide in normal use and differ under a fixture that copies both
    # gates into a scratch tree.
    path = Path(__file__).resolve().parent / "check-suite-coverage.py"
    spec = importlib.util.spec_from_file_location("_fraiseql_suite_coverage", path)
    if spec is None or spec.loader is None:
        print(f"FATAL: cannot load the YAML parser from {path}", file=sys.stderr)
        raise SystemExit(2)
    module = importlib.util.module_from_spec(spec)
    # Registered before execution: `@dataclass`/`NamedTuple` in an imported module
    # resolve their own `sys.modules[__module__]`, and an unregistered module makes
    # that lookup return None.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    _YAML_MODULE = module
    return module


def push_trigger(text: str) -> dict | None:
    """The parsed `on.push` mapping of a workflow, or None if it has no `push:`.

    `parse_yaml` leaves `on:` a string key deliberately — the YAML 1.1 coercion
    that turns `on` into `True` is the kind of quiet reshaping it refuses.
    """
    yaml = _yaml_module()
    try:
        doc = yaml.parse_yaml(text)
    except yaml.YamlError as exc:
        # A shape the parser refuses is FATAL, never a skip. Silently passing over
        # an unreadable workflow is how a gate reports coverage it never checked.
        print(
            f"FATAL: {exc} — teach tools/check-suite-coverage.py this YAML shape",
            file=sys.stderr,
        )
        raise SystemExit(2) from None
    if not isinstance(doc, dict):
        return None
    autos = doc.get("on")
    if isinstance(autos, str):
        # `on: push` — a single event name, no filters.
        autos = {autos: None}
    elif isinstance(autos, list):
        autos = {str(k): None for k in autos}
    if not isinstance(autos, dict) or "push" not in autos:
        return None
    cfg = autos["push"]
    if cfg is None:
        return {}  # a bare `push:` — no ref filter, no path filter
    if not isinstance(cfg, dict):
        # Not understood is not "probably fine": treating an unreadable `push:` as
        # an empty one would silently make it look like it gates everything.
        print(
            f"FATAL: unreadable `push:` trigger {cfg!r} — teach the gate this shape",
            file=sys.stderr,
        )
        raise SystemExit(2)
    return cfg


def branch_reachable(cfg: dict) -> bool:
    """True when a push to SOME branch can trigger this workflow.

    This gate's policy over the shared rule: an SDK is "gated" if a contributor
    pushing a branch runs its suite. A wildcard allow-list qualifies; a fixed
    list naming concrete branches does not — that is the `csharp-sdk.yml`
    post-merge-only case (#1119).
    """
    return _yaml_module().push_ref_filter(cfg).reaches_any_branch()


def _names_path(cfg: dict, needle: str) -> bool:
    """Does this `push:` watch a path under `needle`?

    Structured where the old test was a substring search over the raw `push:`
    text, which counted a needle appearing in a comment, in a `branches:` name,
    or — backwards — in `paths-ignore`, where naming the SDK means the workflow
    skips it.
    """
    raw = cfg.get("paths")
    return isinstance(raw, list) and any(needle in str(p) for p in raw)


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
            cfg = push_trigger(wf.read_text(encoding="utf-8"))
            if cfg is None or not _names_path(cfg, needle):
                continue
            if branch_reachable(cfg):
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
