#!/usr/bin/env bash
# Unit tests for tools/check-deny-lint-flags.py.
#
# Run directly:  bash tools/tests/deny_lint_flags_test.sh
# Exits non-zero if any assertion fails.
#
# The gate escalates cargo-deny's `unmatched-skip-root` / `unmatched-skip` lints, which
# default to WARN — a stale exact-version skip pin therefore covers nothing while the run
# still exits 0 (#1020, #933). These assertions are red-capability assertions: each is a
# shape the gate must REJECT. Three of them are shapes an earlier draft of this gate got
# wrong, and each of those wrong verdicts was silent:
#
#   – prose quoting the command (deny.toml's own `reason` string) read as an invocation;
#   – `-D unmatched-skip-root` alone satisfying a containment test for `unmatched-skip`,
#     because the latter is a SUBSTRING of the former;
#   – flags on a NEIGHBOURING line counting toward a flagless invocation.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-deny-lint-flags.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# assert_gate <name> <expected-exit> <file-name> <contents>
assert_gate() {
    local name="$1" want_exit="$2" fname="$3" contents="$4"
    TESTS_RUN=$((TESTS_RUN + 1))
    local dir="$WORK/$name"
    mkdir -p "$dir"
    printf '%s\n' "$contents" >"$dir/$fname"

    local out rc
    set +e
    out="$(DENY_FLAGS_ROOT="$dir" python3 "$GATE" 2>&1)"
    rc=$?
    set -e

    if [ "$rc" -ne "$want_exit" ]; then
        echo "  FAIL: $name — expected exit $want_exit, got $rc"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        echo "  ok: $name"
    fi
}

# assert_gate2 <name> <expected-exit> <f1> <c1> <f2> <c2> — two-file fixture, for cases
# that need a real invocation present alongside the shape under test (otherwise the
# vacuous-scan guard fires and masks what is being asserted).
assert_gate2() {
    local name="$1" want_exit="$2" f1="$3" c1="$4" f2="$5" c2="$6"
    TESTS_RUN=$((TESTS_RUN + 1))
    local dir="$WORK/$name"
    mkdir -p "$dir"
    printf '%s\n' "$c1" >"$dir/$f1"
    printf '%s\n' "$c2" >"$dir/$f2"

    local out rc
    set +e
    out="$(DENY_FLAGS_ROOT="$dir" python3 "$GATE" 2>&1)"
    rc=$?
    set -e

    if [ "$rc" -ne "$want_exit" ]; then
        echo "  FAIL: $name — expected exit $want_exit, got $rc"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        echo "  ok: $name"
    fi
}

echo "=== check-deny-lint-flags.py ==="

# ── The defect itself ────────────────────────────────────────────────────────────
assert_gate "flagless-invocation-is-rejected" 1 Makefile \
'security:
	cargo deny check'

assert_gate "both-flags-pass" 0 Makefile \
'security:
	cargo deny check -D unmatched-skip-root -D unmatched-skip'

# ── The substring trap ───────────────────────────────────────────────────────────
# `unmatched-skip` occurs inside `unmatched-skip-root`. A containment test for the
# bare form is satisfied by the -root form alone, which would green a config whose
# [[bans.skip]] entries are still unguarded.
assert_gate "root-flag-alone-is-rejected" 1 Makefile \
'security:
	cargo deny check -D unmatched-skip-root'

assert_gate "bare-flag-alone-is-rejected" 1 Makefile \
'security:
	cargo deny check -D unmatched-skip'

# ── The neighbouring-command trap ────────────────────────────────────────────────
# A forward window that is not bounded by the end of the command lets an adjacent
# line's flags satisfy a flagless invocation.
assert_gate "flags-on-a-neighbouring-command-do-not-count" 1 Makefile \
'security:
	cargo deny check
	echo "-D unmatched-skip-root -D unmatched-skip"'

# ── Scope ────────────────────────────────────────────────────────────────────────
# `check advisories` cannot trip the bans lints, so requiring the flags there would be
# noise. But naming bans alongside it must still be gated.
assert_gate "advisories-only-is-exempt" 0 audit.yml \
'      run: cargo deny check advisories
      # a real invocation must exist too, or the vacuous-scan guard fires
      run2: cargo deny check -D unmatched-skip-root -D unmatched-skip'

assert_gate "scoped-but-covering-bans-is-gated" 1 audit.yml \
'      run: cargo deny check advisories bans'

# ── Prose must not be mistaken for an invocation ─────────────────────────────────
# deny.toml:432 quotes `cargo deny check bans` inside a `reason` string while warning
# about this exact failure mode. The first draft of the gate failed on it.
assert_gate2 "prose-quoting-the-command-is-not-an-invocation" 0 deny.toml \
'[[bans.skip-tree]]
name = "wasmtime"
reason = "an exact pin that goes stale un-skips the tree and `cargo deny check bans` reports every duplicate under it"
version = "=46.0.2"' Makefile \
'security:
	cargo deny check -D unmatched-skip-root -D unmatched-skip'

assert_gate "commented-out-invocation-is-ignored" 1 Makefile \
'security:
	# cargo deny check -D unmatched-skip-root -D unmatched-skip
	cargo deny check'

# ── The Go string-slice spelling, held open across lines by gofmt ────────────────
assert_gate "go-multiline-with-flags-passes" 0 security.go \
'	return m.denyBase().
		WithExec([]string{
			"cargo-deny", "check",
			"-D", "unmatched-skip-root",
			"-D", "unmatched-skip",
		}).
		Stdout(ctx)'

assert_gate "go-multiline-without-flags-is-rejected" 1 security.go \
'	return m.denyBase().
		WithExec([]string{
			"cargo-deny", "check",
		}).
		Stdout(ctx)'

assert_gate "go-singleline-with-flags-passes" 0 security.go \
'		WithExec([]string{"cargo-deny", "check", "-D", "unmatched-skip-root", "-D", "unmatched-skip"}).'

# ── The vacuous-scan guard ───────────────────────────────────────────────────────
# A discovery scan that finds nothing must fail loudly rather than report success —
# how three gates in this repo shipped unable to reject anything (#1075).
assert_gate "no-invocation-found-is-a-failure" 1 README.txt \
'this tree contains no cargo-deny invocation at all'

echo ""
if [ "$TESTS_FAILED" -ne 0 ]; then
    echo "FAILED: $TESTS_FAILED of $TESTS_RUN assertions"
    exit 1
fi
echo "OK: $TESTS_RUN/$TESTS_RUN assertions passed"
