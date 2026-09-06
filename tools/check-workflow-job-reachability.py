#!/usr/bin/env python3
"""No job may be gated on an event or a ref its own workflow cannot receive.

On 2026-05-31 the Dagger migration stripped `push`-to-branch and `pull_request`
triggers from a dozen workflows. It stripped the **triggers**; it left the **job
conditions that referenced them** behind. `docker-build.yml` — the file that
builds and publishes every image this project ships — kept two jobs that read as
image coverage and could not run at all:

  * `test-images`, `if: github.event_name == 'pull_request'`, in a workflow whose
    `on:` is `push: tags: ['v*']` plus `workflow_dispatch`. There is no PR trigger.
  * `verify-deployment`, `if: … github.ref == 'refs/heads/main'`, where the only
    push events are tag pushes, so the ref is never a branch.

Neither had ever run. Nothing said so: an unreachable job is not skipped-and-
reported, it is absent from the checks list entirely, which is indistinguishable
from a workflow that simply has fewer jobs.

What is checked, per workflow, for every job-level AND step-level `if:`:

  A. **unreachable** — the condition is FALSE under every event the workflow's
     `on:` can deliver. The job can never run.
  B. **dead arm** — one `github.event_name` / `github.ref` comparison inside the
     condition can never contribute: in every world where that comparison could
     be true, the surrounding condition is still false. This is what catches the
     `main` and `release/*` arms of a job that is otherwise reachable on a tag.
  C. **vacuous arm** — the comparison is true under *every* world. It names an
     event the workflow cannot receive in the mirror image: `!= 'pull_request'`
     in a workflow that has no `pull_request` trigger reads as "skipped on PRs"
     and is a constant.

Steps were left out of the first pass because their repair is a decision, not a
deletion: most of the dead ones are *report-the-result* steps (post a PR comment,
save a baseline for the next comparison, commit regenerated diagrams), and
deleting them leaves a workflow that measures and discards. #1207 took those
seventeen decisions; the gate now covers steps so the class cannot come back.

Sixteen of those seventeen could never run and one was a constant. Two were worth
reading twice: `benchmarks.yml`'s `criterion-benchmarks` job had five conditional
steps and ALL FIVE were dead, so the job ran `cargo bench` and did nothing with
the result; and `generate-d2-diagrams.yml` regenerated the diagrams and could not
persist them — its "Commit changes" step was push-only in a dispatch-only
workflow, so dispatching it produced an artifact-free no-op.

Modelling, and where it is deliberately conservative — a world is one
(event, ref-space) pair the `on:` block can deliver:

  push (branches)  push (tags)  pull_request  workflow_dispatch (branch|tag)
  schedule / workflow_run → ref UNKNOWN      workflow_call → event UNKNOWN too

Anything the model cannot decide evaluates to MAYBE, and MAYBE never produces a
finding. A `workflow_call` workflow runs in its *caller's* event context, so
every event and ref atom there is MAYBE and no job in it is ever flagged.

Overrides, for testing:
  WORKFLOW_REACHABILITY_ROOT=<dir>   tree to scan instead of the repo root
"""

from __future__ import annotations

import importlib.util
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def scan_root() -> Path:
    env = os.environ.get("WORKFLOW_REACHABILITY_ROOT")
    return Path(env) if env else REPO


def die(message: str) -> None:
    print(f"FATAL: {message}", file=sys.stderr)
    raise SystemExit(2)


def _yaml_module():
    """`parse_yaml` / `YamlError` from tools/check-suite-coverage.py.

    One hand-written YAML-subset parser serves both gates. The ShellGates
    container is bare Ubuntu plus python3 — no PyYAML, no pip step — so the
    choice is between importing that parser and keeping a second copy of it that
    drifts from it. A missing or unloadable sibling is FATAL, never a skip: a
    reachability gate that quietly scans nothing is the failure it exists to
    prevent.
    """
    path = REPO / "tools" / "check-suite-coverage.py"
    spec = importlib.util.spec_from_file_location("_fraiseql_suite_coverage", path)
    if spec is None or spec.loader is None:
        die(f"cannot load the YAML parser from {path}")
    module = importlib.util.module_from_spec(spec)
    # Registered before execution: `@dataclass` in an imported module resolves its
    # own `sys.modules[__module__]`, and an unregistered module makes that lookup
    # return None (AttributeError on 3.14, not an import error naming the cause).
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


# ── The world model ──────────────────────────────────────────────────────────

TRUE, MAYBE, FALSE = 1, 0, -1

# Events whose ref this gate does not model. `schedule` and `workflow_run` run on
# the default branch, whose name is repo configuration rather than a property of
# the file being read; the rest carry refs that no condition in this repo tests.
UNKNOWN_REF_EVENTS = {
    "schedule",
    "workflow_run",
    "repository_dispatch",
    "deployment",
    "deployment_status",
}

# Events that carry a tag ref.
TAG_REF_EVENTS = {"release"}


@dataclass(frozen=True)
class RefSpace:
    """Which refs one trigger can deliver."""

    kind: str  # "branch" | "tag" | "pull" | "unknown"
    include: tuple[str, ...] = ("**",)
    exclude: tuple[str, ...] = ()


@dataclass(frozen=True)
class World:
    """One (event, ref-space) context the workflow can actually receive."""

    event: str | None  # None: the caller's event (workflow_call) — unconstrained
    refs: RefSpace
    label: str

    def __str__(self) -> str:
        return self.label


def _as_trigger_map(on) -> dict:
    """`on:` in its three spellings → {event: config}."""
    if isinstance(on, str):
        return {on: None}
    if isinstance(on, list):
        out = {}
        for item in on:
            if not isinstance(item, str):
                die(f"unreadable `on:` list entry {item!r}")
            out[item] = None
        return out
    if isinstance(on, dict):
        return on
    die(f"unreadable `on:` block: {on!r}")
    return {}


def _patterns(cfg: dict, include_key: str, ignore_key: str) -> tuple[tuple[str, ...], tuple[str, ...]]:
    include = cfg.get(include_key)
    ignore = cfg.get(ignore_key)
    inc: list[str] = []
    exc: list[str] = []
    for raw, sink in ((include, inc), (ignore, exc)):
        if raw is None:
            continue
        if not isinstance(raw, list):
            die(f"`{include_key}`/`{ignore_key}` is not a list: {raw!r}")
        sink.extend(str(p) for p in raw)
    # A `!pattern` inside the include list is an exclusion.
    for pat in list(inc):
        if pat.startswith("!"):
            inc.remove(pat)
            exc.append(pat[1:])
    if not inc:
        inc = ["**"]
    return tuple(inc), tuple(exc)


def worlds_for(on) -> list[World]:
    """Every (event, ref-space) the `on:` block can deliver."""
    worlds: list[World] = []
    for event, cfg in _as_trigger_map(on).items():
        cfg = cfg if isinstance(cfg, dict) else {}

        if event == "workflow_call":
            # A reusable workflow runs in the caller's context: any event, any ref.
            worlds.append(World(None, RefSpace("unknown"), "workflow_call (caller's event)"))
        elif event in ("push",):
            has_branches = "branches" in cfg or "branches-ignore" in cfg
            has_tags = "tags" in cfg or "tags-ignore" in cfg
            if has_branches or not has_tags:
                inc, exc = _patterns(cfg, "branches", "branches-ignore")
                worlds.append(World(event, RefSpace("branch", inc, exc), f"push (branches: {', '.join(inc)})"))
            if has_tags or not has_branches:
                inc, exc = _patterns(cfg, "tags", "tags-ignore")
                worlds.append(World(event, RefSpace("tag", inc, exc), f"push (tags: {', '.join(inc)})"))
        elif event in ("pull_request", "pull_request_target"):
            worlds.append(World(event, RefSpace("pull"), event))
        elif event == "workflow_dispatch":
            # Dispatch targets any branch, and any tag, that exists.
            worlds.append(World(event, RefSpace("branch"), "workflow_dispatch (a branch)"))
            worlds.append(World(event, RefSpace("tag"), "workflow_dispatch (a tag)"))
        elif event in TAG_REF_EVENTS:
            worlds.append(World(event, RefSpace("tag"), event))
        elif event in UNKNOWN_REF_EVENTS:
            worlds.append(World(event, RefSpace("unknown"), event))
        else:
            # An event this gate has not been taught: known name, unmodelled ref.
            worlds.append(World(event, RefSpace("unknown"), event))
    return worlds


# ── GitHub's ref filter patterns ─────────────────────────────────────────────

# `*` and `?` stop at `/`; `**` crosses it. `+` and character classes are legal
# in GitHub filter patterns and appear nowhere in this repo: rather than guess at
# them, the gate refuses — a wrong pattern translation is a wrong verdict.
_UNSUPPORTED_PATTERN = re.compile(r"[+\[\]]")


def _pattern_re(pattern: str) -> re.Pattern[str]:
    if _UNSUPPORTED_PATTERN.search(pattern):
        die(f"ref filter pattern {pattern!r} uses `+` or a character class — teach the gate this shape")
    out: list[str] = []
    i = 0
    while i < len(pattern):
        if pattern.startswith("**", i):
            out.append(".*")
            i += 2
        elif pattern[i] == "*":
            out.append("[^/]*")
            i += 1
        elif pattern[i] == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(pattern[i]))
            i += 1
    return re.compile("".join(out) + r"\Z")


def _matches(space: RefSpace, name: str) -> bool:
    if any(_pattern_re(p).match(name) for p in space.exclude):
        return False
    return any(_pattern_re(p).match(name) for p in space.include)


# Concrete ref names probed when asking "can this space produce a ref starting
# with P?". A glob cannot be inverted in general; these cover every shape this
# repo's filters use (`**`, `v*`, `release/*`, `dev`, literal tag prefixes).
_PROBES = ("", "x", "0", "1.0.0", "main", "x/y", "1.0.0/x")


def _prefix_possible(space: RefSpace, prefix: str) -> bool:
    if any(_matches(space, prefix + probe) for probe in _PROBES):
        return True
    # A literal include pattern that itself begins with the prefix: `tags:
    # ['elixir-sdk/v*']` against `startsWith(ref, 'refs/tags/elixir-sdk/')`.
    return any(
        _matches(space, pat) for pat in space.include if not re.search(r"[*?]", pat) and pat.startswith(prefix)
    )


def _literal_prefix(pattern: str) -> str:
    """The part of a filter pattern before its first wildcard."""
    cut = min((i for i in (pattern.find("*"), pattern.find("?")) if i >= 0), default=len(pattern))
    return pattern[:cut]


def ref_equals(world: World, value: str) -> int:
    """TRUE only if EVERY ref this world delivers is `value`, MAYBE if some is.

    A world is a set of refs, not one ref: `push: branches: [dev, release/*]` can
    deliver `refs/heads/dev`, so the comparison is satisfiable — but it is not a
    constant, and reporting it as one would delete a live arm.
    """
    space = world.refs
    if space.kind == "unknown":
        return MAYBE
    if space.kind == "pull":
        return MAYBE if re.fullmatch(r"refs/pull/[^/]+/(merge|head)", value) else FALSE
    prefix = "refs/heads/" if space.kind == "branch" else "refs/tags/"
    if not value.startswith(prefix):
        return FALSE
    name = value[len(prefix) :]
    if not _matches(space, name):
        return FALSE
    only_this = space.include == (name,) and not space.exclude and _literal_prefix(name) == name
    return TRUE if only_this else MAYBE


def ref_starts_with(world: World, value: str) -> int:
    space = world.refs
    if space.kind == "unknown":
        return MAYBE
    if space.kind == "pull":
        return MAYBE if "refs/pull/".startswith(value) or value.startswith("refs/pull/") else FALSE
    prefix = "refs/heads/" if space.kind == "branch" else "refs/tags/"
    if len(value) <= len(prefix):
        # Every ref in a branch or tag space carries the whole prefix.
        return TRUE if prefix.startswith(value) else FALSE
    if not value.startswith(prefix):
        return FALSE
    rest = value[len(prefix) :]
    if not _prefix_possible(space, rest):
        return FALSE
    certain = all(_literal_prefix(pat).startswith(rest) for pat in space.include)
    return TRUE if certain else MAYBE


def event_equals(world: World, name: str) -> int:
    if world.event is None:
        return MAYBE
    return TRUE if world.event == name else FALSE


PR_EVENTS = {"pull_request", "pull_request_target"}


def pr_payload_absent(name: str | None, world: World) -> bool:
    """`github.event.pull_request…` is present only on a pull_request event.

    Without this, `contains(github.event.pull_request.labels.*.name, 'perf')` is
    an unknown, and a job whose only surviving arm is that expression looks
    reachable in a workflow that can never deliver a PR. It is not: the context
    is undefined, `.labels.*.name` over it is empty, and `contains` is false.
    """
    if name is None or not name.startswith("github.event.pull_request"):
        return False
    if world.event is None:  # workflow_call: the caller's event may well be a PR
        return False
    return world.event not in PR_EVENTS


# ── GitHub expression syntax ─────────────────────────────────────────────────


class ExprError(Exception):
    pass


@dataclass
class Node:
    span: tuple[int, int]


@dataclass
class Lit(Node):
    value: object


@dataclass
class Context(Node):
    name: str


@dataclass
class Call(Node):
    name: str
    args: list[Node]


@dataclass
class Cmp(Node):
    op: str
    left: Node
    right: Node


@dataclass
class Not(Node):
    operand: Node


@dataclass
class Bool(Node):
    op: str  # "&&" | "||"
    parts: list[Node]


_TOKEN = re.compile(
    r"""
      (?P<ws>\s+)
    | (?P<str>'(?:[^']|'')*')
    | (?P<num>-?\d+(?:\.\d+)?)
    | (?P<op>&&|\|\||==|!=|<=|>=|!|<|>|\(|\)|,|\[|\])
    | (?P<path>[A-Za-z_][A-Za-z0-9_.\-*]*)
    """,
    re.VERBOSE,
)


def tokenize(text: str) -> list[tuple[str, str, int, int]]:
    out: list[tuple[str, str, int, int]] = []
    i = 0
    while i < len(text):
        m = _TOKEN.match(text, i)
        if not m:
            raise ExprError(f"unreadable character {text[i]!r} at offset {i}")
        kind = m.lastgroup or ""
        if kind != "ws":
            out.append((kind, m.group(), m.start(), m.end()))
        i = m.end()
    return out


class Parser:
    """Recursive descent over the subset of GitHub expressions job `if:`s use."""

    def __init__(self, text: str):
        self.text = text
        self.toks = tokenize(text)
        self.i = 0

    def peek(self) -> tuple[str, str, int, int] | None:
        return self.toks[self.i] if self.i < len(self.toks) else None

    def take(self) -> tuple[str, str, int, int]:
        tok = self.peek()
        if tok is None:
            raise ExprError("unexpected end of expression")
        self.i += 1
        return tok

    def accept(self, value: str) -> bool:
        tok = self.peek()
        if tok and tok[1] == value:
            self.i += 1
            return True
        return False

    def expect(self, value: str) -> None:
        if not self.accept(value):
            got = self.peek()
            raise ExprError(f"expected {value!r}, got {got[1]!r}" if got else f"expected {value!r}")

    def parse(self) -> Node:
        node = self.parse_or()
        if self.peek() is not None:
            raise ExprError(f"trailing input at {self.peek()[1]!r}")
        return node

    def parse_or(self) -> Node:
        parts = [self.parse_and()]
        while self.accept("||"):
            parts.append(self.parse_and())
        if len(parts) == 1:
            return parts[0]
        return Bool((parts[0].span[0], parts[-1].span[1]), "||", parts)

    def parse_and(self) -> Node:
        parts = [self.parse_cmp()]
        while self.accept("&&"):
            parts.append(self.parse_cmp())
        if len(parts) == 1:
            return parts[0]
        return Bool((parts[0].span[0], parts[-1].span[1]), "&&", parts)

    def parse_cmp(self) -> Node:
        left = self.parse_unary()
        tok = self.peek()
        if tok and tok[1] in ("==", "!=", "<", "<=", ">", ">="):
            self.take()
            right = self.parse_unary()
            return Cmp((left.span[0], right.span[1]), tok[1], left, right)
        return left

    def parse_unary(self) -> Node:
        tok = self.peek()
        if tok and tok[1] == "!":
            self.take()
            operand = self.parse_unary()
            return Not((tok[2], operand.span[1]), operand)
        return self.parse_primary()

    def parse_primary(self) -> Node:
        tok = self.take()
        kind, value, start, end = tok
        if value == "(":
            inner = self.parse_or()
            self.expect(")")
            return inner
        if kind == "str":
            return Lit((start, end), value[1:-1].replace("''", "'"))
        if kind == "num":
            return Lit((start, end), float(value))
        if kind == "path":
            if value in ("true", "false"):
                return Lit((start, end), value == "true")
            if value == "null":
                return Lit((start, end), None)
            if self.accept("("):
                args: list[Node] = []
                if not self.accept(")"):
                    args.append(self.parse_or())
                    while self.accept(","):
                        args.append(self.parse_or())
                    self.expect(")")
                close = self.toks[self.i - 1][3]
                return Call((start, close), value, args)
            return Context((start, end), value)
        raise ExprError(f"unexpected token {value!r}")


def strip_wrapper(condition: str) -> str:
    """`${{ … }}` is optional around a job `if:`; both spellings are one expression."""
    text = condition.strip()
    m = re.fullmatch(r"\$\{\{(.*)\}\}", text, re.S)
    return m.group(1).strip() if m else text


# ── Three-valued evaluation ──────────────────────────────────────────────────


def evaluate(node: Node, world: World, pinned: dict[int, int] | None = None) -> int:
    """TRUE / MAYBE / FALSE for `node` in `world`.

    Kleene logic over the atoms this gate models; every other sub-expression is
    MAYBE. Shared unknowns are not tracked (`x && !x` is MAYBE, not FALSE), which
    makes the evaluation an over-approximation of satisfiability — it can only
    ever fail to report, never report something reachable.
    """
    if pinned and id(node) in pinned:
        return pinned[id(node)]

    if isinstance(node, Bool):
        values = [evaluate(p, world, pinned) for p in node.parts]
        return min(values) if node.op == "&&" else max(values)
    if isinstance(node, Not):
        return -evaluate(node.operand, world, pinned)
    if isinstance(node, Cmp):
        return _evaluate_cmp(node, world)
    if isinstance(node, Call):
        return _evaluate_call(node, world)
    if isinstance(node, Lit):
        if isinstance(node.value, bool):
            return TRUE if node.value else FALSE
        return MAYBE
    if isinstance(node, Context):
        # An absent context is null, and null is falsy.
        return FALSE if pr_payload_absent(node.name, world) else MAYBE
    return MAYBE


def _literal_of(node: Node) -> str | None:
    return node.value if isinstance(node, Lit) and isinstance(node.value, str) else None


def _context_of(node: Node) -> str | None:
    return node.name if isinstance(node, Context) else None


def _evaluate_cmp(node: Cmp, world: World) -> int:
    if node.op not in ("==", "!="):
        return MAYBE
    for left, right in ((node.left, node.right), (node.right, node.left)):
        context, literal = _context_of(left), _literal_of(right)
        if literal is None:
            continue
        if pr_payload_absent(context, world) and literal != "":
            # null never equals a non-empty string.
            return FALSE if node.op == "==" else TRUE
        if context == "github.event_name":
            value = event_equals(world, literal)
            return value if node.op == "==" else -value
        if context == "github.ref":
            value = ref_equals(world, literal)
            return value if node.op == "==" else -value
    return MAYBE


def _evaluate_call(node: Call, world: World) -> int:
    if node.name in ("contains", "startsWith", "endsWith") and len(node.args) == 2:
        subject = _context_of(node.args[0])
        if pr_payload_absent(subject, world):
            return FALSE
        if node.name == "startsWith" and subject == "github.ref":
            literal = _literal_of(node.args[1])
            if literal is not None:
                return ref_starts_with(world, literal)
    return MAYBE


def is_modelled_atom(node: Node, worlds: list[World]) -> bool:
    """True for the comparisons whose value this gate can actually decide."""
    if not isinstance(node, (Cmp, Call)):
        return False
    probe = World("__probe__", RefSpace("branch"), "probe")
    if isinstance(node, Cmp):
        return _evaluate_cmp(node, probe) != MAYBE or any(_evaluate_cmp(node, w) != MAYBE for w in worlds)
    return _evaluate_call(node, probe) != MAYBE or any(_evaluate_call(node, w) != MAYBE for w in worlds)


def atoms(node: Node) -> list[Node]:
    if isinstance(node, Bool):
        return [a for part in node.parts for a in atoms(part)]
    if isinstance(node, Not):
        return atoms(node.operand)
    if isinstance(node, (Cmp, Call)):
        return [node]
    return []


# ── The gate ─────────────────────────────────────────────────────────────────


@dataclass
class Finding:
    workflow: str
    line: int
    subject: str
    kind: str
    detail: str


def job_line(text: str, job_id: str) -> int:
    for lineno, line in enumerate(text.splitlines(), start=1):
        if re.match(rf"^\s+{re.escape(job_id)}:\s*(#.*)?$", line):
            return lineno
    return 0


def step_line(text: str, label: str, fallback: int) -> int:
    """Line of `- name: <label>` (or `- uses: <label>`), else the job's line.

    A wrong line number is worse than none, so this matches the whole scalar and
    falls back rather than guessing.
    """
    for key in ("name", "uses"):
        for lineno, line in enumerate(text.splitlines(), start=1):
            if re.match(rf"^\s+-\s+{key}:\s+{re.escape(label)}\s*$", line):
                return lineno
    return fallback


def step_label(step: dict, index: int) -> str:
    """How a step is named in a finding. Every step has one of these three."""
    for key in ("name", "uses"):
        value = step.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return f"step #{index + 1}"


def check_workflow(path: Path, yaml) -> tuple[list[Finding], int]:
    text = path.read_text(encoding="utf-8")
    try:
        doc = yaml.parse_yaml(text)
    except yaml.YamlError as exc:
        die(f"{path.name}: {exc} — teach tools/check-suite-coverage.py this YAML shape")
    if not isinstance(doc, dict):
        die(f"{path.name}: not a mapping")
    if "on" not in doc:
        die(f"{path.name}: no `on:` block — teach the gate this workflow shape")
    jobs = doc.get("jobs")
    if not isinstance(jobs, dict):
        die(f"{path.name}: no `jobs:` mapping — teach the gate this workflow shape")

    worlds = worlds_for(doc["on"])
    if not worlds:
        die(f"{path.name}: `on:` delivers no event at all")
    trigger_summary = ", ".join(str(w) for w in worlds)

    findings: list[Finding] = []
    conditional = 0

    def analyse(condition, subject: str, line: int, never: str) -> None:
        """Three checks over one `if:`. `never` names what cannot happen (A)."""
        nonlocal conditional
        if not isinstance(condition, str):
            die(f"{path.name}:{subject}: `if:` is not a scalar: {condition!r}")
        conditional += 1
        source = strip_wrapper(condition)
        try:
            expr = Parser(source).parse()
        except ExprError as exc:
            die(f"{path.name}:{subject}: cannot parse `if:` ({exc}) — teach the gate this expression")

        collapsed = " ".join(source.split())

        if all(evaluate(expr, w) == FALSE for w in worlds):
            findings.append(
                Finding(
                    path.name,
                    line,
                    subject,
                    never,
                    f"if: {collapsed}\n      this workflow receives: {trigger_summary}",
                )
            )
            return

        for atom in atoms(expr):
            if not is_modelled_atom(atom, worlds):
                continue
            arm = " ".join(source[atom.span[0] : atom.span[1]].split())
            live = any(
                evaluate(atom, w) != FALSE and evaluate(expr, w, {id(atom): TRUE}) != FALSE for w in worlds
            )
            if not live:
                findings.append(
                    Finding(
                        path.name,
                        line,
                        subject,
                        "dead condition arm",
                        f"`{arm}` can never contribute\n      full: if: {collapsed}"
                        f"\n      this workflow receives: {trigger_summary}",
                    )
                )
            elif all(evaluate(atom, w) == TRUE for w in worlds):
                findings.append(
                    Finding(
                        path.name,
                        line,
                        subject,
                        "vacuous condition arm",
                        f"`{arm}` is true under every trigger this workflow has"
                        f"\n      full: if: {collapsed}"
                        f"\n      this workflow receives: {trigger_summary}",
                    )
                )

    for job_id, job in jobs.items():
        if not isinstance(job, dict):
            die(f"{path.name}:{job_id}: unreadable job")

        line = job_line(text, job_id)
        if job.get("if") is not None:
            analyse(job["if"], f"job `{job_id}`", line, "can never run")

        # Step-level `if:`. A dead step is quieter than a dead job — the job runs,
        # reports success, and simply never does the part the step was for (#1207).
        steps = job.get("steps")
        if steps is None:
            continue
        if not isinstance(steps, list):
            die(f"{path.name}:{job_id}: `steps:` is not a list — teach the gate this workflow shape")
        for index, step in enumerate(steps):
            if not isinstance(step, dict):
                die(f"{path.name}:{job_id}: unreadable step #{index + 1}")
            if step.get("if") is None:
                continue
            label = step_label(step, index)
            analyse(
                step["if"],
                f"job `{job_id}` step [{label}]",
                step_line(text, label, line),
                "can never run",
            )

    return findings, conditional


# Third-party trees. A dependency ships its own `.github/workflows` for its own
# repository; those are not this repository's to delete, and walking them is
# most of the cost of the scan. Path-based rather than `git ls-files`: the
# ShellGates container has no git index — `git ls-files` returns NOTHING there,
# and a gate that filters on it would pass by scanning nothing.
NESTED_SCAN_SKIP = frozenset(
    {".git", ".venv", "__pycache__", "dist", "node_modules", "target", "vendor"}
)


def nested_workflow_dirs(root: Path) -> list[Path]:
    """Every `.github/workflows` directory BELOW the repository root.

    GitHub Actions reads workflows only from `.github/workflows/` at the
    repository root, so a nested one is unreachable by construction: it cannot
    run, cannot be dispatched, and cannot even be reported as skipped. It is the
    #1206/#1207 class — an artifact that reads as CI coverage and provides none
    — and the two found when this check was added had never run once.

    Both were the furniture of a repository that had been merged in:
    `crates/fraiseql-wire` (four workflows, triggers naming `main`/`develop`
    when this trunk is `dev`) and `sdks/official/fraiseql-php` (one, a weaker
    unpinned copy of the root's own `php-sdk.yml`) — #1233.
    """
    root_workflows = (root / ".github" / "workflows").resolve()
    found: list[Path] = []

    for dirpath, dirnames, _filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in NESTED_SCAN_SKIP]
        here = Path(dirpath)
        if here.name != "workflows" or here.parent.name != ".github":
            continue
        if here.resolve() == root_workflows:
            continue
        found.append(here)

    return sorted(found)


def main() -> int:
    yaml = _yaml_module()
    root = scan_root()
    directory = root / ".github" / "workflows"
    if not directory.is_dir():
        print(f"FATAL: no workflow directory at {directory}", file=sys.stderr)
        return 1

    paths = sorted(directory.glob("*.yml")) + sorted(directory.glob("*.yaml"))
    # The vacuous-scan guard: a gate that reports success having read nothing is
    # the exact failure mode it exists to catch.
    if not paths:
        print(f"FATAL: no workflows found under {directory}", file=sys.stderr)
        return 1

    nested = nested_workflow_dirs(root)
    if nested:
        print(
            "Nested `.github/workflows` directories — GitHub Actions reads workflows only\n"
            "from the repository root, so nothing below has ever run or can run:\n"
        )
        unreachable = 0
        for directory_path in nested:
            for workflow in sorted(directory_path.glob("*.yml")) + sorted(
                directory_path.glob("*.yaml")
            ):
                print(f"  {workflow.relative_to(root)}")
                unreachable += 1
            if not any(directory_path.iterdir()):
                print(f"  {directory_path.relative_to(root)}  (empty)")
        print(
            f"\n{unreachable} unreachable workflow file(s) in "
            f"{len(nested)} nested directory(ies).\n"
            "Delete them, or move the workflow to .github/workflows/ at the repository\n"
            "root and give it triggers this repository actually delivers. Keeping one\n"
            "where it is, is choosing an artifact that reads as coverage and provides none."
        )
        return 1

    findings: list[Finding] = []
    conditional = 0
    for path in paths:
        found, count = check_workflow(path, yaml)
        findings.extend(found)
        conditional += count

    if findings:
        print("Workflow jobs and steps gated on events their workflow cannot receive:\n")
        for f in findings:
            print(f"  {f.workflow}:{f.line}  {f.subject} — {f.kind}")
            print(f"      {f.detail}\n")
        print(
            f"{len(findings)} finding(s) across {len(paths)} workflows "
            f"({conditional} conditional jobs and steps).\n"
            "Delete it, or give the workflow a result it keeps without the trigger.\n"
            "Restoring the trigger it names is a CI-load decision, not a repair —\n"
            "take it deliberately, not to silence this gate."
        )
        return 1

    print(
        f"workflow reachability: {len(paths)} workflows, {conditional} conditional jobs and steps — "
        "every one can run, every arm can matter"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
