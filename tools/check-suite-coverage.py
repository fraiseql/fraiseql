#!/usr/bin/env python3
"""The no-orphan-suites gate: every test suite maps to a CI leg that executes it.

"Execution coverage must be a checked artifact, not an inference" — the
2026-07-27 remediation program's retrospective, rule 1. This gate makes it so:

1. **Discover what exists** — every `tests/*.rs` integration binary (with its
   `[[test]] required-features`) and every feature-gated `tests.rs` unit-test
   module under `src/`, per workspace crate.
2. **Discover what runs** — every `cargo test` / `cargo nextest` invocation in
   `.dagger/main.go`, attributed to its leg function, with feature lists
   resolved and the leg's bound services (DATABASE_URL etc.) recorded. The gate
   reads the SAME file the legs execute, so gate and legs cannot drift.
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
            r"test_support::postgres\(|try_database_url\(|\bdatabase_url\(\)|"
            + ENV_READ.format(name="(?:TLS_)?DATABASE_URL")
        ),
        "satisfied_by": {"DATABASE_URL", "TLS_DATABASE_URL"},
    },
    "redis": {
        "detect": re.compile(r"test_support::redis\(|" + ENV_READ.format(name="REDIS_URL")),
        "satisfied_by": {"REDIS_URL"},
    },
    "nats": {
        "detect": re.compile(r"test_support::nats\(|" + ENV_READ.format(name="NATS_URL")),
        "satisfied_by": {"NATS_URL"},
    },
    "minio": {
        "detect": re.compile(r"test_support::minio\(|" + ENV_READ.format(name="MINIO_ENDPOINT")),
        "satisfied_by": {"MINIO_ENDPOINT"},
    },
    "azure_blob": {
        "detect": re.compile(
            r"test_support::azure_blob\(|" + ENV_READ.format(name="AZURE_BLOB_ENDPOINT")
        ),
        "satisfied_by": {"AZURE_BLOB_ENDPOINT"},
    },
    "gcs": {
        "detect": re.compile(r"test_support::gcs\(|" + ENV_READ.format(name="GCS_ENDPOINT")),
        "satisfied_by": {"GCS_ENDPOINT"},
    },
    "vault": {
        "detect": re.compile(r"test_support::vault\(|" + ENV_READ.format(name="VAULT_ADDR")),
        "satisfied_by": {"VAULT_ADDR"},
    },
}

ALL_FEATURES = object()  # sentinel: --all-features


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
        b.required_features = declared.get(f.stem, set())
        src = f.read_text(encoding="utf-8", errors="replace")
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
        self.filters: list[str] = []  # positive test-name filters
        self.skips: list[str] = []  # --skip tokens
        self.ignored_mode = False  # `-- --ignored`: runs ONLY #[ignore]d tests
        self.include_ignored = False  # `-- --include-ignored`: runs both classes
        self.env: set[str] = set()  # leg-bound env vars (filled later)

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


# ── Phase C: coverage ─────────────────────────────────────────────────────────


def features_enabled(inv: Invocation, wanted: set[str]) -> bool:
    if inv.features is ALL_FEATURES:
        return True
    assert isinstance(inv.features, set)
    return wanted <= inv.features


def covers_binary(inv: Invocation, b: TestBinary) -> bool:
    if inv.lib_only or inv.doc_only:
        return False
    if inv.crate is not None and inv.crate != b.crate:
        return False
    if inv.workspace and b.crate in inv.excludes:
        return False
    if inv.crate is None and not inv.workspace:
        return False
    if inv.tests and "*" not in inv.tests and b.name not in inv.tests:
        return False
    if not features_enabled(inv, b.required_features):
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
    if inv.crate is not None and inv.crate != mod.crate:
        return False
    if inv.workspace and mod.crate in inv.excludes:
        return False
    if inv.crate is None and not inv.workspace:
        return False
    if not features_enabled(inv, mod.features):
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
    invocations = extract_invocations(main_go)
    exemptions = load_exemptions()

    crates = sorted(p for p in (REPO / "crates").iterdir() if (p / "Cargo.toml").exists())
    binaries: list[TestBinary] = []
    modules: list[LibTestModule] = []
    crate_names = set()
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
            f"checked against {len(invocations)} leg invocations."
        )
        print("  Wire the suite into a leg in .dagger/main.go, or add an exemption with a")
        print("  reason to tools/suite-coverage-exemptions.toml.")
        return 1

    print(
        f"suite-coverage: OK — {len(binaries)} binaries and {len(modules)} feature-gated "
        f"lib modules all covered ({len(invocations)} leg invocations, "
        f"{len(used_exemptions)} exemptions in use)."
    )
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
