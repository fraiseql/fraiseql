#!/usr/bin/env python3
"""check-release-validation.py — fail when a post-publish package validation cannot redden the
release.

Background (trust closure, 2026-09-21): release.yml's "Validate PyPI package" and
npm-publish.yml's "Validate npm package" installed the just-published package and — for PyPI —
asserted its version, but both carried ``continue-on-error: true``. verify-release (the hard
gate) polls the registries for the version, so a package that is *indexed but broken* — wrong
contents, an import that fails, a wheel that pins the previous version — passed the release
green. The npm step never imported the package at all. These two steps are the only place the
published artifact is exercised as a consumer would; they must be able to fail, and they must
load the package.

Rules, per (workflow, step name):
  1. the step exists;
  2. it does not carry a truthy ``continue-on-error``;
  3. its ``run`` installs the package pinned to the release version (``==${VERSION}`` /
     ``@${VERSION}``) and loads it (``import fraiseql`` / a ``require()`` of the package itself) and compares the
     installed version to the expected one (a ``__version__``/``.version`` check).

A workflow that cannot be read or a step that is absent is a failure, not a pass.

Override, for testing:  RELEASE_VALIDATION_ROOT=<dir>
"""
from __future__ import annotations

import importlib.util
import os
import re
import subprocess
import sys
from pathlib import Path

TOOLS = Path(__file__).resolve().parent

_YAML_MODULE = None


def _yaml_module():
    """`parse_yaml` / `YamlError` from tools/check-suite-coverage.py.

    The ShellGates and preflight containers are bare Ubuntu plus python3 — no
    PyYAML, no pip step — so this gate borrows the one hand-written YAML-subset
    parser the workflow gates already share rather than keeping a copy that
    drifts from it. A missing or unloadable sibling is FATAL, never a skip: a
    release gate that quietly checks nothing is the failure it exists to prevent.
    """
    global _YAML_MODULE
    if _YAML_MODULE is not None:
        return _YAML_MODULE
    path = TOOLS / "check-suite-coverage.py"
    spec = importlib.util.spec_from_file_location("_fraiseql_suite_coverage", path)
    if spec is None or spec.loader is None:
        print(f"FATAL: cannot load the YAML parser from {path}", file=sys.stderr)
        raise SystemExit(2)
    module = importlib.util.module_from_spec(spec)
    # Registered before execution: `@dataclass` in an imported module resolves its
    # own `sys.modules[__module__]`, and an unregistered module makes that lookup
    # return None (AttributeError on 3.14, not an import error naming the cause).
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    _YAML_MODULE = module
    return module


CHECKS = [
    # (workflow path, step name, install substring, load regex, version-compare substring)
    (".github/workflows/release.yml", "Validate PyPI package", "fraiseql==${VERSION}", r"\bimport fraiseql\b", "__version__"),
    # The load must be the package itself, not its package.json: `require('.../fraiseql')`.
    (".github/workflows/npm-publish.yml", "Validate npm package", "fraiseql@${VERSION}", r"require\(['\"][^'\"]*\bfraiseql['\"]\)", ".version"),
]


def repo_root() -> Path:
    override = os.environ.get("RELEASE_VALIDATION_ROOT")
    if override:
        return Path(override)
    out = subprocess.run(["git", "rev-parse", "--show-toplevel"], check=True, capture_output=True, text=True)
    return Path(out.stdout.strip())


def steps_of(workflow: dict) -> list[dict]:
    steps: list[dict] = []
    for job in (workflow.get("jobs") or {}).values():
        for step in (job or {}).get("steps") or []:
            if isinstance(step, dict):
                steps.append(step)
    return steps


def main() -> int:
    root = repo_root()
    yaml = _yaml_module()
    failures: list[str] = []
    for rel, name, install, load, compare in CHECKS:
        path = root / rel
        if not path.is_file():
            failures.append(f"{rel}: not found")
            continue
        try:
            workflow = yaml.parse_yaml(path.read_text()) or {}
        except yaml.YamlError as exc:
            failures.append(f"{rel}: not parseable YAML ({exc})")
            continue
        matches = [s for s in steps_of(workflow) if s.get("name") == name]
        if not matches:
            failures.append(f"{rel}: no step named '{name}' — the published package is never exercised")
            continue
        for step in matches:
            label = f"{rel} step '{name}'"
            coe = step.get("continue-on-error", False)
            if coe not in (False, None, "false"):
                failures.append(f"{label}: continue-on-error is {coe!r}; a validation that cannot fail is decorative")
            run = step.get("run") or ""
            if install not in run:
                failures.append(f"{label}: does not install the release version ({install!r} not in run)")
            if not re.search(load, run):
                failures.append(f"{label}: does not load the package (no match for {load!r} in run)")
            if compare not in run:
                failures.append(f"{label}: does not compare the installed version ({compare!r} not in run)")
    if failures:
        print("release validation:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"release validation: ok — {len(CHECKS)} post-publish validations are blocking, install the release version, load it and compare the version")
    return 0


if __name__ == "__main__":
    sys.exit(main())
