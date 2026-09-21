#!/usr/bin/env bash
# Unit tests for tools/check-doc-claims.sh.
#
# Run directly:  bash tools/tests/check_doc_claims_test.sh
# Exits non-zero if any assertion fails.
#
# One clean fixture tree, then one mutation per rule in each direction: the claim the rule
# exists to catch is red, the historical or negative form the rule must tolerate is green, and
# a tree with nothing for a rule to scan is red rather than vacuously green.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-doc-claims.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# clean_root <dir>: a tree every rule passes on.
clean_root() {
    local r="$1"
    mkdir -p "$r/docs/adr" "$r/crates/fraiseql-observers/src" "$r/crates/x/src/real_dir" \
             "$r/sdks/official/tests" "$r/sdks/official/fraiseql-a" "$r/sdks/official/fraiseql-b"
    printf '# guide\nPostgreSQL only.\n' > "$r/docs/guide.md"
    printf '# changelog\nMySQL support\n' > "$r/CHANGELOG.md"
    printf 'crates/x/src/real.rs   thing\nreal_dir.rs   moved into a directory\n' > "$r/architecture.md"
    : > "$r/crates/x/src/real.rs"
    printf '//! Actions: webhook, email.\n' > "$r/crates/fraiseql-observers/src/lib.rs"
    printf '# roadmap\n- open epics only\n' > "$r/roadmap.md"
    printf 'Cross-SDK parity suite: 2 authoring SDKs produce identical schema JSON\n' > "$r/README.md"
    printf 'emit fraiseql-a\nemit fraiseql-b\nfraiseql-cli compile\n' > "$r/sdks/official/tests/run_parity.sh"
}

# assert_gate <name> <expected-rc> <expected-substring> <root>
assert_gate() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local name="$1" want_rc="$2" want_sub="$3" root="$4"
    local out rc
    set +e
    out="$(DOC_CLAIMS_ROOT="$root" bash "$GATE" 2>&1)"
    rc=$?
    set -e
    if [[ "$rc" -eq "$want_rc" && "$out" == *"$want_sub"* ]]; then
        echo "  ok: $name"
    else
        echo "  FAIL: $name — rc=$rc (want $want_rc), output did not contain '$want_sub':" >&2
        printf '%s\n' "$out" | sed 's/^/      /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "check-doc-claims.sh:"
r="$WORK/clean"; clean_root "$r"
assert_gate "clean tree passes" 0 "doc claims: ok" "$r"

r="$WORK/backend"; clean_root "$r"; printf 'Supports MySQL and SQLite.\n' >> "$r/docs/guide.md"
assert_gate "rule 1: backend claimed as current is red" 1 "rule 1" "$r"

r="$WORK/backend_hist"; clean_root "$r"; printf 'MySQL was removed in v2.15.0.\n' >> "$r/docs/guide.md"
assert_gate "rule 1: historical phrasing passes" 0 "doc claims: ok" "$r"

r="$WORK/backend_adr"; clean_root "$r"; printf 'We chose MySQL.\n' > "$r/docs/adr/0001.md"
assert_gate "rule 1: an ADR may name a backend" 0 "doc claims: ok" "$r"

r="$WORK/ghost"; clean_root "$r"; printf 'ghost.rs   never existed\n' >> "$r/architecture.md"
assert_gate "rule 2: phantom file is red" 1 "names ghost.rs" "$r"

r="$WORK/no_names"; clean_root "$r"; printf 'prose only\n' > "$r/architecture.md"
assert_gate "rule 2: architecture.md naming nothing is red" 1 "nothing to check" "$r"

r="$WORK/sms"; clean_root "$r"; printf '//! - SMS, push notifications\n' >> "$r/crates/fraiseql-observers/src/lib.rs"
assert_gate "rule 3: SMS advertised is red" 1 "advertises SMS" "$r"

r="$WORK/sms_ok"; clean_root "$r"; printf '//! SMS and push are rejected as unsupported (H24); see #428.\n' >> "$r/crates/fraiseql-observers/src/lib.rs"
assert_gate "rule 3: SMS named with the rejection passes" 0 "doc claims: ok" "$r"

r="$WORK/stub"; clean_root "$r"; printf '/// Send SMS (stub for)\n' >> "$r/crates/fraiseql-observers/src/lib.rs"
assert_gate "rule 3: stub wording is red" 1 "as a stub" "$r"

r="$WORK/roadmap"; clean_root "$r"; printf '**Current Stable**: v2.8.0\n' >> "$r/roadmap.md"
assert_gate "rule 4: version status line is red" 1 "rule 4" "$r"

r="$WORK/count"; clean_root "$r"; sed -i 's/2 authoring/9 authoring/' "$r/README.md"
assert_gate "rule 5: README count differing from the suite is red" 1 "covers 9 SDKs" "$r"

r="$WORK/no_sentence"; clean_root "$r"; printf 'nothing here\n' > "$r/README.md"
assert_gate "rule 5: README without the sentence is red" 1 "no 'parity suite" "$r"

r="$WORK/no_md"; mkdir -p "$r"
assert_gate "empty tree is red on every rule, not green" 1 "rule 1: no markdown" "$r"

echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
