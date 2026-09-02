#!/usr/bin/env bash
# test_subject_test.sh — the red capability of tools/check-test-subject.py.
#
# A gate that deletes 23 always-green test binaries cannot be shown to work by a
# green run: the tree it now passes over is the tree it just emptied. So every
# branch is proved on a synthetic tree instead — the offending shape must go RED
# and the legitimate shape must stay GREEN.
#
# Cases 3, 6 and 7 are the ones that actually bit while the gate was written:
#   * a header saying "Tests for FraiseQL" cleared the file until comments were
#     stripped (security_audit_test.rs would have passed on its own doc block);
#   * `CARGO_BIN_EXE_fraiseql-cli` failed a `\bfraiseql-` match, because the `_`
#     before it is itself a word character — six real CLI suites that drive the
#     actual binary were briefly condemned;
#   * `"postgres://localhost/fraiseql"` is prose in a URL, and a laxer pattern
#     accepting the bare word would have cleared operational_tools_test.rs.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-test-subject.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

run_gate() {
  TEST_SUBJECT_ROOT="$1" python3 "$gate" > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

mk() { mkdir -p "$(dirname "$1")"; printf '%s\n' "$2" > "$1"; }

expect() { # expect <rc> <substring> <label>
  if [ "$(cat "${tmp}/rc")" = "$1" ] && grep -F "$2" "${tmp}/out" >/dev/null; then
    pass "$3"
  else
    fail "$3"
  fi
}

# ── 1. A binary that asserts against its own literals fails ──────────────────────────
root="${tmp}/mock"
mk "${root}/crates/fraiseql-server/tests/security_audit_test.rs" '
#[test]
fn test_sql_injection_prevention() {
    let malicious_input = "'"'"' OR '"'"'1'"'"'='"'"'1";
    assert!(malicious_input.contains('"'"'\'"'"''"'"'));
    println!("OK SQL injection prevention test passed");
}'
run_gate "$root"
expect 1 "security_audit_test.rs" "a binary asserting against its own literals fails"

# ── 2. A binary importing the crate under test passes ────────────────────────────────
root="${tmp}/real"
mk "${root}/crates/fraiseql-server/tests/real_test.rs" '
use fraiseql_server::error::GraphQLError;
#[test]
fn t() { let _ = GraphQLError::request("x"); }'
run_gate "$root"
expect 0 "all 1 test binaries" "a binary importing fraiseql_server passes"

# ── 3. A crate named ONLY in a comment still fails ───────────────────────────────────
# The shape that made this gate necessary: every deleted file had a header
# claiming the subject it never touched.
root="${tmp}/commentonly"
mk "${root}/crates/fraiseql-server/tests/doc_only_test.rs" '
//! Security tests for fraiseql_server — validates the real handler.
/* fraiseql_core::runtime::Executor is what this would exercise */
#[test]
fn t() { assert!(true); }'
run_gate "$root"
expect 1 "doc_only_test.rs" "a crate named only in comments does not count as a reference"

# ── 4. A binary reaching the crate through `mod common;` passes ──────────────────────
# api_schema_tests.rs greps zero and is real coverage. `grep -c fraiseql` is a
# screen, not a verdict.
root="${tmp}/viacommon"
mk "${root}/crates/fraiseql-server/tests/api_schema_tests.rs" '
mod common;
#[test]
fn t() { common::boot(); }'
mk "${root}/crates/fraiseql-server/tests/common/mod.rs" '
use fraiseql_server::Server;
pub fn boot() -> Option<Server> { None }'
run_gate "$root"
expect 0 "all 1 test binaries" "a binary reaching the crate via 'mod common;' passes"

# ── 5. The `mod outer { mod inner; }` harness-root shape resolves ────────────────────
# security.rs / integration.rs / property.rs declare their suites nested, which
# puts the backing files in tests/<outer>/ rather than tests/.
root="${tmp}/harness"
mk "${root}/crates/fraiseql-server/tests/security.rs" '
mod security {
    mod auth_bypass_detection_test;
}'
mk "${root}/crates/fraiseql-server/tests/security/auth_bypass_detection_test.rs" '
use fraiseql_auth::verify;
#[test]
fn t() { let _ = verify(); }'
run_gate "$root"
expect 0 "all 1 test binaries" "the nested harness-root shape resolves to tests/<outer>/"

# ── 6. `CARGO_BIN_EXE_fraiseql-cli` counts — it drives the real binary ───────────────
root="${tmp}/binexe"
mk "${root}/crates/fraiseql-cli/tests/lint_command_tests.rs" '
use std::process::Command;
#[test]
fn t() { let _ = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli")).output(); }'
run_gate "$root"
expect 0 "all 1 test binaries" "invoking the real binary via CARGO_BIN_EXE counts as coverage"

# ── 7. The bare word `fraiseql` in a string does NOT count ───────────────────────────
root="${tmp}/bareword"
mk "${root}/crates/fraiseql-server/tests/operational_tools_test.rs" '
#[test]
fn t() {
    let database_url = "postgres://localhost/fraiseql".to_string();
    assert!(!database_url.is_empty());
}'
run_gate "$root"
expect 1 "operational_tools_test.rs" "a bare 'fraiseql' in a URL is prose, not a reference"

# ── 8. A `'"'` char literal must not desynchronise the scanner ───────────────────────
# `.replace('"', ..)` is real Rust in this tree. Read as a string opener, the quote
# swallows the rest of the file, so the trailing comment is never recognised AS a
# comment and its `fraiseql_core` counts as a reference — the file passes on prose.
root="${tmp}/charlit"
mkdir -p "${root}/crates/fraiseql-db/tests"
cat > "${root}/crates/fraiseql-db/tests/identifier_properties.rs" <<'RS'
#[test]
fn t() {
    let s = String::new().replace('"', "x");
    assert!(s.is_empty());
}
// would exercise fraiseql_core::runtime, once written
RS
run_gate "$root"
expect 1 "identifier_properties.rs" "a '\"' char literal does not desynchronise comment stripping"

# ── 9. A `mod` inside a raw string is inert ──────────────────────────────────────────
# The raw string holds an unbalanced quote — which is what raw strings are for. Scanned
# as ordinary quote pairs, the span containing `mod common;` falls *between* pairs and
# survives, so the walker follows it into a real module and the binary passes on a
# module it never declared.
root="${tmp}/rawstr"
mkdir -p "${root}/crates/fraiseql-cli/tests"
cat > "${root}/crates/fraiseql-cli/tests/raw_test.rs" <<'RS'
#[test]
fn t() {
    let sql = r#"a "b FROM t; mod common;"#;
    assert!(!sql.is_empty());
}
RS
cat > "${root}/crates/fraiseql-cli/tests/common.rs" <<'RS'
use fraiseql_cli::compile;
pub fn boot() { let _ = compile; }
RS
run_gate "$root"
expect 1 "raw_test.rs" "a 'mod' inside a raw string is not followed"

# ── 10. Discovering no binaries is a failure, not a pass ──────────────────
# The #1216 shape: a gate that looks at nothing reports OK forever.
root="${tmp}/empty"
mkdir -p "${root}/crates"
run_gate "$root"
expect 1 "discovered no test binaries" "an empty scan fails rather than reporting OK"

if [ "$failures" -ne 0 ]; then
  echo "FAIL: ${failures} check-test-subject.py assertion(s) failed"
  exit 1
fi
echo "OK: check-test-subject.py goes red on self-referential binaries and stays green on real ones."
