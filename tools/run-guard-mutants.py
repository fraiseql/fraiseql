#!/usr/bin/env python3
"""Run the mutant of every guard site in tools/guard-sites.toml and require its killers.

Each site's `anchor` (resolved inside its function exactly as tools/check-guard-sites.py
resolves it) is replaced by its `mutant`, the site's shard command is run, and the
original bytes are restored. Sites sharing a `joint` are mutated together: each covers
the other, so neither alone can be killed. A mutant's verdict is:

- KILLED: the shard failed and one of the site's `killers` is among the failed tests;
- KILLED-ELSEWHERE: the shard failed, but none of the named killers did (a test killing
  it for another reason does not show the rule is tested);
- NOBUILD: the mutant did not compile (a mutant that cannot build proves nothing);
- SURVIVED: the shard passed.

Each shard is first run unmutated, and one that fails there is BASELINE-RED and none of
its mutants run: a red shard would mark every mutant KILLED. A shard with a test binary
that runs no test (its target `cfg`'d out under the shard's features, or a filter that
matches nothing) is BASELINE-EMPTY and none of its mutants run either: every survivor it
reported would be untested. Anything but KILLED fails
the run. After the last mutant every touched file is checked
against its original sha256, and a mismatch fails the run too.

Selection: every site (the weekly sweep), `--site ID` (repeatable), `--shard NAME`, or
`--changed-since REF`: the sites whose `file` differs between REF and the working tree,
which is the per-PR run. `--list` prints the selection without running it; `--output DIR`
keeps each mutant's test output, to read why a mutant was killed.

Run from the repository root; the shard commands inherit this environment (a database
URL for the e2e shards, the cargo build dir). `GUARD_SITES_ROOT` and `GUARD_SITES_INVENTORY`
set the root and inventory as for the gate (for tools/tests/guard_mutants_test.sh).
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import os
import subprocess
import sys
import time
import tomllib
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
ROOT = Path(os.environ.get("GUARD_SITES_ROOT", TOOLS.parent))
INVENTORY = ROOT / os.environ.get("GUARD_SITES_INVENTORY", "tools/guard-sites.toml")


def load_gate():
    spec = importlib.util.spec_from_file_location("check_guard_sites", TOOLS / "check-guard-sites.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules["check_guard_sites"] = module
    spec.loader.exec_module(module)
    return module


def changed_files(ref: str) -> set[str]:
    out = subprocess.run(
        ["git", "diff", "--name-only", ref, "--"], cwd=ROOT, check=True, capture_output=True, text=True
    )
    return set(out.stdout.split())


def groups(sites: list[dict]) -> list[list[dict]]:
    """Sites in mutation units: a joint's members together, every other site alone."""
    units: list[list[dict]] = []
    joints: dict[str, list[dict]] = {}
    for site in sites:
        if "joint" in site:
            if site["joint"] not in joints:
                joints[site["joint"]] = []
                units.append(joints[site["joint"]])
            joints[site["joint"]].append(site)
        else:
            units.append([site])
    return units


def failed_tests(output: str) -> list[str]:
    return [
        line.split()[1]
        for line in output.splitlines()
        if line.startswith("test ") and line.rstrip().endswith("FAILED")
    ]


def empty_binaries(output: str) -> list[str]:
    """The test binaries a run compiled but that ran no test at all: a binary whose target is
    `cfg`'d out under the shard's features, or a filter that matches nothing. It can kill no
    mutant, so a shard holding one would report survivors that were never tested."""
    empty: list[str] = []
    binary = "?"
    for line in output.splitlines():
        stripped = line.strip()
        if stripped.startswith("Running "):
            binary = stripped.split()[1]
        elif stripped.startswith("test result:") and " 0 passed; 0 failed;" in stripped:
            empty.append(binary)
    return empty


def names(killer: str, failed: str) -> bool:
    return failed == killer or failed.endswith("::" + killer)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--site", action="append", default=[])
    parser.add_argument("--shard")
    parser.add_argument("--changed-since")
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--output", type=Path, help="write each mutant's test output here")
    args = parser.parse_args()

    gate = load_gate()
    with INVENTORY.open("rb") as handle:
        data = tomllib.load(handle)
    shards = {shard["name"]: shard for shard in data.get("shard", [])}
    sites = data.get("site", [])
    if args.site:
        unknown = set(args.site) - {site["id"] for site in sites}
        if unknown:
            print(f"unknown site id(s): {sorted(unknown)}")
            return 2
        sites = [site for site in sites if site["id"] in args.site]
    if args.shard:
        sites = [site for site in sites if site["shard"] == args.shard]
    if args.changed_since:
        touched = changed_files(args.changed_since)
        files = {site["file"] for site in sites if site["file"] in touched}
        # A joint member's file changing re-runs the whole joint.
        joint_names = {site["joint"] for site in sites if site["file"] in files and "joint" in site}
        sites = [site for site in sites if site["file"] in files or site.get("joint") in joint_names]

    units = groups(sites)
    for unit in units:
        print(f"{' + '.join(site['id'] for site in unit)}  [{unit[0]['shard']}]")
    print(f"{len(units)} mutant(s) selected")
    if args.list or not units:
        return 0

    originals: dict[Path, bytes] = {}
    verdicts: list[tuple[str, str, float]] = []
    baselines: dict[str, bool] = {}
    try:
        for unit in units:
            name = unit[0]["shard"]
            if name not in baselines:
                # A shard that already fails would mark every mutant of it KILLED.
                started = time.monotonic()
                run = subprocess.run(
                    ["bash", "-c", shards[name]["command"]], cwd=ROOT, capture_output=True, text=True
                )
                baselines[name] = run.returncode == 0
                empty = empty_binaries(run.stdout + run.stderr) if baselines[name] else []
                if empty:
                    baselines[name] = False
                    verdicts.append((f"shard {name}", f"BASELINE-EMPTY {','.join(empty)}", 0.0))
                    print(f"shard {name}: BASELINE-EMPTY {','.join(empty)} ran no test", flush=True)
                elif not baselines[name]:
                    failed = ",".join(failed_tests(run.stdout + run.stderr)) or f"rc={run.returncode}"
                    verdicts.append((f"shard {name}", f"BASELINE-RED {failed}", 0.0))
                    print(f"shard {name}: BASELINE-RED {failed}", flush=True)
                else:
                    print(f"shard {name}: baseline passes ({time.monotonic() - started:.0f}s)", flush=True)
            if not baselines[name]:
                continue
            label = " + ".join(site["id"] for site in unit)
            shard = shards[unit[0]["shard"]]
            saved: dict[Path, bytes] = {}
            try:
                for site in unit:
                    path = ROOT / site["file"]
                    raw = path.read_bytes()
                    originals.setdefault(path, raw)
                    saved.setdefault(path, raw)
                    text = path.read_text(encoding="utf-8")
                    if site["file"].endswith(".sql"):
                        # A `.sql` file has no functions: the anchor is found in the whole file.
                        spans = [(0, len(text))]
                    else:
                        spans = gate.Source.of(text).function_spans(site["function"])
                    body = text[spans[0][0] : spans[0][1]] if len(spans) == 1 else ""
                    if body.count(site["anchor"]) != 1:
                        raise SystemExit(f"{site['id']}: does not resolve; run tools/check-guard-sites.py")
                    at = spans[0][0] + body.index(site["anchor"])
                    path.write_text(
                        text[:at] + site["mutant"] + text[at + len(site["anchor"]) :], encoding="utf-8"
                    )
                started = time.monotonic()
                run = subprocess.run(
                    ["bash", "-c", shard["command"]], cwd=ROOT, capture_output=True, text=True
                )
                elapsed = time.monotonic() - started
            finally:
                for path, raw in saved.items():
                    path.write_bytes(raw)
            output = run.stdout + run.stderr
            if args.output:
                args.output.mkdir(parents=True, exist_ok=True)
                (args.output / f"{label.replace(' + ', '+')}.out").write_text(output, encoding="utf-8")
            failed = failed_tests(output)
            killers = [killer for site in unit for killer in site["killers"]]
            if "could not compile" in output or "error[E" in output:
                verdict = "NOBUILD"
            elif run.returncode == 0:
                verdict = "SURVIVED"
            elif any(names(killer, test) for killer in killers for test in failed):
                verdict = "KILLED"
            else:
                verdict = "KILLED-ELSEWHERE " + (",".join(failed) or f"rc={run.returncode}")
            verdicts.append((label, verdict, elapsed))
            print(f"{label}: {verdict} ({elapsed:.0f}s)", flush=True)
    finally:
        for path, raw in originals.items():
            if hashlib.sha256(path.read_bytes()).digest() != hashlib.sha256(raw).digest():
                path.write_bytes(raw)
                print(f"RESTORED {path.relative_to(ROOT)}: it did not match its original bytes")
                verdicts.append((str(path), "UNRESTORED", 0.0))

    bad = [(label, verdict) for label, verdict, _ in verdicts if verdict != "KILLED"]
    total = sum(elapsed for _, _, elapsed in verdicts)
    print(f"guard mutants: {len(verdicts) - len(bad)}/{len(verdicts)} killed in {total:.0f}s")
    for label, verdict in bad:
        print(f"  FAILED {label}: {verdict}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
