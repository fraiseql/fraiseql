#!/usr/bin/env python3
"""The delivery-coverage gate: every artifact this repository ships maps to a leg that
EXECUTES it, or to an exemption naming the issue that owns the gap.

`tools/check-suite-coverage.py` made *test* execution a checked artifact, after the
2026-07-27 retrospective's rule 1 — "execution coverage must be a checked artifact, not
an inference". This is the same move one level up, for delivery. The hole it closes is
the one that let the release image stay unbuildable for weeks with eleven CI legs green
(#1205): every gate checked the source, almost nothing checked the thing we ship.

1. **Discover what ships**, each class from the source of truth another gate already
   reads, never from a second hand-maintained list:

   - container images   ← `imageVariants` in .dagger/image.go   (check-image-parity.py)
   - crates             ← publishable workspace members         (check-publish-parity.py)
   - SDK packages       ← sdks/official/* minus NOT_AN_SDK      (check-sdk-workflow-coverage.py)
                          plus any sdks/community/* a workflow actually publishes
   - the Helm chart     ← deploy/**/Chart.yaml
   - the Compose stack  ← CANONICAL= in tools/compose-stack-test.sh
   - release assets     ← the EXPECTED array in release.yml's verify-release job

2. **Compare against tools/delivery-artifacts.toml, both directions.** An artifact with
   no row fails: that is a new thing shipping ungated, which is the property this gate
   exists to buy. A row naming an artifact discovery no longer finds also fails — #883's
   lesson, one layer up: a ledger that outlives its subject reports coverage of nothing.

3. **Resolve every leg reference**, because a row may not claim coverage that does not
   exist. `dagger:<Func>` must name a real `func (m *FraiseqlCi)` AND a `dagger call
   <kebab-name>` in some workflow's `run:` block — a function no workflow calls is not CI
   coverage (`.dagger/release.go`'s PublishDryRun is exactly that: real, working, and
   reachable only from `make release-validate`). Comments are not invocations, so the
   search reads parsed `run:` steps rather than raw file text; dagger-image.yml's own
   header narrates `dagger call images` in prose, and a grep would have accepted it.

4. **Require an executing leg, or an exemption that self-clears.** An empty `executes`
   needs an exemption carrying an issue number. The exemption fails when it matches no
   artifact (stale) and when every artifact it matches has gained an executing leg (the
   gap closed and the row is now a lie). Both directions, because an exemption is a claim
   with an expiry date and nothing else notices when it passes.

Anything this parser cannot read is FATAL rather than skipped. A coverage gate that
silently drops what it does not understand reports a coverage it never checked, which is
worse than the absent gate it replaced.

Runs in preflight and in the Dagger ShellGates leg (python3, stdlib only — it reads
files and builds nothing). Locally: `make lint-delivery-coverage`.
"""

from __future__ import annotations

import importlib.util
import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "tools" / "delivery-artifacts.toml"
WORKFLOWS = REPO / ".github" / "workflows"
DAGGER = REPO / ".dagger"

# Matches tools/check-sdk-workflow-coverage.py's NOT_AN_SDK — sdks/official holds two
# directories that are not SDKs.
NOT_AN_SDK = {"conformance", "tests"}

# A workflow "publishes" if a run: step issues one of these. `--dry-run` forms are
# deliberately excluded: dart-sdk.yml's publish job runs `dart pub publish --dry-run` and
# publishes nothing (#1223), and counting it would let this gate report a delivery path
# that does not deliver.
PUBLISH_CMD = re.compile(
    r"(?:npm\s+publish|twine\s+upload|uv\s+publish|gem\s+push|mvn\s+.*\bdeploy\b|"
    r"dotnet\s+nuget\s+push|mix\s+hex\.publish|api/update-package|"
    r"cargo\s+publish(?!\s+--dry-run)|dart\s+pub\s+publish(?!\s+--dry-run))"
)

ERRORS: list[str] = []


def die(message: str) -> None:
    """A fault in the gate itself, not a finding: stop rather than report a partial pass."""
    print(f"delivery-coverage: FATAL — {message}", file=sys.stderr)
    raise SystemExit(2)


def err(message: str) -> None:
    ERRORS.append(message)


def _sibling(name: str, mod: str):
    """Import a sibling gate's parser rather than duplicating it.

    The ShellGates container is bare Ubuntu plus python3 — no PyYAML and no pip step — so
    a second copy is the only alternative, in a gate whose whole subject is copies
    drifting. A sibling that cannot be loaded is FATAL, never a skip.
    """
    path = REPO / "tools" / name
    spec = importlib.util.spec_from_file_location(mod, path)
    if spec is None or spec.loader is None:
        die(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    # Registered before execution: @dataclass in an imported module resolves its own
    # sys.modules[__module__], and an unregistered module makes that lookup return None.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


# ── Discovery ────────────────────────────────────────────────────────────────


def discover_images() -> set[str]:
    parity = _sibling("check-image-parity.py", "_fraiseql_image_parity")
    table = parity.parse_go_table(DAGGER / "image.go")
    if not table:
        die("zero image variants parsed from .dagger/image.go — the table moved")
    return {f"image:{name}" for name in table}


def discover_crates() -> set[str]:
    parity = _sibling("check-publish-parity.py", "_fraiseql_publish_parity")
    crates = parity.publishable_workspace_crates(REPO)
    if not crates:
        die("zero publishable crates found — the workspace manifest moved")
    return {f"crate:{c}" for c in crates}


def discover_sdks(runs_by_workflow: dict[str, list[str]]) -> set[str]:
    found: set[str] = set()

    official = REPO / "sdks" / "official"
    if not official.is_dir():
        die(f"no {official} — the SDK layout changed and this gate went blind")
    for d in sorted(official.iterdir()):
        if d.is_dir() and d.name not in NOT_AN_SDK:
            found.add(f"sdk:official/{d.name}")
    if not found:
        die("zero official SDKs found under sdks/official")

    # A community SDK is ours to deliver only when a workflow actually publishes it —
    # today just the Ruby gem. The others are contributed source we do not ship.
    community = REPO / "sdks" / "community"
    if community.is_dir():
        for d in sorted(community.iterdir()):
            if not d.is_dir():
                continue
            needle = f"sdks/community/{d.name}/"
            for wf, runs in runs_by_workflow.items():
                text = (WORKFLOWS / wf).read_text(encoding="utf-8")
                if needle in text and any(PUBLISH_CMD.search(r) for r in runs):
                    found.add(f"sdk:community/{d.name}")
                    break
    return found


def discover_chart() -> set[str]:
    charts = sorted((REPO / "deploy").rglob("Chart.yaml"))
    if not charts:
        die("no Chart.yaml under deploy/ — the chart moved")
    return {f"chart:{c.parent.name}" for c in charts}


def discover_compose() -> set[str]:
    """The canonical stack, read from the constant Phase 06's gate itself uses."""
    script = REPO / "tools" / "compose-stack-test.sh"
    m = re.search(r'^CANONICAL="([^"]+)"$', script.read_text(encoding="utf-8"), re.M)
    if not m:
        die(f"no CANONICAL= line in {script} — the canonical stack is unidentifiable")
    return {f"compose:{m.group(1)}"}


def discover_release_assets() -> set[str]:
    """The asset names release.yml's verify-release job asserts by name."""
    text = (WORKFLOWS / "release.yml").read_text(encoding="utf-8")
    m = re.search(r"^\s*EXPECTED=\(\n(.*?)^\s*\)$", text, re.M | re.S)
    if not m:
        die("no EXPECTED=( array in release.yml — the release-asset list moved")
    names = [line.strip() for line in m.group(1).splitlines() if line.strip()]
    if not names:
        die("the EXPECTED=( array in release.yml is empty")
    return {f"release-asset:{n}" for n in names}


# ── Workflow reading ─────────────────────────────────────────────────────────


def workflow_docs() -> dict[str, dict]:
    yaml = _sibling("check-suite-coverage.py", "_fraiseql_suite_coverage")
    docs: dict[str, dict] = {}
    files = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    if not files:
        die(f"no workflows under {WORKFLOWS}")
    for path in files:
        try:
            docs[path.name] = yaml.parse_yaml(path.read_text(encoding="utf-8"))
        except Exception as exc:  # noqa: BLE001 — an unreadable workflow is fatal
            die(f"cannot parse {path.name}: {exc}")
    return docs


def jobs_of(doc: dict) -> set[str]:
    jobs = doc.get("jobs")
    return set(jobs) if isinstance(jobs, dict) else set()


def runs_of(doc: dict) -> list[str]:
    """Every `run:` script in the workflow. Comments in the file are NOT invocations."""
    out: list[str] = []
    jobs = doc.get("jobs")
    if not isinstance(jobs, dict):
        return out
    for job in jobs.values():
        if not isinstance(job, dict):
            continue
        for step in job.get("steps") or []:
            if isinstance(step, dict) and isinstance(step.get("run"), str):
                out.append(step["run"])
    return out


def kebab(name: str) -> str:
    """Dagger's Go-function-name to CLI-verb conversion: ImagePropertiesAll → image-properties-all."""
    return re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower()


def dagger_functions() -> set[str]:
    found: set[str] = set()
    for path in sorted(DAGGER.glob("*.go")):
        found |= set(
            re.findall(
                r"^func \(m \*FraiseqlCi\) ([A-Z]\w*)\(", path.read_text(encoding="utf-8"), re.M
            )
        )
    if not found:
        die("no exported FraiseqlCi functions found in .dagger/ — the module moved")
    return found


# ── Leg resolution ───────────────────────────────────────────────────────────


def resolve_leg(leg: str, ctx: str, docs: dict, funcs: set[str], runs: dict) -> None:
    if leg.startswith("workflow:"):
        parts = leg.split(":")
        if len(parts) != 3:
            err(f"{ctx}: malformed leg {leg!r} — want workflow:<file>:<job>")
            return
        _, wf, job = parts
        if wf not in docs:
            err(f"{ctx}: leg {leg!r} names .github/workflows/{wf}, which does not exist")
        elif job not in jobs_of(docs[wf]):
            err(f"{ctx}: leg {leg!r} names job {job!r}, which {wf} does not define")
        return

    if leg.startswith("dagger:"):
        func = leg.split(":", 1)[1]
        if func not in funcs:
            err(f"{ctx}: leg {leg!r} names no `func (m *FraiseqlCi) {func}` in .dagger/")
            return
        verb = kebab(func)
        pattern = re.compile(rf"dagger\s+call\s+{re.escape(verb)}(?!\S)")
        if not any(pattern.search(r) for rs in runs.values() for r in rs):
            err(
                f"{ctx}: leg {leg!r} exists but no workflow run: step calls "
                f"`dagger call {verb}` — a function CI never invokes is not coverage"
            )
        return

    if leg.startswith("tool:"):
        rel = leg.split(":", 1)[1]
        if not (REPO / rel).is_file():
            err(f"{ctx}: leg {leg!r} names {rel}, which does not exist")
            return
        if not any(rel in r for rs in runs.values() for r in rs):
            err(
                f"{ctx}: leg {leg!r} exists but no workflow run: step invokes {rel} — "
                "a tool CI never runs is not coverage"
            )
        return

    err(f"{ctx}: leg {leg!r} has an unknown prefix — want workflow:, dagger: or tool:")


# ── Main ─────────────────────────────────────────────────────────────────────


def main() -> int:
    if not MANIFEST.is_file():
        die(f"no {MANIFEST}")
    try:
        data = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as exc:
        die(f"{MANIFEST.name} is not valid TOML: {exc}")

    docs = workflow_docs()
    runs = {name: runs_of(doc) for name, doc in docs.items()}
    funcs = dagger_functions()

    discovered = (
        discover_images()
        | discover_crates()
        | discover_sdks(runs)
        | discover_chart()
        | discover_compose()
        | discover_release_assets()
    )

    rows = data.get("artifact")
    if not isinstance(rows, list) or not rows:
        die(f"{MANIFEST.name} declares no [[artifact]] rows")

    by_id: dict[str, dict] = {}
    for i, row in enumerate(rows):
        rid = row.get("id")
        if not isinstance(rid, str) or not rid:
            die(f"{MANIFEST.name}: [[artifact]] #{i + 1} has no id")
        if rid in by_id:
            err(f"artifact {rid!r} is declared twice")
        for key in ("kind", "consumed_as"):
            if not isinstance(row.get(key), str):
                err(f"artifact {rid!r}: {key} is missing or not a string")
        for key in ("publishes", "builds", "executes"):
            if not isinstance(row.get(key), list):
                err(f"artifact {rid!r}: {key} is missing or not a list")
        by_id[rid] = row

    # ── bidirectional: discovery ↔ ledger ────────────────────────────────────
    for rid in sorted(discovered - set(by_id)):
        err(
            f"{rid} ships and has no row in {MANIFEST.name} — a new delivery artifact "
            "cannot arrive ungated; add a row naming the leg that executes it"
        )
    for rid in sorted(set(by_id) - discovered):
        err(
            f"{rid} has a row in {MANIFEST.name} but discovery no longer finds it — "
            "the artifact stopped shipping, or its discovery source moved; delete the row"
        )

    # ── every leg named must exist and be wired into CI ──────────────────────
    for rid, row in by_id.items():
        for key in ("publishes", "builds", "executes"):
            for leg in row.get(key) or []:
                if not isinstance(leg, str):
                    err(f"artifact {rid!r}: {key} contains a non-string entry")
                    continue
                resolve_leg(leg, f"artifact {rid!r} {key}", docs, funcs, runs)

    # ── executes is the gated column ─────────────────────────────────────────
    exemptions = data.get("exempt") or []
    if not isinstance(exemptions, list):
        die(f"{MANIFEST.name}: [[exempt]] is not a list")

    def matches(pattern: str, rid: str) -> bool:
        if pattern.endswith(":*"):
            return rid.startswith(pattern[:-1])
        return pattern == rid

    exempted: dict[str, list[str]] = {rid: [] for rid in by_id}
    for i, ex in enumerate(exemptions):
        pat = ex.get("applies_to")
        issue = ex.get("issue")
        reason = ex.get("reason")
        label = f"exemption #{i + 1} ({pat!r})"
        if not isinstance(pat, str) or not pat:
            die(f"{MANIFEST.name}: [[exempt]] #{i + 1} has no applies_to")
        if not isinstance(issue, int):
            err(f"{label}: issue is missing or not an integer — an exemption must name the issue that owns the gap")
        if not isinstance(reason, str) or not reason.strip():
            err(f"{label}: reason is missing or empty")

        hit = [rid for rid in by_id if matches(pat, rid)]
        if not hit:
            err(
                f"{label}: matches no artifact — stale. The thing it excused stopped "
                "shipping; delete the exemption"
            )
            continue
        covered = [rid for rid in hit if by_id[rid].get("executes")]
        if len(covered) == len(hit):
            err(
                f"{label}: every artifact it matches now HAS an executing leg "
                f"({', '.join(sorted(covered))}) — the gap it describes has closed; "
                "delete the exemption"
            )
        elif covered:
            err(
                f"{label}: too broad — {len(covered)} of {len(hit)} artifacts it matches "
                f"already have an executing leg ({', '.join(sorted(covered))}); narrow it "
                "so it excuses only what is still uncovered"
            )
        for rid in hit:
            exempted[rid].append(pat)

    for rid in sorted(by_id):
        has_exec = bool(by_id[rid].get("executes"))
        pats = exempted[rid]
        if not has_exec and not pats:
            err(
                f"{rid} has no leg that EXECUTES it and no exemption. Either name the leg "
                "that runs it and requires a working answer, or add an [[exempt]] row with "
                "the issue that owns the gap — \"the artifact exists\" is not a test"
            )
        if len(pats) > 1:
            err(f"{rid} is matched by {len(pats)} exemptions ({', '.join(pats)}) — exactly one must own it")

    if ERRORS:
        print("delivery-coverage: FAIL\n", file=sys.stderr)
        for message in ERRORS:
            print(f"  {message}", file=sys.stderr)
        print(
            f"\n  {len(ERRORS)} finding(s). The ledger is {MANIFEST.relative_to(REPO)}.",
            file=sys.stderr,
        )
        return 1

    executed = sum(1 for row in rows if row.get("executes"))
    print(
        f"delivery-coverage: OK — {len(rows)} shipped artifact(s) accounted for; "
        f"{executed} executed by a leg, {len(rows) - executed} exempt with an issue."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
