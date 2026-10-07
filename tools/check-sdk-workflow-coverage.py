#!/usr/bin/env python3
"""Every official SDK's own suite runs under ONE unfiltered, required check.

`sdk-conformance.yml` drives what each SDK *emits*. Each SDK's own unit suite and
linters are a different check, and for a long time they gated nothing:

  * #1119: four of eleven per-SDK workflows could not run on a branch push at all
    (`tags` without `branches`, `branches: [dev, main]`, and the official Ruby SDK
    watched by no workflow).
  * #1467: the per-SDK workflows that did run were `paths:`-filtered, and GitHub
    reports a filtered-out required check as "not run", never "passed". None of them
    could be required, so a merge could land with every SDK suite red.

`.github/workflows/sdk-suites.yml` replaced them. This gate holds its shape:

  A. its `push:` carries no `paths`/`paths-ignore` and reaches every working branch;
  B. every directory under sdks/official/ (minus NOT_AN_SDK) has a row in
     `tools/sdk-suites-matrix.py`, and every row names an existing directory —
     a twelfth SDK cannot arrive ungated;
  C. the `suite` job has a setup step per SDK (`if: matrix.sdk == '<key>'`) and
     runs `tools/sdk-suite.sh`;
  D. a job named `SDK suites` needs that job, runs under `always()`, and is listed in
     `tools/required-checks.toml` — the aggregate is the context that gates;
  E. no OTHER workflow runs `tools/sdk-suite.sh` on a branch push: a filtered copy
     is the shape #1467 removed, and it would run beside the gate without gating.

Which halves a `push:` defines is GitHub's rule, implemented once as
`push_ref_filter` in tools/check-suite-coverage.py; `tools/check-trigger-rule-copies.py`
refuses a second copy.

Exit codes: 0 = clean, 1 = findings, 2 = FATAL (an input the gate cannot read).

Overrides, for testing:
  SDK_WORKFLOW_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

import importlib.util
import os
import re
import subprocess
import sys
from pathlib import Path

# Directories under sdks/official/ that are not SDKs.
NOT_AN_SDK = {"conformance", "tests"}

WORKFLOW = "sdk-suites.yml"
AGGREGATE = "SDK suites"
SUITE_JOB = "suite"
SUITE_SCRIPT = "tools/sdk-suite.sh"


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


def fatal(message: str) -> None:
    print(f"FATAL: {message}", file=sys.stderr)
    raise SystemExit(2)


def _load(path: Path, name: str):
    """Import a sibling module by path. Missing or broken is FATAL, never a skip."""
    if not path.is_file():
        fatal(f"cannot load {path}")
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        fatal(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    # Registered before execution: `@dataclass`/`NamedTuple` resolve their own
    # `sys.modules[__module__]`.
    sys.modules[name] = module
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


def yaml_module():
    # Relative to THIS FILE: the parser is this gate's own code, while
    # SDK_WORKFLOW_ROOT names the tree being checked.
    return _load(
        Path(__file__).resolve().parent / "check-suite-coverage.py",
        "_fraiseql_suite_coverage",
    )


def parse(path: Path, yaml) -> dict:
    try:
        doc = yaml.parse_yaml(path.read_text(encoding="utf-8"))
    except yaml.YamlError as exc:
        fatal(f"{path.name}: {exc} — teach tools/check-suite-coverage.py this YAML shape")
    if not isinstance(doc, dict):
        fatal(f"{path.name}: not a mapping")
    return doc


def push_cfg(doc: dict, name: str) -> dict | None:
    """The parsed `on.push` mapping, `{}` for a bare `push:`, None for no push."""
    autos = doc.get("on")
    if isinstance(autos, str):
        autos = {autos: None}
    elif isinstance(autos, list):
        autos = dict.fromkeys(str(k) for k in autos)
    if not isinstance(autos, dict) or "push" not in autos:
        return None
    cfg = autos["push"]
    if cfg is None:
        return {}
    if not isinstance(cfg, dict):
        fatal(f"{name}: unreadable `push:` trigger {cfg!r}")
    return cfg


def ref_filter(yaml, cfg: dict, name: str):
    """GitHub's push ref-filter rule over `cfg`; a shape it cannot read is FATAL."""
    try:
        return yaml.push_ref_filter(cfg)
    except yaml.WorkflowUnresolvable as exc:
        fatal(f"{name}: {exc}")


def as_list(value) -> list[str]:
    if value is None:
        return []
    return [str(v) for v in value] if isinstance(value, list) else [str(value)]


def run_steps(job: dict) -> list[str]:
    steps = job.get("steps") or []
    return [str(s.get("run")) for s in steps if isinstance(s, dict) and s.get("run")]


def main() -> int:
    root = repo_root()
    yaml = yaml_module()
    findings: list[str] = []

    sdk_dir = root / "sdks" / "official"
    if not sdk_dir.is_dir():
        fatal(f"no {sdk_dir}")
    dirs = sorted(
        d.name for d in sdk_dir.iterdir() if d.is_dir() and d.name not in NOT_AN_SDK
    )
    if not dirs:
        fatal("found zero SDKs under sdks/official; the layout changed and this gate went blind")

    # B. the matrix table and the directories agree, both ways.
    table = _load(root / "tools" / "sdk-suites-matrix.py", "_fraiseql_sdk_matrix").VERSIONS
    keys = set(table)
    for d in dirs:
        if d.removeprefix("fraiseql-") not in keys:
            findings.append(
                f"sdks/official/{d} has no row in tools/sdk-suites-matrix.py, "
                "so no push runs its suite"
            )
    for key in sorted(keys):
        if f"fraiseql-{key}" not in dirs:
            findings.append(
                f"tools/sdk-suites-matrix.py row `{key}` names no sdks/official/fraiseql-{key}"
            )

    wf_dir = root / ".github" / "workflows"
    path = wf_dir / WORKFLOW
    if not path.is_file():
        findings.append(
            f".github/workflows/{WORKFLOW} is missing: no required check runs the SDK suites"
        )
        return report(findings, len(dirs))
    doc = parse(path, yaml)

    # A. unfiltered, every branch.
    cfg = push_cfg(doc, WORKFLOW)
    if cfg is None:
        findings.append(f"{WORKFLOW} has no `push:` trigger, so it reports on no push")
    else:
        for key in ("paths", "paths-ignore"):
            if key in cfg:
                findings.append(
                    f"{WORKFLOW}: `push.{key}` filters the workflow, and GitHub reports a "
                    'filtered-out required check as "not run", never "passed"'
                )
        if not ref_filter(yaml, cfg, WORKFLOW).reaches_every_branch():
            findings.append(f"{WORKFLOW}: `push:` does not reach every working branch")

    jobs = doc.get("jobs") or {}
    if not isinstance(jobs, dict):
        fatal(f"{WORKFLOW}: `jobs` is not a mapping")

    # C. a setup step per SDK, and the shared suite script.
    suite = jobs.get(SUITE_JOB)
    if not isinstance(suite, dict):
        findings.append(f"{WORKFLOW} has no `{SUITE_JOB}` job")
    else:
        conds = " ".join(
            str(s.get("if", "")) for s in suite.get("steps") or [] if isinstance(s, dict)
        )
        for key in sorted(keys):
            if not re.search(rf"matrix\.sdk\s*==\s*'{re.escape(key)}'", conds):
                findings.append(f"{WORKFLOW}: job `{SUITE_JOB}` has no setup step for `{key}`")
        if not any(SUITE_SCRIPT in r for r in run_steps(suite)):
            findings.append(f"{WORKFLOW}: job `{SUITE_JOB}` never runs {SUITE_SCRIPT}")

    # D. the aggregate, and that it is what the ruleset requires.
    aggregates = [
        (jid, j) for jid, j in jobs.items() if isinstance(j, dict) and j.get("name") == AGGREGATE
    ]
    if len(aggregates) != 1:
        findings.append(
            f"{WORKFLOW}: wanted exactly one job named `{AGGREGATE}`, found {len(aggregates)}"
        )
    else:
        jid, job = aggregates[0]
        if SUITE_JOB not in as_list(job.get("needs")):
            findings.append(f"{WORKFLOW}: `{AGGREGATE}` ({jid}) does not need `{SUITE_JOB}`")
        if "always()" not in str(job.get("if", "")):
            findings.append(
                f"{WORKFLOW}: `{AGGREGATE}` ({jid}) must run under `always()`, or a failed "
                "suite skips it and GitHub reads the skipped context as passing"
            )
    required = (root / "tools" / "required-checks.toml").read_text(encoding="utf-8")
    if not re.search(rf'^\s*"{re.escape(AGGREGATE)}",', required, re.M):
        findings.append(
            f"tools/required-checks.toml does not list `{AGGREGATE}`, so it gates nothing"
        )

    # E. no second, filtered copy.
    for other in sorted(wf_dir.glob("*.yml")) + sorted(wf_dir.glob("*.yaml")):
        if other.name == WORKFLOW:
            continue
        odoc = parse(other, yaml)
        ocfg = push_cfg(odoc, other.name)
        if ocfg is None or not ref_filter(yaml, ocfg, other.name).reaches_any_branch():
            continue
        for jid, job in (odoc.get("jobs") or {}).items():
            if isinstance(job, dict) and any(SUITE_SCRIPT in r for r in run_steps(job)):
                findings.append(
                    f"{other.name}: job `{jid}` runs {SUITE_SCRIPT} on a branch push beside "
                    f"{WORKFLOW}; a second copy runs without gating — delete it"
                )

    return report(findings, len(dirs))


def report(findings: list[str], count: int) -> int:
    if findings:
        print("sdk-workflow-coverage: FAIL", file=sys.stderr)
        for line in findings:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(
        f"sdk-workflow-coverage: OK — all {count} official SDKs run under the required "
        f"`{AGGREGATE}` check ({WORKFLOW}, unfiltered)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
