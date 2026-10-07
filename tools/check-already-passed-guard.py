#!/usr/bin/env python3
"""Every self-hosted job a push can start skips a SHA that already passed it.

Every commit reaches `dev` by fast-forward, so the `dev` push re-delivers a SHA
its branch push (or a dispatch on the chain branch) has just tested.
`.github/workflows/already-passed.yml` answers "has this SHA already passed this
workflow?", and four Dagger legs gate on it. `Dagger — image` did not: it reran
for ~36 minutes on archbox after every fast-forward, on a tree a dispatch on the
chain branch had already built and booted (dev cdec0acf0, 2026-10-07).

The guard is opt-in per workflow, so a fifth heavy leg arrives without it by
default. This gate makes it opt-out instead. For every workflow with a `push`
trigger, each job whose `runs-on` names `self-hosted` must:

  A. `needs:` a job that `uses: ./.github/workflows/already-passed.yml`;
  B. pass that guard its OWN file name as `with.workflow` — a guard asking about
     another workflow answers a different question, and copy-pasting a leg is
     exactly how that would happen;
  C. carry an `if:` that reads `needs.<guard>.outputs.passed != 'true'` and is
     not short-circuited by `cancelled()` (the guard fails toward running, so
     the job must still run when the guard job itself failed).

A workflow is exempt only by a row in EXEMPT below, with its reason.

Exit codes: 0 = clean, 1 = findings, 2 = FATAL (a workflow the gate cannot read,
or a scan that found no guarded leg at all — an empty scan proves nothing).

Overrides, for testing:
  ALREADY_PASSED_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

import importlib.util
import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
GUARD_USES = "./.github/workflows/already-passed.yml"

# workflow file → why its self-hosted jobs rerun on every push of the same SHA.
EXEMPT = {
    "dagger-security.yml": (
        "its verdict depends on the advisory database as well as the SHA: a "
        "RustSec advisory published between the branch push and the fast-forward "
        "must still turn dev red, and the leg costs ~2 min"
    ),
}


def die(message: str) -> None:
    print(f"FATAL: {message}", file=sys.stderr)
    raise SystemExit(2)


def scan_root() -> Path:
    env = os.environ.get("ALREADY_PASSED_ROOT")
    return Path(env) if env else REPO


def yaml_module():
    """The stdlib-only YAML subset parser shared by the workflow gates.

    The ShellGates container has no PyYAML. A missing sibling is FATAL, never a
    skip.
    """
    path = REPO / "tools" / "check-suite-coverage.py"
    spec = importlib.util.spec_from_file_location("_fraiseql_suite_coverage", path)
    if spec is None or spec.loader is None:
        die(f"cannot load the YAML parser from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


def as_list(value) -> list[str]:
    if value is None:
        return []
    if isinstance(value, list):
        return [str(v) for v in value]
    return [str(value)]


def check_workflow(path: Path, yaml) -> tuple[list[str], int]:
    """Findings for one workflow, and how many guarded jobs it holds."""
    try:
        doc = yaml.parse_yaml(path.read_text())
    except yaml.YamlError as exc:
        die(f"{path.name}: cannot parse: {exc}")
    if not isinstance(doc, dict):
        die(f"{path.name}: not a mapping")
    triggers = doc.get("on")
    if isinstance(triggers, str):
        triggers = {triggers: None}
    elif isinstance(triggers, list):
        triggers = dict.fromkeys(triggers)
    if not isinstance(triggers, dict) or "push" not in triggers:
        return [], 0

    jobs = doc.get("jobs") or {}
    if not isinstance(jobs, dict):
        die(f"{path.name}: `jobs` is not a mapping")

    guards = {
        name: job
        for name, job in jobs.items()
        if isinstance(job, dict) and job.get("uses") == GUARD_USES
    }
    findings: list[str] = []
    guarded = 0
    for name, job in jobs.items():
        if not isinstance(job, dict) or name in guards:
            continue
        if "self-hosted" not in as_list(job.get("runs-on")):
            continue
        if path.name in EXEMPT:
            continue
        where = f"{path.name}: job `{name}`"
        needed = [n for n in as_list(job.get("needs")) if n in guards]
        if not needed:
            findings.append(
                f"{where} runs on a self-hosted runner on push but does not "
                f"`needs:` a `{GUARD_USES}` job, so every fast-forward reruns it"
            )
            continue
        guard = needed[0]
        asked = (guards[guard].get("with") or {}).get("workflow")
        if asked != path.name:
            findings.append(
                f"{where}: guard `{guard}` asks about `{asked}`, not `{path.name}`"
            )
            continue
        cond = str(job.get("if") or "")
        wanted = re.compile(
            rf"needs\.{re.escape(guard)}\.outputs\.passed\s*!=\s*'true'"
        )
        if not wanted.search(cond) or "!cancelled()" not in cond.replace(" ", ""):
            findings.append(
                f"{where}: `if:` must be `!cancelled() && "
                f"needs.{guard}.outputs.passed != 'true'`, got {cond!r}"
            )
            continue
        guarded += 1
    return findings, guarded


def main() -> int:
    yaml = yaml_module()
    directory = scan_root() / ".github" / "workflows"
    if not directory.is_dir():
        die(f"no workflow directory at {directory}")
    findings: list[str] = []
    guarded = 0
    for path in sorted(directory.glob("*.yml")) + sorted(directory.glob("*.yaml")):
        found, count = check_workflow(path, yaml)
        findings.extend(found)
        guarded += count
    for name in EXEMPT:
        if not (directory / name).is_file():
            findings.append(f"EXEMPT names {name}, which does not exist")
    if findings:
        for line in findings:
            print(f"FAIL: {line}")
        return 1
    if guarded == 0:
        die("no self-hosted push job routes through already-passed: scanned nothing")
    print(f"already-passed guard: OK ({guarded} self-hosted push jobs guarded)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
