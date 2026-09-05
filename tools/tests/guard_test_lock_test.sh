#!/usr/bin/env bash
# guard_test_lock_test.sh — the red capability of tools/check-guard-test-lock.py.
#
# The gate's own defect class is why it cannot be proved by a green run: it now passes
# over the tree it just cleaned, so "green" is equally consistent with "the scan matches
# nothing". Every branch is therefore proved on a synthetic tree — the offending shape
# must go RED, the legitimate shapes must stay GREEN, and a scan that found nothing must
# exit 2 rather than report success.
#
# The cases that actually bit while the gate was written:
#   * case 3 — a helper wrapping `with_guard_engaged` is still holding the lock. Without
#     the transitive closure the gate condemned correct suites, which is how a gate gets
#     switched off.
#   * case 6 — the chokepoint names were seeded into the entry-point set before the
#     emptiness check, so "discovered nothing" was indistinguishable from "all clear"
#     and the abort branch was unreachable.
#   * case 9 — a body truncated at the first `}` hides any guard call that follows a
#     nested block. That direction is silent: fewer tests checked, still exit 0.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-guard-test-lock.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

run_gate() {
  GUARD_TEST_LOCK_ROOT="$1" python3 "$gate" > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

expect() { # expect <rc> <substring> <label>
  if [ "$(cat "${tmp}/rc")" = "$1" ] && grep -F "$2" "${tmp}/out" >/dev/null; then
    pass "$3"
  else
    fail "$3"
  fi
}

mkf() { mkdir -p "$(dirname "$1")"; cat > "$1"; }

# Every synthetic tree needs a real guard entry point to discover, or the gate aborts
# (case 6) before reaching the branch under test.
seed_guard() {
  mkf "$1/crates/fraiseql-x/src/guard.rs" <<'RS'
pub fn guard_thing(addr: &str) -> bool {
    if fraiseql_guard::deployment::insecure_bypass("FRAISEQL_X_ALLOW_INSECURE").is_honoured() {
        return true;
    }
    !addr.is_empty()
}
RS
}

# ── 1. A lock-free reader beside a mutator fails ─────────────────────────────────────
root="${tmp}/c1"; seed_guard "$root"
mkf "${root}/crates/fraiseql-x/src/tests.rs" <<'RS'
#[test]
fn hatch_is_refused_in_production() {
    temp_env::with_vars([("FRAISEQL_ENV", Some("production"))], || {
        assert!(guard_thing("https://example.com"));
    });
}

#[test]
fn addr_is_blocked() {
    assert!(!guard_thing(""));
}
RS
run_gate "$root"
expect 1 "::addr_is_blocked" "1. lock-free reader beside a mutator fails"

# ── 2. The same test, taking the lock directly, passes ───────────────────────────────
root="${tmp}/c2"; seed_guard "$root"
mkf "${root}/crates/fraiseql-x/src/tests.rs" <<'RS'
#[test]
fn hatch_is_refused_in_production() {
    temp_env::with_vars([("FRAISEQL_ENV", Some("production"))], || {
        assert!(guard_thing("https://example.com"));
    });
}

#[test]
fn addr_is_blocked() {
    temp_env::with_vars([("FRAISEQL_X_ALLOW_INSECURE", None::<&str>)], || {
        assert!(!guard_thing(""));
    });
}
RS
run_gate "$root"
expect 0 "OK:" "2. taking the lock directly passes"

# ── 3. Two levels of helper still counts as holding the lock ─────────────────────────
# A gate that only looks one level deep condemns correct code, and a gate that
# condemns correct code gets deleted.
root="${tmp}/c3"; seed_guard "$root"
mkf "${root}/crates/fraiseql-x/src/tests.rs" <<'RS'
fn with_guard_engaged<T>(f: impl FnOnce() -> T) -> T {
    let mut out = None;
    temp_env::with_vars([("FRAISEQL_X_ALLOW_INSECURE", None::<&str>)], || out = Some(f()));
    out.unwrap()
}

fn under_engaged_guard(addr: &str) -> bool {
    with_guard_engaged(|| guard_thing(addr))
}

#[test]
fn hatch_is_refused_in_production() {
    temp_env::with_vars([("FRAISEQL_ENV", Some("production"))], || {
        assert!(guard_thing("https://example.com"));
    });
}

#[test]
fn addr_is_blocked() {
    assert!(!under_engaged_guard(""));
}
RS
run_gate "$root"
expect 0 "OK:" "3. a two-level helper chain still holds the lock"

# ── 4. A lock-free reader with no mutator in its binary passes ───────────────────────
# Each crates/*/tests/<stem>.rs is its own process. With nothing writing the env
# there is no race, and flagging it would be noise.
root="${tmp}/c4"; seed_guard "$root"
mkf "${root}/crates/fraiseql-x/src/tests.rs" <<'RS'
#[test]
fn hatch_is_refused_in_production() {
    temp_env::with_vars([("FRAISEQL_ENV", Some("production"))], || {
        assert!(guard_thing("https://example.com"));
    });
}
RS
mkf "${root}/crates/fraiseql-x/tests/lonely.rs" <<'RS'
#[test]
fn addr_is_blocked_in_its_own_process() {
    assert!(!guard_thing(""));
}
RS
run_gate "$root"
expect 0 "OK:" "4. a lock-free reader with no mutator in its binary is not flagged"
grep -F "addr_is_blocked_in_its_own_process" "${tmp}/out" >/dev/null \
  && { echo "  FAIL 4b. the unarmed binary was named anyway"; failures=$((failures + 1)); } \
  || pass "4b. …and is not named in the output"

# ── 5. A binary that mutates a NON-deployment variable does not arm the gate ─────────
root="${tmp}/c5"; seed_guard "$root"
mkf "${root}/crates/fraiseql-x/src/tests.rs" <<'RS'
#[test]
fn reads_an_unrelated_var() {
    temp_env::with_vars([("MY_APP_FIXTURE_PATH", Some("/tmp/x"))], || {
        assert_eq!(1 + 1, 2);
    });
}

#[test]
fn addr_is_blocked_beside_an_unrelated_mutation() {
    assert!(!guard_thing(""));
}
RS
# A second crate whose binary IS armed, so `checked` > 0 and the run reaches a verdict
# instead of aborting with nothing to look at.
mkf "${root}/crates/fraiseql-y/src/tests.rs" <<'RS'
#[test]
fn hatch_is_refused_in_production() {
    temp_env::with_vars([("FRAISEQL_ENV", Some("production"))], || {
        assert!(guard_thing("https://example.com"));
    });
}
RS
run_gate "$root"
expect 0 "OK:" "5. a non-deployment env mutation does not arm the gate"
grep -F "addr_is_blocked_beside_an_unrelated_mutation" "${tmp}/out" >/dev/null \
  && { echo "  FAIL 5b. the unarmed binary was named anyway"; failures=$((failures + 1)); } \
  || pass "5b. …and its lock-free reader is not named"

# ── 6. A tree with no discoverable guard aborts instead of reporting success ─────────
root="${tmp}/c6"
mkf "${root}/crates/fraiseql-x/src/tests.rs" <<'RS'
#[test]
fn addr_is_blocked() {
    temp_env::with_vars([("FRAISEQL_ENV", None::<&str>)], || assert!(true));
}
RS
run_gate "$root"
expect 2 "zero guard entry points" "6. a scan that discovers nothing exits 2, not 0"

# ── 7. Entry points and a mutator, but nothing asserting a guard, aborts ─────────────
root="${tmp}/c7"; seed_guard "$root"
mkf "${root}/crates/fraiseql-x/src/tests.rs" <<'RS'
#[test]
fn posture_is_read_somewhere_else() {
    temp_env::with_vars([("FRAISEQL_ENV", Some("production"))], || {
        assert_eq!(1 + 1, 2);
    });
}
RS
run_gate "$root"
expect 2 "zero test functions" "7. zero guard-asserting tests exits 2, not 0"

# ── 8. An exemption that names nothing is a hole, and says so ────────────────────────
# Proved against the real repo with a bogus row spliced in. The row must name a file the
# scan actually reaches: an exemption whose file is absent describes a different
# repository — the synthetic-tree case — and is skipped rather than reported as rotted.
patched="${tmp}/patched-gate.py"
sed 's|^EXEMPTIONS = \[|EXEMPTIONS = [\n    ("fraiseql-secrets/src/secrets_manager/backends/vault/tests.rs", "a_test_that_was_deleted", "bogus"),|' \
  "$gate" > "$patched"
grep -F 'a_test_that_was_deleted' "$patched" >/dev/null || { echo "  FAIL 8. splice did not apply"; failures=$((failures + 1)); }
GUARD_TEST_LOCK_ROOT="$repo_root" python3 "$patched" > "${tmp}/out" 2>&1; echo "$?" > "${tmp}/rc"
expect 2 "names nothing" "8. a stale exemption exits 2"

# ── 9. A guard call after a nested block is still seen ───────────────────────────────
# Brace matching, not a regex to the first `}`. Truncating the body here would drop
# the call, check one fewer test, and exit 0 — the silent direction.
root="${tmp}/c9"; seed_guard "$root"
mkf "${root}/crates/fraiseql-x/src/tests.rs" <<'RS'
#[test]
fn hatch_is_refused_in_production() {
    temp_env::with_vars([("FRAISEQL_ENV", Some("production"))], || {
        assert!(guard_thing("https://example.com"));
    });
}

#[test]
fn addr_is_blocked() {
    for probe in ["", " "] {
        assert!(probe.len() < 2);
    }
    assert!(!guard_thing(""));
}
RS
run_gate "$root"
expect 1 "::addr_is_blocked" "9. a guard call after a nested block is still seen"

# ── 10. …and the same shape, locked, stays green ─────────────────────────────────────
root="${tmp}/c10"; seed_guard "$root"
mkf "${root}/crates/fraiseql-x/src/tests.rs" <<'RS'
#[test]
fn hatch_is_refused_in_production() {
    temp_env::with_vars([("FRAISEQL_ENV", Some("production"))], || {
        assert!(guard_thing("https://example.com"));
    });
}

#[test]
fn addr_is_blocked() {
    for probe in ["", " "] {
        assert!(probe.len() < 2);
    }
    temp_env::with_vars([("FRAISEQL_X_ALLOW_INSECURE", None::<&str>)], || {
        assert!(!guard_thing(""));
    });
}
RS
run_gate "$root"
expect 0 "OK:" "10. the same shape, locked after a nested block, stays green"

# ── 11. The live repo is clean ───────────────────────────────────────────────────────
run_gate "$repo_root"
expect 0 "OK:" "11. the live repository passes"

echo ""
if [ "$failures" -ne 0 ]; then
  echo "guard_test_lock_test.sh: ${failures} case(s) failed"
  exit 1
fi
echo "guard_test_lock_test.sh: all cases passed"
