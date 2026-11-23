#!/usr/bin/env bash
# mount_ledger_test.sh — the red capability of tools/check-mount-ledger.py.
#
# The gate passes over the tree it was written against, so green is equally consistent
# with "the scan matches nothing". Every branch is proved on a synthetic tree: each
# unrecorded mount shape goes RED at its own position, the legitimate shapes stay GREEN,
# and a scan that finds no ledger call fails rather than reporting a clean tree.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-mount-ledger.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

routing="crates/fraiseql-server/src/server/routing"
capabilities="crates/fraiseql-server/src/server/capabilities"

# A fresh tree holding one recorded mount in routing, plus `$2` written to `$1`.
tree() {
  rm -rf "${tmp}/ws"
  mkdir -p "${tmp}/ws/${routing}" "${tmp}/ws/${capabilities}"
  cat > "${tmp}/ws/${routing}/mod.rs" <<'EOF'
fn mount(&self, app: Router) -> Router {
    self.mounts.merge(app, Mount::public("health"), health())
}
EOF
  if [ -n "${1:-}" ]; then
    mkdir -p "$(dirname "${tmp}/ws/$1")"
    printf '%s\n' "$2" > "${tmp}/ws/$1"
  fi
}

# Run the gate on the tree; expect exit `$1`, and output containing `$2` when given.
expect() {
  local want="$1" needle="${2:-}" name="$3"
  MOUNT_LEDGER_ROOT="${tmp}/ws" python3 "$gate" > "${tmp}/out" 2>&1
  local rc=$?
  if [ "$rc" -ne "$want" ]; then
    fail "${name} — rc=${rc} (want ${want})"
  elif [ -n "$needle" ] && ! grep -qF -- "$needle" "${tmp}/out"; then
    fail "${name} — output did not contain '${needle}'"
  else
    pass "$name"
  fi
}

echo "mount ledger gate:"

tree
expect 0 "1 mount site(s) recorded" "1. a recorded mount passes"

tree "${routing}/admin.rs" $'fn f(&self, app: Router) -> Router {\n    app.merge(admin())\n}'
expect 1 "${routing}/admin.rs:2:8: \`.merge(\`" "2. an unrecorded merge fails at its position"

tree "${routing}/observers.rs" $'fn f(app: Router) -> Router {\n    app\n        .nest("/api", routes())\n}'
expect 1 "${routing}/observers.rs:3:9: \`.nest(\`" "3. a nest on its own line fails at its position"

tree "${capabilities}/mcp.rs" $'fn f(app: Router) -> Router {\n    app.nest_service("/mcp", service())\n}'
expect 1 "${capabilities}/mcp.rs:2:8: \`.nest_service(\`" "4. a nest_service in a capability fails"

tree "${routing}/two.rs" $'fn f(&self, app: Router) -> Router {\n    let app = self.mounts.merge(app, m(), a());\n    app.merge(b())\n}'
expect 1 "${routing}/two.rs:3:8" "5. a recorded mount does not excuse an unrecorded one beside it"

tree "${routing}/doc.rs" $'// app.merge(router) is how it used to be done\n/* app.nest("/x", r) */\nfn f() {}'
expect 0 "" "6. a commented-out mount passes"

tree "${routing}/admin_tests.rs" $'fn t(app: Router) -> Router { app.merge(r()) }'
expect 0 "" "7. a test file is not scanned"

tree "${routing}/free.rs" $'fn f(ledger: &MountLedger, app: Router) -> Router {\n    ledger.nest(app, m(), "/o", r())\n}'
expect 0 "2 mount site(s) recorded" "8. a ledger passed to a free function passes"

tree "${routing}/string.rs" $'fn f() -> &\'static str { "app.merge(" }'
expect 0 "" "9. a mount call inside a string literal passes"

tree "${routing}/escape.rs" $'fn f(app: Router) -> Router {\n    let _ = "a\\"b";\n    app.merge(r())\n}'
expect 1 "${routing}/escape.rs:3:8" "12. an escaped quote does not hide the mount after it"

tree "${routing}/quote.rs" $'fn f(app: Router) -> Router {\n    let _ = \'"\';\n    app.merge(r())\n}'
expect 1 "${routing}/quote.rs:3:8" "13. a quote character does not hide the mount after it"

rm -rf "${tmp}/ws"; mkdir -p "${tmp}/ws/${routing}"
printf 'fn f() {}\n' > "${tmp}/ws/${routing}/mod.rs"
expect 1 "found 0 ledger call(s)" "10. a scan with no ledger call fails"

rm -rf "${tmp}/ws"; mkdir -p "${tmp}/ws"
expect 1 "scanned 0 file(s)" "11. a scan of the wrong tree fails"

if [ "$failures" -ne 0 ]; then
  echo "mount ledger gate: ${failures} case(s) FAILED"
  exit 1
fi
echo "mount ledger gate: all cases pass"
