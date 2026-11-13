#!/usr/bin/env bash
# check-no-first-party-hs256.sh — no HS256 signing or verification code for first-party
# sign-in tokens.
#
# The server signs the sessions it mints with ES256 keys of its own (`[auth_issuer]`) and
# validates every bearer through `OidcValidator`. The shared-secret path (`[auth_hs256]`,
# its middleware, `fraiseql_auth::JwtValidator` and the tokens tests mint for it) is being
# removed. This gate keeps it from coming back, one file at a time:
#
#   EXEMPT   files that legitimately name HS256 for good: they sign tokens that are not
#            first-party sign-in tokens, or they prove an HS256 token is refused.
#   PENDING  files that still carry the path and are scheduled for removal. The list
#            only shrinks: an entry that no longer matches must be deleted from it.
#
# Every other `.rs` or `.toml` file under crates/ and examples/ must not match.
# Comment lines do not count: prose that names the removed path is not the path.
set -euo pipefail
cd "${HS256_GATE_ROOT:-$(git rev-parse --show-toplevel)}"

# Names of the removed path, and the two ways a test hand-signs a shared-secret token.
BANNED='auth_hs256|Hs256Config|with_hs256_secret|generate_hs256_token|\bJwtValidator\b|McpTokenValidator::Hs256|VerifiedSubject::Hs256|hs256_auth|\bAuthMiddleware\b|Algorithm::HS256|EncodingKey::from_secret'

# Exact paths, one per line. `crates/fraiseql-arrow/` is the one directory entry: that
# crate signs its own session tokens, which are not first-party sign-in tokens.
EXEMPT='
crates/fraiseql-arrow/
crates/fraiseql-server/tests/flight_decimal_pg.rs
crates/fraiseql-server/tests/flight_do_put_outbox_pg.rs
crates/fraiseql-core/src/security/oidc/tests.rs
crates/fraiseql-server/tests/first_party_issuer_e2e_pg.rs
'

PENDING='
crates/fraiseql-auth/benches/auth_bench.rs
crates/fraiseql-auth/fuzz/fuzz_targets/jwt_parse.rs
crates/fraiseql-auth/src/jwt/mod.rs
crates/fraiseql-auth/src/lib.rs
crates/fraiseql-auth/src/saml/tests.rs
crates/fraiseql-auth/src/security_tests.rs
crates/fraiseql-auth/src/session_postgres.rs
crates/fraiseql-auth/src/session_postgres/tests.rs
crates/fraiseql-auth/src/tests.rs
crates/fraiseql-auth/tests/postgres_scim_provisioning.rs
crates/fraiseql-auth/tests/postgres_session_mfa.rs
crates/fraiseql-auth/tests/postgres_session_tenant.rs
crates/fraiseql-auth/tests/public_api_is_self_sufficient.rs
crates/fraiseql-core/src/security/auth_middleware/middleware.rs
crates/fraiseql-core/src/security/auth_middleware/mod.rs
crates/fraiseql-core/src/security/auth_middleware/signing_key.rs
crates/fraiseql-core/src/security/auth_middleware/tests.rs
crates/fraiseql-core/src/security/mod.rs
crates/fraiseql-server/src/mcp/handler.rs
crates/fraiseql-server/src/mcp/tests.rs
crates/fraiseql-server/src/middleware/hs256_auth.rs
crates/fraiseql-server/src/middleware/mod.rs
crates/fraiseql-server/src/middleware/oidc_auth.rs
crates/fraiseql-server/src/middleware/rate_limit/identity.rs
crates/fraiseql-server/src/middleware/rate_limit/tests.rs
crates/fraiseql-server/src/middleware/tests.rs
crates/fraiseql-server/src/server/builder.rs
crates/fraiseql-server/src/server/capabilities/rest.rs
crates/fraiseql-server/src/server/mod.rs
crates/fraiseql-server/src/server/parity_tests.rs
crates/fraiseql-server/src/server/routing/admin.rs
crates/fraiseql-server/src/server/routing/auth.rs
crates/fraiseql-server/src/server/routing/extensions.rs
crates/fraiseql-server/src/server/routing/middleware.rs
crates/fraiseql-server/src/server/routing/mod.rs
crates/fraiseql-server/src/server_config/auth_issuer/tests.rs
crates/fraiseql-server/src/server_config/hs256.rs
crates/fraiseql-server/src/server_config/hs256/tests.rs
crates/fraiseql-server/src/server_config/methods.rs
crates/fraiseql-server/src/server_config/mod.rs
crates/fraiseql-server/src/server_config/tests.rs
crates/fraiseql-server/tests/auth_regression_test.rs
crates/fraiseql-server/tests/config_coverage_manifest_test.rs
crates/fraiseql-server/tests/config_struct_test.rs
crates/fraiseql-server/tests/graphql_sse_e2e_pg.rs
crates/fraiseql-server/tests/hs256_revocation_e2e_pg.rs
crates/fraiseql-server/tests/security/auth_bypass_detection_test.rs
examples/authentication/src/main.rs
'

# The self-test (tools/tests/no_first_party_hs256_test.sh) runs against fixture trees,
# which carry none of the real files, so it supplies its own lists.
EXEMPT="${HS256_GATE_EXEMPT-$EXEMPT}"
PENDING="${HS256_GATE_PENDING-$PENDING}"

# Lines of `$1` that match BANNED, comment lines left out.
hits_in() {
  grep -E "$BANNED" "$1" | grep -cvE '^[[:space:]]*(//|#)' || true
}

listed_in() { # $1 path, $2 list
  local entry
  while IFS= read -r entry; do
    [ -z "$entry" ] && continue
    case "$entry" in
      */) case "$1" in "$entry"*) return 0 ;; esac ;;
      *) [ "$1" = "$entry" ] && return 0 ;;
    esac
  done <<< "$2"
  return 1
}

files=$(
  { grep -rlE "$BANNED" crates examples --include='*.rs' --include='*.toml' \
      --exclude-dir=target 2>/dev/null || true; } | sort
)

matched=0
violations=''
for file in $files; do
  n=$(hits_in "$file")
  [ "${n:-0}" -eq 0 ] && continue
  matched=$((matched + 1))
  listed_in "$file" "$EXEMPT" && continue
  listed_in "$file" "$PENDING" && continue
  violations="${violations}  ${file}"$'\n'
done

if [ -n "$violations" ]; then
  echo "ERROR: first-party HS256 code outside the gate's lists:"
  printf '%s' "$violations"
  echo "First-party sessions are signed by [auth_issuer] (ES256) and validated by"
  echo "OidcValidator. Mint a test token from fraiseql_test_support::issuer::TestIssuer,"
  echo "or boot the server with [auth_issuer]. See tools/check-no-first-party-hs256.sh."
  exit 1
fi

# The lists stay exact: an entry that no longer matches must go, or it would excuse the
# next file to reintroduce the path at that spot.
stale=''
while IFS= read -r entry; do
  [ -z "$entry" ] && continue
  case "$entry" in */) continue ;; esac
  if [ ! -f "$entry" ] || [ "$(hits_in "$entry")" -eq 0 ]; then
    stale="${stale}  ${entry}"$'\n'
  fi
done <<< "${EXEMPT}${PENDING}"
if [ -n "$stale" ]; then
  echo "ERROR: listed in tools/check-no-first-party-hs256.sh but no longer matching:"
  printf '%s' "$stale"
  echo "Remove the entries, so the files are gated again."
  exit 1
fi

# Matching nothing at all means the pattern or the file walk broke, not that the tree is
# clean: the exempt files match for good.
if [ "$matched" -eq 0 ]; then
  echo "ERROR: the gate matched no file at all; its pattern or file walk is broken."
  exit 1
fi

pending=$(printf '%s' "$PENDING" | grep -c . || true)
echo "OK: no first-party HS256 code outside the gate's lists (${pending} pending removal)."
