#!/usr/bin/env python3
"""No workflow job may hand cargo a RUSTC_WRAPPER it never installs.

v2.15.0's `release.yml` set `RUSTC_WRAPPER: "sccache"` at workflow level. Every job
inherits that, and cargo execs the wrapper for every rustc call — so a job that runs
`cargo` without first installing sccache fails at the first compile with
`could not execute process `sccache …` (No such file or directory)`. `publish-rust-sdk`
was such a job: it ran `cargo publish` with no `sccache-action` step and no override, and
`fraiseql-rust` never reached crates.io. `validate-release` in the same file already
carried the fix (`RUSTC_WRAPPER: ""`); nothing said the next job needed it too.

Per workflow, for every job:

  * the effective wrapper is the job's `env.RUSTC_WRAPPER` when present, else the
    workflow's; a step's own `env.RUSTC_WRAPPER` overrides both for that step;
  * when it is non-empty, every step whose `run:` invokes `cargo` must come AFTER a
    step that installs the wrapper: `uses: mozilla-actions/sccache-action@…`, or a local
    composite action (`uses: ./.github/actions/…`) one of whose steps does.

An unreadable workflow is a failure, not a pass.

Override, for testing:  RUSTC_WRAPPER_ROOT=<dir>
"""

from __future__ import annotations

import importlib.util
import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# `uses:` prefixes that put the wrapper on PATH. Keyed by wrapper so a different wrapper
# cannot be satisfied by installing sccache.
INSTALLERS = {"sccache": ("mozilla-actions/sccache-action@",)}

# `cargo` as a command word: start of line or after a shell separator, then whitespace.
CARGO_RE = re.compile(r"(?:^|[;&|(]|\bthen|\bdo|\$\()\s*cargo(?:\s|$)", re.MULTILINE)


def die(message: str) -> None:
    print(f"FATAL: {message}", file=sys.stderr)
    raise SystemExit(2)


def yaml_module():
    """The shared YAML-subset parser (ShellGates has python3 and no PyYAML)."""
    path = REPO / "tools" / "check-suite-coverage.py"
    spec = importlib.util.spec_from_file_location("_fraiseql_suite_coverage", path)
    if spec is None or spec.loader is None:
        die(f"cannot load the YAML parser from {path}")
    module = importlib.util.module_from_spec(spec)
    # Registered before execution: `@dataclass` resolves `sys.modules[__module__]`.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


def wrapper_in(env: object) -> str | None:
    """`RUSTC_WRAPPER` from an `env:` mapping; None when the mapping does not set it."""
    if not isinstance(env, dict) or "RUSTC_WRAPPER" not in env:
        return None
    value = env["RUSTC_WRAPPER"]
    return "" if value in (None, False) else str(value).strip()


def installs(step: dict, wrapper: str, root: Path, yaml, seen: frozenset[str] = frozenset()) -> bool:
    uses = step.get("uses")
    if not isinstance(uses, str):
        return False
    if any(uses.startswith(p) for p in INSTALLERS.get(Path(wrapper).name, ())):
        return True
    if not uses.startswith("./") or uses in seen:
        return False
    # A local composite action installs the wrapper when one of its own steps does.
    action_dir = root / uses
    manifest = next((action_dir / n for n in ("action.yml", "action.yaml") if (action_dir / n).is_file()), None)
    if manifest is None:
        die(f"`uses: {uses}` names a local action with no action.yml under {action_dir}")
    try:
        action = yaml.parse_yaml(manifest.read_text()) or {}
    except (OSError, yaml.YamlError) as exc:
        die(f"{manifest}: cannot be read: {exc}")
    steps = (action.get("runs") or {}).get("steps") or [] if isinstance(action, dict) else []
    return any(
        isinstance(s, dict) and installs(s, wrapper, root, yaml, seen | {uses}) for s in steps
    )


def check_workflow(path: Path, root: Path, yaml) -> list[str]:
    try:
        workflow = yaml.parse_yaml(path.read_text()) or {}
    except (OSError, yaml.YamlError) as exc:
        return [f"{path.name}: cannot be read: {exc}"]
    if not isinstance(workflow, dict):
        return [f"{path.name}: not a mapping"]

    findings: list[str] = []
    top = wrapper_in(workflow.get("env")) or ""
    jobs = workflow.get("jobs") or {}
    if not isinstance(jobs, dict):
        return [f"{path.name}: `jobs:` is not a mapping"]

    for job_id, job in jobs.items():
        if not isinstance(job, dict):
            continue
        job_wrapper = wrapper_in(job.get("env"))
        effective = top if job_wrapper is None else job_wrapper
        installed: set[str] = set()
        for index, step in enumerate(job.get("steps") or []):
            if not isinstance(step, dict):
                continue
            step_wrapper = wrapper_in(step.get("env"))
            wrapper = effective if step_wrapper is None else step_wrapper
            if wrapper and installs(step, wrapper, root, yaml):
                installed.add(wrapper)
            run = step.get("run")
            if not wrapper or not isinstance(run, str) or not CARGO_RE.search(run):
                continue
            if wrapper not in installed:
                name = step.get("name") or f"step {index + 1}"
                findings.append(
                    f"{path.name}: job `{job_id}`, step `{name}` runs cargo with "
                    f'RUSTC_WRAPPER="{wrapper}" and no earlier step installs it. '
                    f"Add the installer step, or set `RUSTC_WRAPPER: \"\"` on the job."
                )
    return findings


def main() -> int:
    root = Path(os.environ.get("RUSTC_WRAPPER_ROOT") or REPO)
    workflows = sorted((root / ".github" / "workflows").glob("*.y*ml"))
    if not workflows:
        die(f"no workflows under {root / '.github' / 'workflows'}")
    yaml = yaml_module()
    findings = [f for path in workflows for f in check_workflow(path, root, yaml)]
    for finding in findings:
        print(f"  ✗ {finding}", file=sys.stderr)
    if findings:
        print(f"rustc wrapper: {len(findings)} finding(s)", file=sys.stderr)
        return 1
    print(f"rustc wrapper: ok ({len(workflows)} workflows)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
