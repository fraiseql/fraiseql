#!/usr/bin/env python3
"""The no-orphan-suites gate: every test suite maps to a CI leg that executes it.

"Execution coverage must be a checked artifact, not an inference" — the
2026-07-27 remediation program's retrospective, rule 1. This gate makes it so:

1. **Discover what exists** — every `tests/*.rs` integration binary (with its
   `[[test]] required-features`) and every feature-gated `tests.rs` unit-test
   module under `src/`, per workspace crate.
2. **Discover what runs** — every `cargo test` / `cargo nextest` invocation in
   `.dagger/main.go`, attributed to its leg function, with feature lists
   resolved and the leg's bound services (DATABASE_URL etc.) recorded; and
   every such invocation in a `run:` block under `.github/workflows/`,
   attributed as `<workflow>:<job>`, for the suites that need a networked
   runner and so cannot live in an offline Dagger leg (#1120). The gate reads
   the SAME files CI executes, so gate and CI cannot drift.

   The workflow side is held to a stricter standard, because a workflow can
   look like coverage and provide none: a `workflow_dispatch:`-only trigger, a
   `working-directory` in another Cargo workspace, `--bench`, or a `paths:`
   filter that the suite's own source does not match. Each is resolved in full
   and then discounted with a printed reason — and an invocation the parser
   cannot resolve is fatal rather than dropped, since a parser that quietly
   drops what it cannot read reports coverage that does not exist, which is
   worse than the exemption it replaced.
3. **Fail loud** when a suite is claimed by no leg:
   - a test binary no invocation runs (feature-gated out, crate excluded, or
     simply never named);
   - a binary whose backing services (`postgres`, `redis`, …) are bound by NO
     leg that runs it — a self-skipping suite executing only in service-less
     legs reads green forever without asserting anything (#960's shape);
   - a feature-gated lib test module compiled out of, or filtered out of,
     every lib invocation (#981's shape);
   - the reverse direction: an invocation naming a `--test` binary that no
     longer exists (#883's lesson).

Exemptions are opt-out with a published reason: tools/suite-coverage-exemptions.toml.

Runs in preflight ShellGates (python3, stdlib only) and locally via
`make lint-suite-coverage`.
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DAGGER_MAIN = REPO / ".dagger" / "main.go"
EXEMPTIONS = REPO / "tools" / "suite-coverage-exemptions.toml"

# Service-need detection: if a test source READS the service's env var (or
# calls the test-support getter), its assertions need that backing service, so
# only a leg binding one of the group's env vars counts as covering it. The
# patterns match read shapes, not name mentions — an error-message test that
# merely names DATABASE_URL is not a database test.
ENV_READ = r'(?:var|var_os|env!)\(\s*"{name}"'
SERVICES = {
    "postgres": {
        "detect": re.compile(
            r"(?:test_support|services)::postgres\(|try_database_url\(|\bdatabase_url\(\)|"
            + ENV_READ.format(name="(?:TLS_)?DATABASE_URL")
        ),
        "satisfied_by": {"DATABASE_URL", "TLS_DATABASE_URL"},
    },
    # A streaming standby is a *distinct* service from the primary (#957): a leg
    # can bind Postgres and still have no replica to read from, which is exactly
    # how `tenant_schema_isolation_e2e_pg` reached the `server` suite needing a
    # standby that suite never bound. `standby_database_url()` panics rather than
    # skipping, so it failed loudly — but only once the leg ran, and no gate said
    # so first. `\b` before `standby_database_url` keeps the failover getter (whose
    # name ends with it) out of this group.
    "pg_standby": {
        "detect": re.compile(
            r"\bstandby_database_url\(\)|" + ENV_READ.format(name="STANDBY_DATABASE_URL")
        ),
        "satisfied_by": {"STANDBY_DATABASE_URL"},
    },
    # The second standby, which exists to be promoted and destroyed. A leg binding
    # the read-only standby does not satisfy a test that needs one to `pg_promote()`.
    "pg_failover_standby": {
        "detect": re.compile(
            r"failover_standby_database_url\(\)|"
            + ENV_READ.format(name="FAILOVER_STANDBY_DATABASE_URL")
        ),
        "satisfied_by": {"FAILOVER_STANDBY_DATABASE_URL"},
    },
    "redis": {
        "detect": re.compile(
            r"(?:test_support|services)::redis\(|" + ENV_READ.format(name="REDIS_URL")
        ),
        "satisfied_by": {"REDIS_URL"},
    },
    "nats": {
        "detect": re.compile(
            r"(?:test_support|services)::nats\(|" + ENV_READ.format(name="NATS_URL")
        ),
        "satisfied_by": {"NATS_URL"},
    },
    "minio": {
        "detect": re.compile(
            r"(?:test_support|services)::minio\(|" + ENV_READ.format(name="MINIO_ENDPOINT")
        ),
        "satisfied_by": {"MINIO_ENDPOINT"},
    },
    "azure_blob": {
        "detect": re.compile(
            r"(?:test_support|services)::azure_blob\(|"
            + ENV_READ.format(name="AZURE_BLOB_ENDPOINT")
        ),
        "satisfied_by": {"AZURE_BLOB_ENDPOINT"},
    },
    "gcs": {
        "detect": re.compile(
            r"(?:test_support|services)::gcs\(|" + ENV_READ.format(name="GCS_ENDPOINT")
        ),
        "satisfied_by": {"GCS_ENDPOINT"},
    },
    "vault": {
        "detect": re.compile(
            r"(?:test_support|services)::vault\(|" + ENV_READ.format(name="VAULT_ADDR")
        ),
        "satisfied_by": {"VAULT_ADDR"},
    },
    # #975 outbound CDC sinks. Added *before* the suites that need them: without a
    # group here a suite reading KAFKA_BOOTSTRAP satisfies this gate merely by
    # being named in some leg — including one that binds no broker, which is
    # precisely the #960 shape the gate exists to catch.
    "kafka": {
        "detect": re.compile(
            r"(?:test_support|services)::kafka\(|" + ENV_READ.format(name="KAFKA_BOOTSTRAP")
        ),
        "satisfied_by": {"KAFKA_BOOTSTRAP"},
    },
    "kinesis": {
        "detect": re.compile(
            r"(?:test_support|services)::kinesis\(|" + ENV_READ.format(name="KINESIS_ENDPOINT")
        ),
        "satisfied_by": {"KINESIS_ENDPOINT"},
    },
}

ALL_FEATURES = object()  # sentinel: --all-features

# crate -> {feature: [features it implies]}, filled by discover(). Needed because a leg
# naming `observers-nats` also enables `observers`, so a suite gated on `observers` IS
# covered by it — and a subset test over the literal `--features` list would report a
# false ORPHAN (#1082).
FEATURE_GRAPH: dict[str, dict[str, list[str]]] = {}

# A file-level inner attribute gating the WHOLE test binary, e.g.
#   #![cfg(feature = "functions-runtime")]
#   #![cfg(all(feature = "auth", feature = "rest"))]
#   #![cfg(all(feature = "arrow", not(feature = "wire-backend")))]
# Anchored to the start of a line so an attribute inside a string or a nested item
# cannot match. `#![cfg(test)]` and other non-feature predicates yield nothing.
FILE_CFG = re.compile(r"^#!\[cfg\((.*)\)\]\s*$", re.M)
NOT_FEATURE = re.compile(r'not\s*\(\s*feature\s*=\s*"([^"]+)"\s*\)')
ANY_PREDICATE = re.compile(r"\bany\s*\(")


def parse_file_level_cfg(src: str) -> tuple[set[str], set[str]]:
    """(features that must be ON, features that must be OFF) for the whole file.

    `discover_binaries` learned a binary's feature requirement from
    `[[test]] required-features` and from nowhere else. 67 of the 68 test binaries
    that gate themselves with a file-level `#![cfg(feature = "…")]` declare no such
    key, so the gate treated them as feature-free: any leg naming one satisfied
    coverage whether or not it enabled the feature, and an inner `cfg` under a leg
    that omits it compiles to an empty binary reporting `test result: ok. 0 passed`
    (#1082).

    `any(...)` is deliberately not modelled: "one of these features" cannot be
    expressed as a required set, and guessing would either over- or under-credit.
    No file in the tree uses it; if one appears, it is reported rather than
    silently mis-read.
    """
    required: set[str] = set()
    forbidden: set[str] = set()
    for m in FILE_CFG.finditer(src):
        pred = m.group(1)
        if ANY_PREDICATE.search(pred):
            die(
                "a file-level #![cfg(any(...))] is not modelled by this gate; it would "
                f"be silently mis-credited. Predicate: cfg({pred})"
            )
        for feat in NOT_FEATURE.findall(pred):
            forbidden.add(feat)
        negated = NOT_FEATURE.sub("", pred)
        required.update(re.findall(r'feature\s*=\s*"([^"]+)"', negated))
    return required, forbidden


def expand_features(crate: str, feats: set[str], with_defaults: bool) -> set[str]:
    """Close a `--features` list over the crate's own [features] table.

    `observers-nats = ["observers", "dep:async-nats"]` means a leg passing
    `--features observers-nats` has `observers` on too. Entries naming another crate
    (`dep:x`, `other-crate/feature`) are not features of this crate and are dropped.
    """
    table = FEATURE_GRAPH.get(crate, {})
    seen: set[str] = set()
    stack = list(feats)
    if with_defaults and "default" in table:
        stack.append("default")
    while stack:
        f = stack.pop()
        if f in seen or "/" in f or f.startswith("dep:"):
            continue
        seen.add(f)
        stack.extend(table.get(f, []))
    return seen


def die(msg: str) -> None:
    print(f"suite-coverage: FATAL: {msg}", file=sys.stderr)
    sys.exit(2)


# ── Phase A: discover what exists ─────────────────────────────────────────────


class TestBinary:
    def __init__(self, crate: str, name: str, path: Path):
        self.crate = crate
        self.name = name
        self.path = path
        self.required_features: set[str] = set()
        # Features that must be OFF for this binary to hold any tests. A file-level
        # `#![cfg(all(feature = "arrow", not(feature = "wire-backend")))]` compiles to
        # an EMPTY binary under `--all-features`, so a leg enabling the forbidden
        # feature does not cover it however many other boxes it ticks (#1082).
        self.forbidden_features: set[str] = set()
        self.needs: set[str] = set()  # service-group names from SERVICES
        self.n_tests = 0
        self.n_ignored = 0

    @property
    def ignored_only(self) -> bool:
        return self.n_tests > 0 and self.n_ignored >= self.n_tests

    @property
    def target_id(self) -> str:
        return f"{self.crate}::{self.name}"


class LibTestModule:
    def __init__(self, crate: str, module_path: str, features: set[str], path: Path):
        self.crate = crate
        self.module_path = module_path  # e.g. "inbound::email::tracking::tests"
        self.features = features  # gating features (all must be enabled)
        self.path = path

    @property
    def target_id(self) -> str:
        return f"{self.crate}::lib::{self.module_path}"


def parse_cargo_toml(crate_dir: Path) -> dict:
    with open(crate_dir / "Cargo.toml", "rb") as f:
        return tomllib.load(f)


def strip_line_comments(src: str) -> str:
    """Drop `//`, `///` and `//!` comment text, keeping line structure.

    Deliberately naive — it does not understand `//` inside a string literal —
    because it is only used to count attributes, which cannot appear in a string
    that matters here. Block comments are left alone for the same reason.
    """
    return "\n".join(line.split("//", 1)[0] for line in src.splitlines())


def submodule_sources(entry: Path, src: str, _seen: set[Path] | None = None) -> str:
    """Concatenate the sources of every `mod x;` the binary declares, recursively.

    Rust resolves `mod x;` from `tests/<stem>/x.rs` or `tests/<stem>/x/mod.rs` for an
    entry file `tests/<stem>.rs`, and relative to its own directory for a nested one.
    Both layouts are checked. Cycles are impossible in valid Rust but `_seen` guards
    the walk anyway, because this gate must not hang on malformed input.

    Inline `mod x { … }` blocks need no handling: their bodies are already in `src`.
    """
    seen = _seen if _seen is not None else set()
    parent = entry.parent
    stem_dir = parent / entry.stem
    out = []
    for name in MOD_DECL.findall(src):
        for cand in (stem_dir / f"{name}.rs", stem_dir / name / "mod.rs",
                     parent / f"{name}.rs", parent / name / "mod.rs"):
            if not cand.is_file() or cand in seen or cand == entry:
                continue
            seen.add(cand)
            sub = cand.read_text(encoding="utf-8", errors="replace")
            out.append(sub)
            out.append(submodule_sources(cand, sub, seen))
            break
    return "\n".join(out)


def discover_binaries(crate_dir: Path, crate: str, manifest: dict) -> list[TestBinary]:
    tests_dir = crate_dir / "tests"
    if not tests_dir.is_dir():
        return []
    declared = {
        t["name"]: set(t.get("required-features", []))
        for t in manifest.get("test", [])
        if "name" in t
    }
    binaries = []
    for f in sorted(tests_dir.glob("*.rs")):
        b = TestBinary(crate, f.stem, f)
        b.required_features = set(declared.get(f.stem, set()))
        src = f.read_text(encoding="utf-8", errors="replace")
        # A file-level `#![cfg(feature = "…")]` gates the binary exactly as
        # `required-features` does, but cargo cannot see it: it builds the target and
        # produces an empty one. Fold it in so `covers_binary` demands a leg that
        # actually enables the feature (#1082).
        cfg_required, cfg_forbidden = parse_file_level_cfg(src)
        b.required_features |= cfg_required
        b.forbidden_features |= cfg_forbidden
        # Count the binary's own tests BEFORE appending shared-harness sources:
        # a helper-only file with zero tests is not a suite.
        #
        # Counted over the code, not the comments: a module doc that explains the
        # suite self-skips "(no `#[ignore]`)" is prose, and counting it made a
        # single-test suite look entirely #[ignore]d — a false ORPHAN. Attributes
        # never legitimately live in a comment, so stripping them is lossless here.
        # Service detection below deliberately still scans the raw source: over-
        # detecting a service need is the fail-closed direction.
        code = strip_line_comments(src)
        own_tests = len(re.findall(r"#\[(?:tokio::)?test[\]\(]", code))
        # A binary's tests may live in submodules rather than in the entry file.
        # `crates/fraiseql-server/tests/security.rs` is fourteen lines of
        # `mod security { mod auth_bypass_detection_test; … }` and holds zero
        # `#[test]` attributes of its own — while the six files it aggregates hold
        # 100, including the JWT-validation and OIDC-provider suites. Counting only
        # the entry file scored it `n_tests == 0`, the `continue` below dropped the
        # binary from every coverage check, and the gate printed `OK … all covered`
        # over a suite no leg runs (#1029). Submodule sources are folded in first, so
        # `n_tests == 0` means what it says: nothing to execute.
        code += "\n" + strip_line_comments(submodule_sources(f, src))
        b.n_tests = len(re.findall(r"#\[(?:tokio::)?test[\]\(]", code))
        b.n_ignored = len(re.findall(r"#\[ignore\b", code))
        # Shared harness modules can hold the service getter for the binary.
        if re.search(r"^\s*(pub\s+)?mod common\b", src, re.M):
            for cf in sorted((tests_dir / "common").glob("**/*.rs")):
                src += cf.read_text(encoding="utf-8", errors="replace")
        # …and so can an aggregator's submodules. Service detection read only the
        # entry file, so an aggregator whose DB-touching tests live in submodules
        # scored `[plain]` and could be wired into a database-free leg, where the
        # `try_database_url()` guard turns each of them into a silent skip that reads
        # exactly like a pass (#1029).
        #
        # Folded in only when the entry file declares no tests of its own. A binary
        # with its own tests shows its service usage in its own file (plus `common/`),
        # whereas an aggregator has nowhere else to show it. Folding unconditionally
        # over-detects: `http_server_e2e_test` and `concurrent_load_test` declare
        # `mod test_helpers;`, whose `TestServerConfig::new()` reads `DATABASE_URL`
        # as an unused default — those suites drive a bound server over
        # `FRAISEQL_TEST_URL` and touch no database from the test container.
        if own_tests == 0:
            src += "\n" + submodule_sources(f, src)
        for group, spec in SERVICES.items():
            if spec["detect"].search(src):
                b.needs.add(group)
        if b.n_tests == 0:
            continue  # compiled-only helper file, nothing to execute
        binaries.append(b)
    return binaries


MOD_DECL = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z0-9_]+)\s*;", re.M)


def gating_features_of_mod(parent_src: str, mod_name: str) -> set[str] | None:
    """Features gating `mod <name>;` in parent_src, or None if not declared."""
    for m in re.finditer(rf"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+{mod_name}\s*;", parent_src, re.M):
        # Walk backwards over the contiguous attribute/comment block above.
        head = parent_src[: m.start()]
        lines = head.splitlines()
        feats: set[str] = set()
        i = len(lines) - 1
        while i >= 0:
            line = lines[i].strip()
            if line.startswith("#[") or line.startswith("#!["):
                feats.update(re.findall(r'feature\s*=\s*"([^"]+)"', line))
                i -= 1
            elif line.startswith("//") or line == "":
                i -= 1
            else:
                break
        return feats
    return None


def module_file_for(crate_src: Path, components: list[str]) -> Path | None:
    """The file declaring the module `components` (a.rs or a/mod.rs or lib.rs)."""
    if not components:
        for c in ("lib.rs", "main.rs"):
            if (crate_src / c).exists():
                return crate_src / c
        return None
    as_file = crate_src / (Path(*components[:-1]) / f"{components[-1]}.rs")
    as_dir = crate_src / Path(*components) / "mod.rs"
    if as_file.exists():
        return as_file
    if as_dir.exists():
        return as_dir
    return None


def discover_lib_modules(crate_dir: Path, crate: str) -> list[LibTestModule]:
    """Feature-gated `tests.rs` unit-test modules under src/ (the #981 class).

    For src/a/b/tests.rs the module path is a::b::tests; the gating features are
    the union of `feature = "…"` cfgs on every `mod` declaration along the chain.
    Ungated modules always run with the lib and are not tracked individually.
    """
    src_root = crate_dir / "src"
    if not src_root.is_dir():
        return []
    out = []
    for tf in sorted(src_root.rglob("tests.rs")):
        rel = tf.relative_to(src_root)
        components = list(rel.parts[:-1]) + ["tests"]
        feats: set[str] = set()
        declared_everywhere = True
        for i, comp in enumerate(components):
            parent = module_file_for(src_root, components[:i])
            if parent is None:
                declared_everywhere = False
                break
            g = gating_features_of_mod(parent.read_text(encoding="utf-8", errors="replace"), comp)
            if g is None:
                declared_everywhere = False
                break
            feats.update(g)
        if declared_everywhere and feats:
            out.append(LibTestModule(crate, "::".join(components), feats, tf))
    return out


# ── Phase B: discover what runs ───────────────────────────────────────────────


class Invocation:
    def __init__(self, leg: str, cmd: str):
        self.leg = leg
        self.cmd = cmd  # fully resolved command line
        self.crate: str | None = None
        self.workspace = False
        self.excludes: set[str] = set()
        self.lib_only = False
        self.doc_only = False
        self.tests: list[str] = []  # --test names ("*" = every binary)
        self.features: set[str] | object = set()
        self.no_default_features = False
        self.filters: list[str] = []  # positive test-name filters
        self.skips: list[str] = []  # --skip tokens
        self.ignored_mode = False  # `-- --ignored`: runs ONLY #[ignore]d tests
        self.include_ignored = False  # `-- --include-ignored`: runs both classes
        self.env: set[str] = set()  # leg-bound env vars (filled later)
        # GitHub Actions only: the `paths:` filter of the workflow's push/PR
        # triggers. A path-filtered workflow covers a suite only if editing that
        # suite triggers it — otherwise the suite can be broken and the workflow
        # that "runs" it never fires. `None` = no filter, i.e. every path.
        self.trigger_paths: list[str] | None = None

    def __repr__(self):
        return f"<{self.leg}: {self.cmd[:90]}>"


GO_FUNC = re.compile(r"^func \(m \*FraiseqlCi\) (\w+)\(", re.M)
GO_CONST = re.compile(r"^\t(\w+)\s*=\s*\"([^\"]*)\"", re.M)
GO_LOCAL_STR = re.compile(r"^\t+(\w+)\s*:=\s*(\"(?:[^\"\\]|\\.)*\"(?:\s*\+\s*[^\n]+)*)", re.M)
GO_SLICE = re.compile(r"(\w+)\s*:?=\s*\[\]string\{([^}]*)\}", re.S)
GO_RANGE = re.compile(r"for\s+_,\s*(\w+)\s*:=\s*range\s+(\w+)")


class Unresolvable(Exception):
    pass


def strip_go_comments(expr: str) -> str:
    """Remove `//` line comments, respecting string literals (postgres:// URLs)."""
    out = []
    in_str = False
    i = 0
    while i < len(expr):
        c = expr[i]
        if in_str:
            out.append(c)
            if c == "\\" and i + 1 < len(expr):
                out.append(expr[i + 1])
                i += 1
            elif c == '"':
                in_str = False
        elif c == '"':
            in_str = True
            out.append(c)
        elif c == "/" and expr[i : i + 2] == "//":
            while i < len(expr) and expr[i] != "\n":
                i += 1
            continue
        else:
            out.append(c)
        i += 1
    return "".join(out)


def resolve_go_expr(expr: str, names: dict[str, str], loops: dict[str, list[str]]) -> list[str]:
    """Concatenate a Go `+`-joined string expression; expand loop variables.

    Raises [`Unresolvable`] on an identifier that is neither a known string
    name nor a loop variable. For cargo-test expressions the caller turns that
    into a FATAL error — a silently dropped fragment would make the gate lie
    about what a leg runs.
    """
    expr = strip_go_comments(expr)
    variants: list[list[str]] = [[]]
    for part in re.finditer(r'"((?:[^"\\]|\\.)*)"|([A-Za-z_]\w*)', expr):
        lit, ident = part.group(1), part.group(2)
        if lit is not None:
            for v in variants:
                v.append(lit.replace('\\"', '"'))
        elif ident in names:
            for v in variants:
                v.append(names[ident])
        elif ident in loops:
            variants = [v + [elem] for v in variants for elem in loops[ident]]
        else:
            raise Unresolvable(f"unresolvable identifier {ident!r} in: {expr}")
    return ["".join(v) for v in variants]


def extract_invocations(main_go: str) -> list[Invocation]:
    consts = dict(GO_CONST.findall(main_go))
    funcs = [(m.start(), m.group(1)) for m in GO_FUNC.finditer(main_go)]

    def func_at(pos: int) -> str:
        name = "<toplevel>"
        for start, fname in funcs:
            if start <= pos:
                name = fname
            else:
                break
        return name

    # Local string vars (e.g. `skip := "-- --skip …" + const`), resolved on top
    # of the consts; then loop-expanded []string slices (the wire-leg shape).
    names = dict(consts)
    for m in GO_LOCAL_STR.finditer(main_go):
        try:
            resolved = resolve_go_expr(m.group(2), names, {})
        except Unresolvable:
            continue  # a non-string or computed local — irrelevant unless a cargo expr uses it
        if len(resolved) == 1:
            names[m.group(1)] = resolved[0]
    slices = {
        m.group(1): [e.strip().strip('"') for e in m.group(2).replace("\n", " ").split(",") if e.strip()]
        for m in GO_SLICE.finditer(main_go)
    }
    loops = {}
    for m in GO_RANGE.finditer(main_go):
        if m.group(2) in slices:
            loops[m.group(1)] = slices[m.group(2)]

    invocations = []
    # A command expression starts at a literal containing `cargo test`/`cargo
    # nextest` and spans the whole Go `+`-concatenation (which may cross lines).
    for m in re.finditer(r'"cargo (?:test|nextest)[^"]*"', main_go):
        start = m.start()
        end = m.end()
        # Extend forward across `+`-concatenation; Go comments may sit between
        # the operator and the next term.
        rest = main_go[end:]
        offset = 0
        while True:
            cont = re.match(
                r'\s*\+(?:\s|//[^\n]*)*(?:"(?:[^"\\]|\\.)*"|[A-Za-z_]\w*)', rest[offset:]
            )
            if not cont:
                break
            offset += cont.end()
        expr = main_go[start : end + offset]
        try:
            cmds = resolve_go_expr(expr, names, loops)
        except Unresolvable as e:
            die(f"{e} — teach tools/check-suite-coverage.py about this .dagger/main.go shape")
        for cmd in cmds:
            cmd = cmd.strip()
            if cmd.startswith("cargo "):
                invocations.append(Invocation(func_at(start), cmd))

    # Per-function env bindings.
    env_by_func: dict[str, set[str]] = {}
    for m in re.finditer(r'WithEnvVariable\("([A-Z0-9_]+)"', main_go):
        env_by_func.setdefault(func_at(m.start()), set()).add(m.group(1))
    for inv in invocations:
        inv.env = env_by_func.get(inv.leg, set())

    for inv in invocations:
        parse_cmd(inv)
    return invocations


def parse_cmd(inv: Invocation) -> None:
    # Split runner args from test-harness args (after a bare `--`).
    toks = inv.cmd.split()
    harness: list[str] = []
    if "--" in toks:
        i = toks.index("--")
        harness = toks[i + 1 :]
        toks = toks[:i]
    i = 0
    while i < len(toks):
        t = toks[i]
        if t in ("cargo", "test", "nextest", "run") and i <= 2:
            pass
        elif t in ("-p", "--package"):
            i += 1
            inv.crate = toks[i].strip("'\"")
        elif t == "--workspace":
            inv.workspace = True
        elif t == "--exclude":
            i += 1
            inv.excludes.add(toks[i])
        elif t == "--lib":
            inv.lib_only = True
        elif t == "--doc":
            inv.doc_only = True
        elif t == "--test":
            i += 1
            inv.tests.append(toks[i].strip("'\""))
        elif t == "--all-features":
            inv.features = ALL_FEATURES
        elif t == "--no-default-features":
            inv.no_default_features = True
        elif t == "--features":
            i += 1
            feats = toks[i].strip("'\"")
            if inv.features is not ALL_FEATURES:
                assert isinstance(inv.features, set)
                inv.features.update(f.strip() for f in feats.split(",") if f.strip())
        elif not t.startswith("-"):
            # Positional TESTNAME filter (cargo accepts it before `--`).
            inv.filters.append(t.strip("'\""))
        i += 1
    # Harness args: --skip X pairs; bare tokens are positive filters.
    j = 0
    while j < len(harness):
        if harness[j] == "--skip":
            j += 1
            inv.skips.append(harness[j])
        elif harness[j] == "--ignored":
            inv.ignored_mode = True
        elif harness[j] == "--include-ignored":
            inv.include_ignored = True
        elif harness[j] in ("--test-threads",):
            j += 1
        elif harness[j].startswith("--"):
            pass
        else:
            inv.filters.append(harness[j])
        j += 1


# ── Phase B2: what runs in GitHub Actions ─────────────────────────────────────
#
# Four codegen consumer suites shell out to language toolchains and the network,
# so they cannot live in the offline Dagger legs and execute in a GitHub-hosted
# job instead (#1120). Carrying them as exemptions meant the gate took on faith
# the very thing it exists to check: an exemption is a claim, and deleting the
# `--test` flag from the workflow would have left all four compiling, #[ignore]d,
# exempt, and running nowhere.
#
# This side of the gate is held to a stricter standard than the Dagger side,
# because a workflow can *look* like coverage while providing none. Three ways,
# all present in this tree today:
#
#   1. `feature-flags.yml` and `bench.yml` are `workflow_dispatch:`-only — their
#      push/PR triggers were stripped in the Dagger migration and kept "as
#      porting spec". Their `cargo test` lines run on no push and no PR. Reading
#      them as coverage would invent it outright.
#   2. `rust-sdk.yml` (job-level `defaults.run.working-directory`) and
#      `rust-sdk-client.yml` (per-step `working-directory`) run under
#      `sdks/official/…`, which declares its own `[workspace]`. `cargo test
#      --all-features` there covers that workspace, not this one.
#   3. `bench.yml` runs `--bench`, which selects a benchmark target, not a test
#      binary.
#
# So every invocation is resolved in full — including `${{ matrix.* }}`, which
# must expand or raise — and only then filtered, with the reason recorded. The
# order matters: resolving first means an unparseable new shape is FATAL even in
# a workflow that would not have counted anyway, which is what keeps this from
# rotting the moment someone edits feature-flags.yml.

WORKFLOWS = REPO / ".github" / "workflows"

GHA_EXPR = re.compile(r"\$\{\{\s*(.*?)\s*\}\}")


class YamlError(Exception):
    pass


def _yaml_scalar(text: str) -> str | bool | None:
    """A plain/quoted YAML scalar. No int/float coercion — nothing here needs it."""
    text = text.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in "\"'":
        body = text[1:-1]
        return body.replace("''", "'") if text[0] == "'" else body
    if text in ("true", "True", "yes", "on"):
        return True
    if text in ("false", "False", "no", "off"):
        return False
    if text in ("null", "~", ""):
        return None
    return text


def _yaml_key(text: str) -> str:
    """A mapping key: unquoted, but never value-coerced.

    `on:` is the whole reason this is separate from [`_yaml_scalar`]. Under YAML
    1.1 the plain scalar `on` is the boolean true, so a value-coercing key reader
    turns every workflow's trigger block into a key named `True` and the gate
    then reports that no workflow has an `on:` block — loudly, but about the
    wrong thing.
    """
    text = text.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in "\"'":
        body = text[1:-1]
        return body.replace("''", "'") if text[0] == "'" else body
    return text


def _strip_comment(line: str) -> str:
    """Drop a trailing `#` comment, respecting quotes. `#` mid-token is not one."""
    out, quote = [], None
    for i, c in enumerate(line):
        if quote:
            out.append(c)
            if c == quote:
                quote = None
        elif c in "\"'":
            quote = c
            out.append(c)
        elif c == "#" and (i == 0 or line[i - 1] in " \t"):
            break
        else:
            out.append(c)
    return "".join(out).rstrip()


def _parse_flow(text: str):
    """`[a, b]` / `{a: b}`, one level of nesting. Raises on anything else."""
    text = text.strip()
    if text.startswith("[") and text.endswith("]"):
        inner = text[1:-1].strip()
        return [_yaml_scalar(p) for p in _split_flow(inner)] if inner else []
    if text.startswith("{") and text.endswith("}"):
        inner = text[1:-1].strip()
        out = {}
        for part in _split_flow(inner):
            if ":" not in part:
                raise YamlError(f"flow mapping entry without ':': {part!r}")
            k, _, v = part.partition(":")
            out[_yaml_key(k)] = _yaml_scalar(v)
        return out
    raise YamlError(f"not a flow collection: {text!r}")


def _split_flow(inner: str) -> list[str]:
    parts, buf, quote, depth = [], [], None, 0
    for c in inner:
        if quote:
            buf.append(c)
            if c == quote:
                quote = None
        elif c in "\"'":
            quote = c
            buf.append(c)
        elif c in "[{":
            depth += 1
            buf.append(c)
        elif c in "]}":
            depth -= 1
            buf.append(c)
        elif c == "," and depth == 0:
            parts.append("".join(buf).strip())
            buf = []
        else:
            buf.append(c)
    if "".join(buf).strip():
        parts.append("".join(buf).strip())
    return parts


class _Lines:
    """Indentation-aware cursor over the significant lines of a YAML document."""

    def __init__(self, text: str):
        self.raw = text.expandtabs(8).split("\n")
        self.i = 0

    def peek(self) -> tuple[int, str] | None:
        while self.i < len(self.raw):
            line = self.raw[self.i]
            stripped = _strip_comment(line)
            if not stripped.strip():
                self.i += 1
                continue
            if stripped.strip() == "---":
                self.i += 1
                continue
            if stripped.lstrip().startswith("%"):
                raise YamlError("YAML directives are not supported")
            return len(stripped) - len(stripped.lstrip(" ")), stripped.strip()
        return None

    def next(self) -> tuple[int, str]:
        got = self.peek()
        if got is None:
            raise YamlError("unexpected end of document")
        self.i += 1
        return got

    def block_scalar(self, indent: int, style: str) -> str:
        """Read a `|` / `>` block, keeping raw (un-comment-stripped) text."""
        body: list[str] = []
        block_indent = None
        while self.i < len(self.raw):
            line = self.raw[self.i].expandtabs(8)
            if not line.strip():
                body.append("")
                self.i += 1
                continue
            cur = len(line) - len(line.lstrip(" "))
            if cur <= indent:
                break
            if block_indent is None:
                block_indent = cur
            body.append(line[block_indent:] if len(line) >= block_indent else line.lstrip(" "))
            self.i += 1
        while body and not body[-1].strip():
            body.pop()
        if style.startswith(">"):
            # Folded: a run of non-empty lines joins with spaces; a blank line is a
            # newline. Indented continuation lines are literal in real YAML; nothing
            # in these workflows relies on that, and treating them as folded would
            # silently reshape a command, so refuse instead.
            folded, run = [], []
            for line in body:
                if not line.strip():
                    if run:
                        folded.append(" ".join(run))
                        run = []
                    folded.append("")
                elif line.startswith(" "):
                    raise YamlError("indented line inside a folded (>) scalar")
                else:
                    run.append(line.strip())
            if run:
                folded.append(" ".join(run))
            text = "\n".join(folded)
        else:
            text = "\n".join(body)
        if style.endswith("-"):
            return text
        return text + "\n" if text else text


def _parse_block(lines: _Lines, indent: int):
    """Parse the block at `indent`: a mapping, or a sequence of `-` entries."""
    got = lines.peek()
    if got is None or got[0] < indent:
        return None
    if got[1].startswith("- "):
        return _parse_seq(lines, indent)
    if got[1] == "-":
        return _parse_seq(lines, indent)
    return _parse_map(lines, indent)


def _parse_seq(lines: _Lines, indent: int) -> list:
    out = []
    while True:
        got = lines.peek()
        if got is None or got[0] != indent or not (got[1] == "-" or got[1].startswith("- ")):
            return out
        lines.next()
        rest = got[1][1:].lstrip()
        if not rest:
            child = lines.peek()
            out.append(_parse_block(lines, child[0]) if child and child[0] > indent else None)
            continue
        # `- key: value` opens a mapping whose indent is where `key` starts.
        item_indent = indent + (len(got[1]) - len(got[1][1:].lstrip()))
        if _looks_like_mapping(rest):
            lines.raw[lines.i - 1] = " " * item_indent + rest
            lines.i -= 1
            out.append(_parse_map(lines, item_indent))
        else:
            out.append(_scalar_or_flow(rest))


def _looks_like_mapping(text: str) -> bool:
    if text.startswith(("[", "{")):
        return False
    key, sep, _ = _split_key(text)
    return bool(sep) and bool(key)


def _split_key(text: str) -> tuple[str, str, str]:
    """Split `key: value` outside quotes. Returns (key, ':' or '', value)."""
    quote = None
    for i, c in enumerate(text):
        if quote:
            if c == quote:
                quote = None
        elif c in "\"'":
            quote = c
        elif c == ":" and (i + 1 == len(text) or text[i + 1] in " \t"):
            return text[:i].strip(), ":", text[i + 1 :].strip()
    return text, "", ""


def _scalar_or_flow(text: str):
    if text.startswith(("[", "{")):
        return _parse_flow(text)
    return _yaml_scalar(text)


def _parse_map(lines: _Lines, indent: int) -> dict:
    out: dict = {}
    while True:
        got = lines.peek()
        if got is None or got[0] != indent:
            return out
        if got[1].startswith("- "):
            return out
        lines.next()
        key, sep, value = _split_key(got[1])
        if not sep:
            raise YamlError(f"expected `key: value`, got {got[1]!r}")
        key = _yaml_key(key)
        if value.startswith(("|", ">")) and (len(value) == 1 or value[1] in "-+ "):
            out[key] = lines.block_scalar(indent, value.split()[0])
        elif value:
            out[key] = _scalar_or_flow(value)
        else:
            child = lines.peek()
            out[key] = _parse_block(lines, child[0]) if child and child[0] > indent else None
    return out


def parse_yaml(text: str):
    """The GitHub-Actions subset of YAML, stdlib-only, strict.

    Deliberately NOT a general parser: anchors, multi-document streams, complex
    keys and nested flow collections raise rather than resolve to something
    plausible. It also leaves `on:` a string key — the YAML 1.1 boolean coercion
    that turns `on` into `True` is precisely the kind of quiet reshaping this
    gate cannot afford.
    """
    lines = _Lines(text)
    first = lines.peek()
    if first is None:
        return {}
    return _parse_block(lines, first[0])


def gha_path_match(pattern: str, path: str) -> bool:
    """GitHub's push/pull_request `paths:` filter glob.

    `**` crosses directory separators, `*` and `?` do not, and a pattern with no
    wildcard matches that path or anything beneath it.
    """
    rx, i = [], 0
    while i < len(pattern):
        c = pattern[i]
        if pattern.startswith("**", i):
            rx.append(".*")
            i += 2
        elif c == "*":
            rx.append("[^/]*")
            i += 1
        elif c == "?":
            rx.append("[^/]")
            i += 1
        else:
            rx.append(re.escape(c))
            i += 1
    return re.fullmatch("".join(rx), path) is not None


class WorkflowUnresolvable(Exception):
    pass


def _expand_matrix(matrix: dict) -> list[dict]:
    """`strategy.matrix` → the list of combinations, as GitHub computes it."""
    if "exclude" in matrix:
        raise WorkflowUnresolvable("`matrix.exclude` — teach the gate this shape")
    axes = {k: v for k, v in matrix.items() if k not in ("include", "exclude")}
    for name, values in axes.items():
        if not isinstance(values, list):
            raise WorkflowUnresolvable(f"matrix axis {name!r} is not a list: {values!r}")
    # With no axes the base is EMPTY, not `[{}]`: an include-only matrix (the
    # `feature-flags.yml` shape) expands to exactly its include entries. Seeding
    # `[{}]` instead leaves a phantom variable-less combination in the product,
    # and every `${{ matrix.* }}` in the job is then unresolvable against it.
    combos: list[dict] = [{}] if axes else []
    for name, values in axes.items():
        combos = [{**c, name: v} for c in combos for v in values]
    include = matrix.get("include") or []
    if not isinstance(include, list):
        raise WorkflowUnresolvable(f"`matrix.include` is not a list: {include!r}")
    for entry in include:
        if not isinstance(entry, dict):
            raise WorkflowUnresolvable(f"`matrix.include` entry is not a mapping: {entry!r}")
        overlap = {k: v for k, v in entry.items() if k in axes}
        extended = False
        for c in combos:
            if overlap and all(c.get(k) == v for k, v in overlap.items()):
                c.update(entry)
                extended = True
        if not extended:
            combos.append(dict(entry))
    # No `strategy.matrix` at all: one run, no variables.
    return combos or [{}]


def _resolve_expressions(cmd: str, combo: dict, where: str) -> str:
    """Substitute `${{ matrix.x }}`; raise on anything else the gate cannot know."""

    def sub(m: re.Match) -> str:
        expr = m.group(1)
        key = expr[len("matrix.") :] if expr.startswith("matrix.") else None
        if key is not None and key in combo:
            return str(combo[key])
        raise WorkflowUnresolvable(f"{where}: cannot resolve `${{{{ {expr} }}}}`")

    return GHA_EXPR.sub(sub, cmd)


def _cargo_commands(script: str) -> list[tuple[str, str]]:
    """Cargo test/nextest commands in a `run:` script, with the cwd in force.

    Returns `(relative-cwd, command)`. A `cd` earlier in the same script moves the
    cwd for what follows it — `cd /tmp/gen-rs && cargo check` is the shape that
    makes ignoring this unsafe.
    """
    script = re.sub(r"\\\n\s*", " ", script)
    # Drop heredoc bodies: their contents are data, not commands.
    script = re.sub(r"<<'?(\w+)'?\n.*?\n\s*\1\n", "\n", script, flags=re.S)
    out, cwd = [], "."
    for line in script.split("\n"):
        line = line.strip()
        if line.startswith("#"):
            continue
        for seg in re.split(r"&&|\|\||;|(?<!\|)\|(?!\|)", line):
            seg = seg.strip()
            if not seg:
                continue
            m = re.match(r"cd\s+(\S+)$", seg)
            if m:
                target = m.group(1).strip("\"'")
                cwd = target if target.startswith("/") else (f"{cwd}/{target}" if cwd != "." else target)
                continue
            if re.match(r"cargo\s+(?:\+\S+\s+)?(?:test|nextest)\b", seg):
                out.append((cwd, seg))
    return out


def _auto_triggers(doc: dict) -> dict | None:
    """The push/pull_request trigger config, or None when the workflow is manual.

    `on: [push]` and `on: push` normalise to a filterless trigger; a
    `workflow_dispatch:`-only workflow returns None and covers nothing.
    """
    on = doc.get("on")
    if on is None:
        raise WorkflowUnresolvable("workflow has no `on:` block")
    if isinstance(on, str):
        on = {on: None}
    elif isinstance(on, list):
        on = {str(k): None for k in on}
    if not isinstance(on, dict):
        raise WorkflowUnresolvable(f"unreadable `on:` block: {on!r}")
    autos = {k: v for k, v in on.items() if k in ("push", "pull_request")}
    return autos or None


def _trigger_paths(autos: dict) -> list[str] | None:
    """The union of the automatic triggers' `paths:`; None means "every path"."""
    union: list[str] = []
    for name, cfg in autos.items():
        cfg = cfg or {}
        if not isinstance(cfg, dict):
            raise WorkflowUnresolvable(f"unreadable `{name}:` trigger: {cfg!r}")
        if "paths-ignore" in cfg:
            raise WorkflowUnresolvable(f"`{name}.paths-ignore` — teach the gate this shape")
        paths = cfg.get("paths")
        if paths is None:
            return None
        if not isinstance(paths, list):
            raise WorkflowUnresolvable(f"`{name}.paths` is not a list: {paths!r}")
        union.extend(str(p) for p in paths)
    return union


def extract_workflow_invocations() -> tuple[list[Invocation], list[str]]:
    """Cargo test invocations in `.github/workflows/`, and why each was discounted.

    Every invocation is resolved before any of it is discarded, so a shape the
    parser cannot read is FATAL even in a workflow whose result would have been
    thrown away. Returns the counting invocations and a human-readable ledger of
    the rest.
    """
    counted: list[Invocation] = []
    ledger: list[str] = []
    if not WORKFLOWS.is_dir():
        return counted, ledger

    for wf in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        text = wf.read_text(encoding="utf-8")
        if not re.search(r"cargo\s+(?:\+\S+\s+)?(?:test|nextest)\b", text):
            continue
        try:
            doc = parse_yaml(text)
        except YamlError as e:
            die(f"{wf.name}: {e} — teach tools/check-suite-coverage.py this YAML shape")
        if not isinstance(doc, dict) or not isinstance(doc.get("jobs"), dict):
            die(f"{wf.name}: no `jobs:` mapping — teach the gate this workflow shape")

        try:
            autos = _auto_triggers(doc)
            trigger_paths = _trigger_paths(autos) if autos else None
        except WorkflowUnresolvable as e:
            die(f"{wf.name}: {e}")

        for job_id, job in doc["jobs"].items():
            if not isinstance(job, dict):
                die(f"{wf.name}:{job_id}: unreadable job")
            leg = f"{wf.name}:{job_id}"
            defaults = ((job.get("defaults") or {}).get("run") or {}).get("working-directory")
            steps = job.get("steps") or []
            if not isinstance(steps, list):
                die(f"{leg}: `steps:` is not a list")
            try:
                combos = _expand_matrix((job.get("strategy") or {}).get("matrix") or {})
            except WorkflowUnresolvable as e:
                die(f"{leg}: {e}")

            for step in steps:
                if not isinstance(step, dict) or not step.get("run"):
                    continue
                script = step["run"]
                if not isinstance(script, str):
                    die(f"{leg}: `run:` is not a string")
                step_dir = step.get("working-directory") or defaults or "."
                for combo in combos:
                    for cwd, raw in _cargo_commands(script):
                        try:
                            cmd = _resolve_expressions(raw, combo, leg)
                            base = _resolve_expressions(str(step_dir), combo, leg)
                        except WorkflowUnresolvable as e:
                            die(str(e))
                        # The step's working-directory and any `cd` inside the
                        # script compose; either alone is enough to leave the
                        # workspace this gate reasons about.
                        effective = cwd if cwd.startswith("/") else _join(base, cwd)
                        if effective not in (".", ""):
                            ledger.append(
                                f"{leg}: not counted — runs in `{effective}`, "
                                f"outside this workspace: {cmd[:70]}"
                            )
                            continue
                        if re.search(r"(?:^|\s)--bench(?:\s|=|$)", cmd):
                            ledger.append(
                                f"{leg}: not counted — `--bench` selects a benchmark "
                                f"target, not a test binary: {cmd[:70]}"
                            )
                            continue
                        if autos is None:
                            ledger.append(
                                f"{leg}: not counted — workflow_dispatch-only, so this "
                                f"runs on no push and no PR: {cmd[:70]}"
                            )
                            continue
                        inv = Invocation(leg, cmd)
                        inv.trigger_paths = trigger_paths
                        parse_cmd(inv)
                        counted.append(inv)
    return counted, ledger


def _join(base: str, rel: str) -> str:
    if rel in (".", ""):
        return base if base else "."
    if base in (".", ""):
        return rel
    return f"{base}/{rel}"


# ── Phase C: coverage ─────────────────────────────────────────────────────────


def features_enabled(
    inv: Invocation, crate: str, wanted: set[str], forbidden: set[str] = frozenset()
) -> bool:
    if inv.features is ALL_FEATURES:
        # `--all-features` enables everything, so a `not(feature = …)` arm compiles to
        # nothing. The suite builds and runs zero tests — covered by nobody (#1082).
        return not forbidden
    assert isinstance(inv.features, set)
    enabled = expand_features(crate, inv.features, with_defaults=not inv.no_default_features)
    return wanted <= enabled and not (forbidden & enabled)


def triggers_path(inv: Invocation, path: Path) -> bool:
    """Would editing `path` start the workflow this invocation runs in?

    A Dagger leg has no path filter and always answers yes. A path-filtered
    workflow that does not list the suite's own crate is not coverage: the suite
    could be broken in the same commit that fails to run it.
    """
    if inv.trigger_paths is None:
        return True
    rel = path.resolve().relative_to(REPO).as_posix()
    return any(gha_path_match(p, rel) for p in inv.trigger_paths)


def covers_binary(inv: Invocation, b: TestBinary) -> bool:
    if inv.lib_only or inv.doc_only:
        return False
    if not triggers_path(inv, b.path):
        return False
    if inv.crate is not None and inv.crate != b.crate:
        return False
    if inv.workspace and b.crate in inv.excludes:
        return False
    if inv.crate is None and not inv.workspace:
        return False
    if inv.tests and "*" not in inv.tests and b.name not in inv.tests:
        return False
    # A positional TESTNAME filter is passed to every selected binary, and cargo cannot
    # tell the gate which names it will match. `covers_module` has always applied
    # `filters`; `covers_binary` did not, so `cargo test -p fraiseql-storage --features
    # aws-s3 backend::s3` was read as running EVERY binary in the crate, when in fact
    # each one prints "running 0 tests" (#1056, #1093). The safe reading of a filtered
    # run is that it covers only what it names with `--test`, and nothing by crate.
    #
    # `inv.skips` deliberately does NOT disqualify: `--skip` removes matching tests and
    # leaves the rest running, so a binary stays covered unless every one of its tests
    # matches — which the gate cannot know, and which is false for every `--skip` in
    # the tree today (they name lib-module paths like `migrations::tests`, which cannot
    # match an integration binary's free functions).
    if not inv.tests and inv.filters:
        return False
    if not features_enabled(inv, b.crate, b.required_features, b.forbidden_features):
        return False
    # `-- --ignored` runs ONLY #[ignore]d tests; a plain run skips them;
    # `--include-ignored` runs both.
    if not inv.include_ignored:
        if inv.ignored_mode and b.n_ignored == 0:
            return False
        if not inv.ignored_mode and b.ignored_only:
            return False
    # A suite needing services counts as covered only by a leg binding them all.
    if not all(SERVICES[g]["satisfied_by"] & inv.env for g in b.needs):
        return False
    return True


def covers_module(inv: Invocation, mod: LibTestModule) -> bool:
    if inv.doc_only or inv.tests:
        return False
    if not triggers_path(inv, mod.path):
        return False
    if inv.crate is not None and inv.crate != mod.crate:
        return False
    if inv.workspace and mod.crate in inv.excludes:
        return False
    if inv.crate is None and not inv.workspace:
        return False
    if not features_enabled(inv, mod.crate, mod.features):
        return False
    if inv.filters and not any(mod.module_path.startswith(f.rstrip(":")) for f in inv.filters):
        return False
    if any(mod.module_path.startswith(s.rstrip(":")) for s in inv.skips):
        return False
    return True


def load_exemptions() -> dict[str, str]:
    if not EXEMPTIONS.exists():
        return {}
    with open(EXEMPTIONS, "rb") as f:
        data = tomllib.load(f)
    out = {}
    for row in data.get("exempt", []):
        if "target" not in row or "reason" not in row or not row["reason"].strip():
            die(f"exemption row without target/reason: {row}")
        out[row["target"]] = row["reason"]
    return out


def main() -> int:
    main_go = DAGGER_MAIN.read_text(encoding="utf-8")
    dagger_invocations = extract_invocations(main_go)
    workflow_invocations, workflow_ledger = extract_workflow_invocations()
    invocations = dagger_invocations + workflow_invocations
    exemptions = load_exemptions()

    crates = sorted(p for p in (REPO / "crates").iterdir() if (p / "Cargo.toml").exists())
    binaries: list[TestBinary] = []
    modules: list[LibTestModule] = []
    crate_names = set()
    # Two passes: the feature graph of EVERY crate must exist before any coverage
    # question is asked, because a leg naming one feature enables the ones it implies.
    for cd in crates:
        manifest = parse_cargo_toml(cd)
        FEATURE_GRAPH[manifest["package"]["name"]] = {
            name: list(implies) if isinstance(implies, list) else []
            for name, implies in manifest.get("features", {}).items()
        }
    for cd in crates:
        manifest = parse_cargo_toml(cd)
        crate = manifest["package"]["name"]
        crate_names.add(crate)
        binaries.extend(discover_binaries(cd, crate, manifest))
        modules.extend(discover_lib_modules(cd, crate))

    failures: list[str] = []
    used_exemptions: set[str] = set()

    # Direction A+B: every binary covered by some invocation.
    for b in binaries:
        legs = [inv.leg for inv in invocations if covers_binary(inv, b)]
        if legs:
            continue
        if b.target_id in exemptions:
            used_exemptions.add(b.target_id)
            continue
        if b.ignored_only and f"{b.target_id}" not in exemptions:
            failures.append(
                f"ORPHAN (all-#[ignore]) {b.target_id}: every test is #[ignore]d — "
                f"exempt it with a reason or wire an --ignored run"
            )
            continue
        why = []
        if b.required_features:
            why.append(f"required-features={sorted(b.required_features)}")
        if b.needs:
            why.append(f"needs={sorted(b.needs)}")
        near = [
            inv.leg
            for inv in invocations
            if covers_binary_ignoring_env(inv, b) and not covers_binary(inv, b)
        ]
        hint = f" (runs service-less in: {', '.join(sorted(set(near)))} — self-skip reads green)" if near else ""
        failures.append(f"ORPHAN {b.target_id}: no leg runs it [{'; '.join(why) or 'plain'}]{hint}")

    # Feature-gated lib test modules.
    for mod in modules:
        legs = [inv.leg for inv in invocations if covers_module(inv, mod)]
        if legs:
            continue
        if mod.target_id in exemptions:
            used_exemptions.add(mod.target_id)
            continue
        failures.append(
            f"ORPHAN {mod.target_id}: feature-gated (features={sorted(mod.features)}) and "
            f"no lib invocation enables it un-filtered — compiled out reads as passing"
        )

    # Direction C: every --test flag names an existing binary.
    known = {(b.crate, b.name) for b in binaries}
    for inv in invocations:
        for t in inv.tests:
            if t == "*":
                continue  # glob: every binary of the crate
            if inv.crate and (inv.crate, t) not in known:
                failures.append(
                    f"GHOST {inv.leg}: `--test {t}` in a {inv.crate} invocation, but "
                    f"crates/{inv.crate}/tests/{t}.rs does not exist"
                )
        if inv.crate and inv.crate not in crate_names and inv.crate != "fraiseql-ci":
            failures.append(f"GHOST {inv.leg}: `-p {inv.crate}` names a non-workspace crate")

    # Stale exemptions rot the ledger: fail when a target no longer exists.
    all_ids = {b.target_id for b in binaries} | {m.target_id for m in modules}
    for target in exemptions:
        if target not in all_ids:
            failures.append(f"STALE EXEMPTION {target}: no such target exists any more")

    if failures:
        print(f"suite-coverage: FAIL — {len(failures)} finding(s):\n")
        for f in sorted(failures):
            print(f"  ✗ {f}")
        print(
            f"\n  {len(binaries)} binaries / {len(modules)} feature-gated lib modules "
            f"checked against {len(invocations)} invocations "
            f"({len(dagger_invocations)} Dagger, {len(workflow_invocations)} workflow)."
        )
        if workflow_ledger:
            print("\n  Workflow invocations resolved but NOT counted as coverage:")
            for line in sorted(set(workflow_ledger)):
                print(f"    · {line}")
        print("  Wire the suite into a leg in .dagger/main.go, or add an exemption with a")
        print("  reason to tools/suite-coverage-exemptions.toml.")
        return 1

    print(
        f"suite-coverage: OK — {len(binaries)} binaries and {len(modules)} feature-gated "
        f"lib modules all covered ({len(dagger_invocations)} Dagger + "
        f"{len(workflow_invocations)} workflow invocations, "
        f"{len(used_exemptions)} exemptions in use)."
    )
    # The discounted workflow invocations are printed on success too: they are the
    # cases where a `cargo test` line exists and provides no coverage, which is
    # exactly what a reader skimming the workflows would otherwise miscount.
    for line in sorted(set(workflow_ledger)):
        print(f"  · {line}")
    return 0


def covers_binary_ignoring_env(inv: Invocation, b: TestBinary) -> bool:
    saved = b.needs
    try:
        b.needs = set()
        return covers_binary(inv, b)
    finally:
        b.needs = saved


if __name__ == "__main__":
    sys.exit(main())
