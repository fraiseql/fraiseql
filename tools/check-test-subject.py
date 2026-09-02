#!/usr/bin/env python3
"""The test-subject gate: an integration test binary must reference the code it tests.

`tools/check-empty-tests.sh` refuses a `#[test]` whose body is only comments. This
is the next rung of the same ladder: a test binary whose *whole file* asserts
against subjects it defined itself.

Twenty-three of them existed — 9 633 lines, 273 tests (#1269, #1270). Fourteen sat
in the **required** `Dagger — test` leg: six named in `serverInProcessTests`, and
eight more reached by `cargo test --workspace`, which does not exclude
`fraiseql-cli`. `security_audit_test.rs` asserted that the
literal `"' OR '1'='1"` contains a quote and printed
`✅ SQL injection prevention test passed`; `mutation_nullability.rs` built
`json!({"return_type": "User!"})` and asserted the field equalled `"User!"`;
`cost_command_tests.rs` defined `calculate_cost` under the comment *"This would
call the actual cost command / For testing, we use a simple calculation"* and
tested that. Every one of them was green on every run and none could ever fail
for a reason originating in this project.

That is worse than absent coverage: the filenames, the green count and the
checkmarks are a positive claim that pooling, metrics, federation composition
and the security controls are exercised before a merge.

The rule
--------
Every integration test binary under `crates/*/tests/*.rs` — together with every
module file it includes — must reference a workspace crate at least once:
`fraiseql::`, or any `fraiseql_*` crate path (`fraiseql_core`, `fraiseql_server`,
`fraiseql_test_support`, …).

Two properties keep it honest:

* **Comments do not count.** `security_audit_test.rs` opens with "Security Audit
  Tests for FraiseQL Server" and touches nothing. A gate reading raw text would
  have passed it on its own header.
* **Included modules do count.** `api_schema_tests.rs` mentions no crate itself
  and is real coverage — it reaches the server through `mod common;`. Module
  declarations are resolved to disk (including the `mod outer { mod inner; }`
  harness-root shape used by `security.rs`, `integration.rs` and `property.rs`)
  and the whole binary is scanned, so a bare `grep -c fraiseql` — a screen, not
  a verdict — cannot condemn it.

Strings *do* count: a CLI test whose only reference is `Command::new("fraiseql")`
is invoking the real binary.

Runs in preflight ShellGates (python3, stdlib only) and locally via
`make lint-test-subject`. Its red-capability pin is `make test-test-subject-gate`.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
# Overridable so the red-capability pin (tools/tests/test_subject_test.sh) can
# point the gate at synthetic trees instead of asserting against the real one —
# a gate proved only on the live repo is proved only until the repo changes.
ROOT = Path(os.environ.get("TEST_SUBJECT_ROOT", REPO))

# A reference to a workspace crate *in code*. Three spellings, all of which mean
# the binary reaches this project:
#   fraiseql_core::…        a Rust path (also fraiseql_test_support, …)
#   fraiseql::…             the `fraiseql` facade crate
#   "fraiseql-cli"          the binary invoked by name — Command::cargo_bin() and
#                           env!("CARGO_BIN_EXE_fraiseql-cli") drive the real CLI
#                           end to end, which is the strongest coverage there is.
#                           No leading \\b: the env! spelling is CARGO_BIN_EXE_fraiseql-cli,
#                           where the preceding "_" is itself a word character.
#
# The bare word `fraiseql` is deliberately NOT a reference: it is what prose says.
# `operational_tools_test.rs` mentioned it only in "postgres://localhost/fraiseql"
# and `documentation_examples_test.rs` only inside a Python snippet it never ran —
# both were mock suites, and a laxer pattern would have cleared them.
CRATE_REF = re.compile(r"\bfraiseql_[a-z0-9_]+\b|\bfraiseql\s*::|fraiseql-[a-z0-9-]+\b")

MOD_FILE = re.compile(r"\bmod\s+([a-z_][a-z0-9_]*)\s*;")
# Anchored at the end of the slice preceding a `{`, so only a declaration
# *immediately* before that brace opens a module directory. A windowed
# `search` would find an earlier `mod x {` in the same window and conclude the
# brace was not a module's.
MOD_INLINE = re.compile(r"\bmod\s+([a-z_][a-z0-9_]*)\s*$")


RAW_OPEN = re.compile(r"(?:b?r)(#*)\"")


def scan(text: str) -> tuple[str, str]:
    """One pass over a Rust source, returning two masked copies of it.

    * `prose_free` — comments blanked, string literals kept. This is what the
      crate-reference search reads: a header claiming a subject must not count,
      but `Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))` must.
    * `code_only` — comments *and* strings blanked. This is what the module
      walker reads, so a brace inside a string or a doc block cannot skew the
      depth that decides which directory a `mod X;` resolves against.

    Both are offset- and line-preserving, so positions remain comparable.

    Char literals are tracked because `'"'` is real Rust — `.replace('"', ..)`
    appears in this tree — and a scanner that reads that quote as a string
    opener desynchronises for the rest of the file, blanking live code. A `'`
    is a literal when escaped (`'\n'`) or closed two characters later (`'x'`);
    otherwise it opens a lifetime (`'a`) and is ordinary code.
    """
    n = len(text)
    prose_free, code_only = list(text), list(text)

    def blank(buf: list[str], a: int, b: int) -> None:
        for k in range(a, min(b, n)):
            if buf[k] != "\n":
                buf[k] = " "

    i = 0
    while i < n:
        two = text[i : i + 2]
        if two == "//":
            j = text.find("\n", i)
            j = n if j < 0 else j
            blank(prose_free, i, j)
            blank(code_only, i, j)
            i = j
        elif two == "/*":
            depth, j = 1, i + 2  # Rust block comments nest
            while j < n and depth:
                if text[j : j + 2] == "/*":
                    depth, j = depth + 1, j + 2
                elif text[j : j + 2] == "*/":
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            blank(prose_free, i, j)
            blank(code_only, i, j)
            i = j
        elif (m := RAW_OPEN.match(text, i)) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            close = '"' + "#" * len(m.group(1))
            end = text.find(close, m.end())
            end = n if end < 0 else end + len(close)
            blank(code_only, i, end)  # kept in prose_free: strings count as references
            i = end
        elif text[i] == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            blank(code_only, i, j + 1)
            i = j + 1
        elif text[i] == "'":
            if i + 1 < n and text[i + 1] == "\\":  # escaped char literal
                j = i + 2
                while j < n and text[j] != "'":
                    j += 1
                i = j + 1
            elif i + 2 < n and text[i + 2] == "'":  # plain char literal
                i += 3
            else:  # a lifetime
                i += 1
        else:
            i += 1
    return "".join(prose_free), "".join(code_only)


def module_files(struct_text: str, base: Path) -> list[Path]:
    """Resolve every `mod X;` in one file to the path Rust would load.

    `base` is the directory submodules of *this* file live in: the root file's
    own directory for a crate root, `<dir>/<stem>/` for an included file. An
    inline `mod outer { … }` shifts that to `<base>/outer/` for declarations
    inside its braces, which is how `security.rs`'s `mod security { mod … }`
    reaches `tests/security/*.rs`.
    """
    # Directory in force at each brace depth, tracked by walking the braces.
    dirs: list[Path] = [base]
    found: list[Path] = []
    i, n = 0, len(struct_text)
    while i < n:
        c = struct_text[i]
        if c == "{":
            m = MOD_INLINE.search(struct_text[max(0, i - 80) : i])
            dirs.append(dirs[-1] / m.group(1) if m else dirs[-1])
            i += 1
        elif c == "}":
            if len(dirs) > 1:
                dirs.pop()
            i += 1
        else:
            m = MOD_FILE.match(struct_text, i)
            if m:
                name = m.group(1)
                for cand in (dirs[-1] / f"{name}.rs", dirs[-1] / name / "mod.rs"):
                    if cand.is_file():
                        found.append(cand)
                        break
                i = m.end()
            else:
                i += 1
    return found


def binary_sources(root: Path) -> list[tuple[Path, str]]:
    """The root file plus every module file the binary transitively includes.

    Each entry carries the file's comment-free text, so the caller searches for
    a crate reference without re-reading or re-scanning anything.
    """
    seen: list[tuple[Path, str]] = []
    visited: set[Path] = set()
    pending = [(root, root.parent)]
    while pending:
        path, base = pending.pop()
        if path in visited:
            continue
        visited.add(path)
        prose_free, code_only = scan(path.read_text(errors="replace"))
        seen.append((path, prose_free))
        for child in module_files(code_only, base):
            sub = child.parent if child.name == "mod.rs" else child.parent / child.stem
            pending.append((child, sub))
    return seen


def main() -> int:
    offenders: list[tuple[Path, int]] = []
    checked = 0
    for tests_dir in sorted(ROOT.glob("crates/*/tests")):
        for root in sorted(tests_dir.glob("*.rs")):
            checked += 1
            files = binary_sources(root)
            if not any(CRATE_REF.search(text) for _, text in files):
                offenders.append((root.relative_to(ROOT), len(files)))

    if not checked:
        print("FAIL: discovered no test binaries under crates/*/tests — the gate is not looking")
        return 1

    for path, nfiles in offenders:
        span = "" if nfiles == 1 else f" (and the {nfiles - 1} module file(s) it includes)"
        print(f"✗ {path}{span}: references no fraiseql crate — it can only test itself")

    if offenders:
        print()
        print(f"FAIL: {len(offenders)} of {checked} test binaries assert against subjects they")
        print("define themselves (#1269, #1270). A binary in crates/*/tests must exercise this")
        print("project: import the real component, or delete the file. If it genuinely reaches")
        print("the crate through a helper module, declare that module — the gate follows it.")
        return 1

    print(f"OK: all {checked} test binaries reference the crate under test.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
