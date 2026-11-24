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
reported would be untested. Anything but KILLED fails the run. After the last mutant every
touched file is checked against its original sha256, and a mismatch fails the run too, as
does an untracked file a shard run wrote (LEFT-BEHIND: a proptest seed of a failure the
mutant caused, which committed would pin it).

An interrupted run leaves no mutant behind. SIGINT, SIGTERM and SIGHUP end it through the
blocks that restore each file. Before a mutant is written, its file's saved bytes are
recorded under the git dir, so a run that could not restore (SIGKILL, a crash) is repaired
by the next start: a file still holding its mutant gets its bytes back, and the run then
refuses to start (exit 2) until every recorded file matches HEAD.

Selection: every site (the weekly sweep), `--site ID` (repeatable), `--shard NAME`, or
`--changed-since REF`: the sites whose `file` differs between REF and the working tree,
which is the per-PR run. `--lane I/N` keeps the selection's share for lane I of N: the
shards are grouped by the cargo build they compile, every shard of one build runs in one
lane, and the builds are balanced across lanes by mutant count, so lanes on their own
target volumes run side by side. A selected site brings every member of its joint, so a joint is
never mutated in part. `--list` prints the selection without running it; `--output DIR`
keeps each mutant's test output, to read why a mutant was killed.

Run from the repository root; the shard commands inherit this environment (a database
URL for the e2e shards, the cargo build dir). A `DATABASE_URL` must reach a server that
ships the `vector` extension, as the `PG_IMAGE` CI uses does: on one that does not, a
shard testing a pgvector column goes red unmutated, which is the rig's fault and not a
verdict, so the run stops before any shard with a RIG error (exit 2). `GUARD_SITES_ROOT` and `GUARD_SITES_INVENTORY`
set the root and inventory as for the gate (for tools/tests/guard_mutants_test.sh).
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
import tomllib
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
ROOT = Path(os.environ.get("GUARD_SITES_ROOT", TOOLS.parent))
INVENTORY = ROOT / os.environ.get("GUARD_SITES_INVENTORY", "tools/guard-sites.toml")


# The Postgres image the e2e shards run against in CI; it ships the `vector` extension.
PG_IMAGE = "ghcr.io/fraiseql/pgvector:pg18"


def rig_problem() -> str | None:
    """Why the database the shards would use cannot give a verdict, or `None`. A server
    without the `vector` extension fails a shard unmutated, and BASELINE-RED would then
    report a fault of the rig as one of the code."""
    url = os.environ.get("DATABASE_URL")
    if not url:
        return None
    query = "SELECT count(*) FROM pg_available_extensions WHERE name = 'vector'"
    try:
        run = subprocess.run(
            ["psql", url, "-tAc", query], capture_output=True, text=True, timeout=30, check=False
        )
    except FileNotFoundError:
        return "psql is not installed, so DATABASE_URL cannot be checked for the `vector` extension"
    except subprocess.TimeoutExpired:
        return "DATABASE_URL did not answer within 30s"
    if run.returncode != 0:
        lines = run.stderr.strip().splitlines()
        return f"DATABASE_URL cannot be queried: {lines[-1] if lines else f'psql rc={run.returncode}'}"
    if run.stdout.strip() != "1":
        return f"DATABASE_URL has no `vector` extension available; run the shards against {PG_IMAGE}"
    return None


def load_gate():
    spec = importlib.util.spec_from_file_location("check_guard_sites", TOOLS / "check-guard-sites.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules["check_guard_sites"] = module
    spec.loader.exec_module(module)
    return module


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def restore_dir() -> Path:
    """Where a mutant in place is recorded: under the git dir, so never in the tree."""
    out = subprocess.run(
        ["git", "rev-parse", "--absolute-git-dir"], cwd=ROOT, check=True, capture_output=True, text=True
    )
    return Path(out.stdout.strip()) / "guard-mutants"


def record(mutated: dict[Path, tuple[bytes, bytes]]) -> None:
    """Record each file about to hold a mutant, with its saved bytes, before it is written:
    a run killed with the mutant in place leaves this for the next start to restore."""
    where = restore_dir()
    where.mkdir(parents=True, exist_ok=True)
    entries = []
    for path, (saved, mutant) in mutated.items():
        file = str(path.relative_to(ROOT))
        name = f"{sha256(file.encode('utf-8'))[:16]}.saved"
        (where / name).write_bytes(saved)
        entries.append(
            {
                "file": file,
                "saved": name,
                "saved_sha256": sha256(saved),
                "mutant_sha256": sha256(mutant),
            }
        )
    staged = where / "marker.json.tmp"
    staged.write_text(json.dumps(entries, indent=2), encoding="utf-8")
    staged.replace(where / "marker.json")


def clear_record() -> None:
    shutil.rmtree(restore_dir(), ignore_errors=True)


def matches_head(file: str) -> bool:
    run = subprocess.run(["git", "diff", "--quiet", "HEAD", "--", file], cwd=ROOT, check=False)
    return run.returncode == 0


def recover() -> str | None:
    """Restore what an interrupted run left mutated, and why a run cannot start, if so.

    A file still holding its recorded mutant gets its saved bytes back. A file the record
    names that differs from HEAD stops the run until it matches: the record is kept so the
    next start checks it again, as a mutant restored over a change, or a change written
    since, is for a person to look at."""
    marker = restore_dir() / "marker.json"
    if not marker.exists():
        return None
    entries = json.loads(marker.read_text(encoding="utf-8"))
    for entry in entries:
        path = ROOT / entry["file"]
        if path.exists() and sha256(path.read_bytes()) == entry["mutant_sha256"]:
            path.write_bytes((restore_dir() / entry["saved"]).read_bytes())
            print(f"RESTORED {entry['file']}: an interrupted run left a mutant in it")
    differ = [entry["file"] for entry in entries if not matches_head(entry["file"])]
    if differ:
        return (
            f"{', '.join(differ)} differs from HEAD after an interrupted run; check it, then "
            "commit or restore it: no mutant runs until each matches HEAD"
        )
    clear_record()
    return None


def interrupted(signum: int, _frame: object) -> None:
    """A signal ends the run through its `finally` blocks, which restore every file."""
    raise SystemExit(128 + signum)


def changed_files(ref: str) -> set[str]:
    out = subprocess.run(
        ["git", "diff", "--name-only", ref, "--"], cwd=ROOT, check=True, capture_output=True, text=True
    )
    return set(out.stdout.split())


def untracked() -> set[str]:
    """The untracked, unignored files under the root: a shard run that writes one (a
    proptest seed of a failure the mutant caused) leaves the tree unlike the one it ran."""
    out = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard", "-z"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return {path for path in out.stdout.split("\0") if path}


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


def build_key(command: str) -> str:
    """The cargo build a shard command compiles: its package and feature set. Shards with
    one key share their dependency artifacts, so a lane that runs them builds them once."""
    package = re.search(r"\bcargo test\b.*?-p (\S+)", command)
    if not package:
        return command
    features = re.search(r"--features (?:'([^']*)'|(\S+))", command)
    named = sorted((features.group(1) or features.group(2)).split(",")) if features else []
    flags = [flag for flag in ("--all-features", "--no-default-features") if flag in command]
    return " ".join([package.group(1), ",".join(named), *flags])


def lane_units(
    units: list[list[dict]], shards: dict[str, dict], index: int, count: int
) -> list[list[dict]]:
    """The units lane `index` of `count` runs. Every unit of one build lands in one lane;
    builds go to the lane with the fewest units so far, the largest first."""
    by_build: dict[str, list[list[dict]]] = {}
    for unit in units:
        by_build.setdefault(build_key(shards[unit[0]["shard"]]["command"]), []).append(unit)
    loads = [0] * count
    mine: set[str] = set()
    for key in sorted(by_build, key=lambda key: (-len(by_build[key]), key)):
        lane = min(range(count), key=lambda lane: (loads[lane], lane))
        loads[lane] += len(by_build[key])
        if lane == index - 1:
            mine.add(key)
    return [unit for unit in units if build_key(shards[unit[0]["shard"]]["command"]) in mine]


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
    parser.add_argument("--lane", help="I/N: run lane I of N, each build in one lane")
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--output", type=Path, help="write each mutant's test output here")
    args = parser.parse_args()

    for signum in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(signum, interrupted)
    problem = recover()
    if problem:
        print(f"INTERRUPTED: {problem}")
        return 2

    gate = load_gate()
    with INVENTORY.open("rb") as handle:
        data = tomllib.load(handle)
    shards = {shard["name"]: shard for shard in data.get("shard", [])}
    inventory = data.get("site", [])
    sites = inventory
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
        sites = [site for site in sites if site["file"] in touched]
    # A selected joint member brings its whole joint: one member mutated alone is not the
    # joint, and a test that kills it alone would read as the joint's verdict.
    joint_names = {site["joint"] for site in sites if "joint" in site}
    sites = [site for site in inventory if site in sites or site.get("joint") in joint_names]

    units = groups(sites)
    if args.lane:
        lane = re.fullmatch(r"([1-9][0-9]*)/([1-9][0-9]*)", args.lane)
        if not lane or int(lane.group(1)) > int(lane.group(2)):
            print(f"--lane takes I/N with 1 <= I <= N, got {args.lane!r}")
            return 2
        units = lane_units(units, shards, int(lane.group(1)), int(lane.group(2)))
    for unit in units:
        print(f"{' + '.join(site['id'] for site in unit)}  [{unit[0]['shard']}]")
    print(f"{len(units)} mutant(s) selected")
    if args.list or not units:
        return 0
    problem = rig_problem()
    if problem:
        print(f"RIG: {problem}")
        print("no shard was run: a fault of the rig is not a verdict")
        return 2

    originals: dict[Path, bytes] = {}
    before = untracked()
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
            # Every member of the unit is applied in memory first, so the record names each
            # file's saved bytes and the exact mutant written over them.
            saved: dict[Path, bytes] = {}
            texts: dict[Path, str] = {}
            for site in unit:
                path = ROOT / site["file"]
                if path not in saved:
                    raw = path.read_bytes()
                    originals.setdefault(path, raw)
                    saved[path] = raw
                    texts[path] = raw.decode("utf-8")
                text = texts[path]
                if site["file"].endswith(".sql"):
                    # A `.sql` file has no functions: the anchor is found in the whole file.
                    spans = [(0, len(text))]
                else:
                    spans = gate.Source.of(text).site_spans(site["function"], site.get("cfg"))
                body = text[spans[0][0] : spans[0][1]] if len(spans) == 1 else ""
                if body.count(site["anchor"]) != 1:
                    raise SystemExit(f"{site['id']}: does not resolve; run tools/check-guard-sites.py")
                at = spans[0][0] + body.index(site["anchor"])
                texts[path] = text[:at] + site["mutant"] + text[at + len(site["anchor"]) :]
            mutated = {path: (saved[path], texts[path].encode("utf-8")) for path in saved}
            record(mutated)
            try:
                for path, (_, mutant) in mutated.items():
                    path.write_bytes(mutant)
                started = time.monotonic()
                run = subprocess.run(
                    ["bash", "-c", shard["command"]], cwd=ROOT, capture_output=True, text=True
                )
                elapsed = time.monotonic() - started
            finally:
                for path, raw in saved.items():
                    path.write_bytes(raw)
                clear_record()
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
        for path in sorted(untracked() - before):
            # Not removed here: it may be a test's own output. Committed, a seed written
            # under a mutant would pin that mutant's failure as a case to re-run.
            print(f"LEFT-BEHIND {path}: a shard run wrote it; remove it before committing")
            verdicts.append((path, "LEFT-BEHIND", 0.0))

    bad = [(label, verdict) for label, verdict, _ in verdicts if verdict != "KILLED"]
    total = sum(elapsed for _, _, elapsed in verdicts)
    print(f"guard mutants: {len(verdicts) - len(bad)}/{len(verdicts)} killed in {total:.0f}s")
    for label, verdict in bad:
        print(f"  FAILED {label}: {verdict}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
