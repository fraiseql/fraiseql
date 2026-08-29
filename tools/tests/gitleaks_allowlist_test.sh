#!/usr/bin/env bash
# Self-test for .gitleaks.toml — the repository's only executing secret scanner (#1208).
#
# Run directly:  bash tools/tests/gitleaks_allowlist_test.sh
# Requires `gitleaks` on PATH. Exits non-zero if any assertion fails.
#
# ── What this file is for ─────────────────────────────────────────────────────
#
# A secret scanner is trivially "green": allowlist everything and it never fires
# again. #1206 deleted a TruffleHog job that had never run once, and nothing
# noticed for three months, because a scanner that reports nothing looks exactly
# like a clean tree.
#
# So every exemption in .gitleaks.toml gets TWO assertions here:
#   - a POSITIVE case proving the exemption does what it claims, and
#   - a NEGATIVE case proving it does not reach further than it claims.
# Without the negatives, a config that blanket-disabled every rule would pass.
# Without the positives, a config that exempted nothing would pass.
#
# The negatives are the ones that earn their keep. gitleaks 8.28.0 silently
# ignores unknown config keys, so the natural way to scope an exemption to one
# rule — `targetRules` on a top-level `[[allowlists]]` block — is accepted,
# ignored, and applied to every rule instead. Case C is what catches that: an
# AWS key planted inside a path exempted for JWTs must still be reported.
#
# No complete credential is written into this file. Every planted secret is
# concatenated from fragments at runtime, so scanning this repository does not
# report the test that guards the scanner.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONFIG="$REPO_ROOT/.gitleaks.toml"

if ! command -v gitleaks >/dev/null 2>&1; then
    echo "❌ gitleaks not on PATH — this self-test runs inside the Dagger secret-scan container"
    exit 1
fi
[ -f "$CONFIG" ] || { echo "❌ missing $CONFIG"; exit 1; }

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ── Planted credentials, assembled so they exist only at runtime ──────────────
# Each is a syntactically valid credential of its kind and a live secret of
# nothing: the AWS id is not issued, the RSA body is 'A' padding, the JWT is
# unsigned.
AKIA="AKIA""Z3XQ7T2LMNPQ4RSV"
STRIPE_REAL="sk_""live_""51H8xQ2eZvKYlo2CkQ4rTbNmXwJ7dFpG9sAeR3uY"
STRIPE_FIXTURE="sk_""live_""abc123def456"
JWT="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9""."\
"eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IlRlc3QifQ""."\
"dBjftJeZ4CVPmB92K27uhbUJU1p1r8W1gFWFOEjXk"
PEM_HEAD="-----BEGIN ""PRIVATE KEY-----"
PEM_TAIL="-----END ""PRIVATE KEY-----"
PEM="$PEM_HEAD
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7VJTUt9Us8cKj
$(printf 'A%.0s' {1..64})
$(printf 'A%.0s' {1..64})
$PEM_TAIL"

# expect <leak|clean> <case-id> <relative-path> <file-content>
#
# One planted item per fixture tree, so the verdict is attributable to it alone
# and the assertion is gitleaks' exit code — no report parsing, nothing to
# misread.
expect() {
    local want="$1" id="$2" relpath="$3" content="$4"
    local dir="$WORK/$id" got
    TESTS_RUN=$((TESTS_RUN + 1))

    mkdir -p "$dir/$(dirname "$relpath")"
    printf '%s\n' "$content" > "$dir/$relpath"

    if gitleaks dir "$dir" --config "$CONFIG" --no-banner --redact \
            --report-format json --report-path "$dir/.report.json" >/dev/null 2>&1; then
        got=clean
    else
        got=leak
    fi

    if [ "$got" = "$want" ]; then
        printf '  ✅ %-4s %s\n' "$id" "expected $want at $relpath"
    else
        printf '  ❌ %-4s %s — expected %s, got %s\n' "$id" "$relpath" "$want" "$got"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "gitleaks allowlist self-test ($(gitleaks version 2>/dev/null || echo '?'))"
echo

echo "── the scanner runs, and a clean tree is clean ──"
expect clean A0 "crates/demo/src/lib.rs" 'pub fn add(a: u32, b: u32) -> u32 { a + b }'
expect leak  A1 "crates/demo/src/lib.rs" "const KEY: &str = \"$AKIA\";"

echo
echo "── test-path exemptions are scoped to their rule, not to the path ──"
# D/E: the jwt exemption covers *_tests.rs and not its sibling.
expect clean D  "crates/demo/src/oauth_tests.rs" "let token = \"$JWT\";"
expect leak  E  "crates/demo/src/oauth.rs"       "let token = \"$JWT\";"
# C: the same exempted file, a rule the exemption does not name. This is the
# case that fails if an exemption is written with the ignored `targetRules` key.
expect leak  C  "crates/demo/src/oauth_tests.rs" "let key = \"$AKIA\";"

echo
echo "── the generated dev-cert directory is exempt; its generator is not ──"
# docker/tls/certs/ is written by docker/tls/gen-certs.sh (`make db-up`) and is
# gitignored, so nothing in it can be committed — the artifact-tree claim. The
# generator beside it IS in the repository.
expect clean F0 "docker/tls/certs/server.key" "$PEM"
expect leak  F1 "docker/tls/gen-certs.sh"     "# $PEM"
# #1211 deleted docker/tls-postgres/ and the by-path exemption its two tracked
# private keys had. Nothing under that name is exempt any more, so a key there
# fails the gate — which is what stops the rig, or its keys, coming back quietly.
expect leak  F2 "docker/tls-postgres/certs/ca.key" "$PEM"

echo
echo "── generated .pem fixtures are exempt; a key pasted into source is not ──"
expect clean G0 "crates/demo/test_data/test_rsa_key.pem" "$PEM"
expect leak  G1 "crates/demo/src/startup.rs"             "const K: &str = \"$PEM\";"

echo
echo "── value exemptions are pinned to the literal ──"
# The two redaction-test fixtures are exempt; a Stripe-shaped key that is not
# one of them fires, in a test file, where the jwt/generic exemptions live.
expect clean H0 "crates/demo/src/audit_tests.rs" "let k = \"$STRIPE_FIXTURE\";"
expect leak  H1 "crates/demo/src/audit_tests.rs" "let k = \"$STRIPE_REAL\";"
# An env var NAME is exempt; a value assigned to that name is not.
expect clean I0 "crates/demo/src/config.rs" 'secret_env = "FRAISEQL_HS256_SECRET"'
expect leak  I1 "crates/demo/src/config.rs" "aws_key = \"$AKIA\""

echo
echo "── the artifact-tree exemption is a directory, not a name ──"
expect clean J0 "target/debug/deps/libpkcs8-abc.rmeta"        "$PEM"
expect clean J1 "sdks/official/fraiseql-php/vendor/dep/x.php" "\$k = \"$AKIA\";"
expect leak  J2 "crates/demo/src/target_config.rs"            "let k = \"$AKIA\";"
expect leak  J3 "crates/demo/vendor_notes.rs"                 "let k = \"$AKIA\";"

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "❌ $TESTS_FAILED of $TESTS_RUN assertions failed"
    exit 1
fi
echo "✅ all $TESTS_RUN assertions passed"
