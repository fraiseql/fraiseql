#!/usr/bin/env bash
# principal_producers_test.sh — the red capability of tools/check-principal-producers.sh.
#
# The gate passes over the tree it was written against, so green is equally consistent
# with "it matches nothing". Each rule is proved on a synthetic tree holding the seams the
# gate's non-vacuity checks read and one producer for each allowlist entry: an unresolved
# producer goes RED, a resolving one stays GREEN, a recognised resolver that stops running
# the seam goes RED, and an allowlist entry that starts resolving goes RED.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-principal-producers.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
ws="${tmp}/ws"
srv="crates/fraiseql-server/src"

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

put() { mkdir -p "$(dirname "${ws}/$1")"; printf '%s\n' "$2" > "${ws}/$1"; }

# A tree every check of the gate passes on.
tree() {
  rm -rf "$ws"
  put "${srv}/identity/mod.rs" 'pub async fn resolve_request_identity() {}'
  put "crates/fraiseql-arrow/src/service.rs" 'fn resolve_identity() {}'
  put "${srv}/admission/mod.rs" $'pub fn identify() {\n    enrich_identity();\n}\nfn enrich_identity() {\n    resolve_request_identity();\n}'
  put "${srv}/routes/graphql/handler/stages.rs" $'pub fn admit() {\n    admission::identify();\n}'
  put "${srv}/routes/api/schema.rs" $'pub(crate) async fn resolved_caller() {\n    resolve_request_identity();\n}'
  for deferred in extractors.rs api_key/mod.rs service_account.rs routes/api/admin_sql.rs \
    routes/api/tenant_admin.rs observers/handlers.rs observers/scope.rs; do
    put "${srv}/${deferred}" 'fn f(c: OptionalSecurityContext) {}'
  done
}

expect() {
  local want="$1" needle="$2" name="$3"
  PRINCIPAL_PRODUCERS_ROOT="$ws" bash "$gate" > "${tmp}/out" 2>&1
  local rc=$?
  if [ "$rc" -ne "$want" ]; then
    fail "${name} — rc=${rc} (want ${want})"
  elif ! grep -qF -- "$needle" "${tmp}/out"; then
    fail "${name} — output did not contain '${needle}'"
  else
    pass "$name"
  fi
}

echo "principal producers gate:"

tree
expect 0 "OK: every principal producer resolves" "1. a tree whose producers all resolve passes"

tree; put "${srv}/routes/api/metadata.rs" 'pub async fn h(OptionalSecurityContext(c): OptionalSecurityContext) {}'
expect 1 "${srv}/routes/api/metadata.rs" "2. a producer that never resolves fails, by file"

tree; put "${srv}/routes/api/metadata.rs" $'pub async fn h(OptionalSecurityContext(c): OptionalSecurityContext) {\n    resolved_caller(c).await;\n}'
expect 0 "OK: every principal producer resolves" "3. a producer resolving through resolved_caller passes"

tree; put "${srv}/routes/api/schema.rs" $'pub(crate) async fn resolved_caller() {\n    Ok(caller)\n}'
expect 1 "resolved_caller in ${srv}/routes/api/schema.rs no longer calls resolve_request_identity" \
  "4. resolved_caller that stops running the seam fails"

tree; put "${srv}/routes/api/tenant_admin.rs" $'fn f(c: OptionalSecurityContext) {\n    resolved_caller(c);\n}'
expect 1 "allowlist entry ${srv}/routes/api/tenant_admin.rs now resolves its principal" \
  "5. a DEFERRED entry that starts resolving fails"

tree; put "${srv}/routes/api/tenant_admin.rs" $'fn f(c: OptionalSecurityContext) {}\n#[cfg(test)]\nmod tests {\n    fn t() { resolved_caller(c); }\n}'
expect 0 "OK: every principal producer resolves" "6. a resolver call in an entry's test module does not count"

tree; rm "${ws}/${srv}/observers/scope.rs"
expect 1 "allowlist entry ${srv}/observers/scope.rs does not exist" "7. a missing allowlist entry fails"

if [ "$failures" -ne 0 ]; then
  echo "principal producers gate: ${failures} case(s) FAILED"
  exit 1
fi
echo "principal producers gate: all cases pass"
