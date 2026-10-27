#!/usr/bin/env bash
# not_implemented_test.sh — the red capability of tools/check-not-implemented.sh.
#
# A passing run of the real tree cannot tell "one allowed call" from "a scan that reads
# nothing", so every branch is proved here on synthetic trees.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-not-implemented.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

routes=crates/fraiseql-server/src/routes

# A tree with the helper's definition and the one allowed call.
tree() {
  local root="${tmp}/$1"
  mkdir -p "${root}/${routes}/studio" "${root}/${routes}/rest"
  printf 'pub(crate) fn not_implemented(what: &str, detail: &str) -> Response {\n    todo()\n}\n' \
    > "${root}/${routes}/studio/mod.rs"
  printf 'fn revoke() -> Response {\n    return not_implemented(\n        "x", "y",\n    );\n}\n' \
    > "${root}/${routes}/studio/auth_users.rs"
  printf 'fn read() {}\n' > "${root}/${routes}/rest/handler.rs"
  echo "$root"
}

expect() { # name rc grep-text root
  NOT_IMPLEMENTED_ROOT="$4" bash "$gate" > "${tmp}/out" 2>&1
  local rc=$?
  if [ "$rc" = "$2" ] && grep -F -- "$3" "${tmp}/out" >/dev/null; then pass "$1"; else fail "$1 (rc $rc)"; fi
}

root=$(tree clean)
expect "the allowed call alone passes" 0 "OK: not_implemented(" "$root"

root=$(tree elsewhere)
printf 'fn read() -> Response {\n    not_implemented("rest.read", "later")\n}\n' > "${root}/${routes}/rest/handler.rs"
expect "a call in another route fails, by file and line" 1 "${routes}/rest/handler.rs:2:" "$root"

root=$(tree second)
printf 'fn other() -> Response {\n    not_implemented("x", "y")\n}\n' >> "${root}/${routes}/studio/auth_users.rs"
expect "a second call beside the allowed one fails" 1 "auth_users.rs answers 501 through not_implemented( 2 time(s), allowed 1" "$root"

root=$(tree removed)
printf 'fn revoke() -> Response {\n    ok()\n}\n' > "${root}/${routes}/studio/auth_users.rs"
expect "the allowed call removed leaves a stale allowance, which fails" 1 "the allowance only shrinks" "$root"

root=$(tree alias)
printf 'use super::not_implemented as decline;\nfn read() -> Response { decline("x", "y") }\n' > "${root}/${routes}/rest/handler.rs"
expect "an aliased import fails" 1 "imported under another name" "$root"

root=$(tree comment)
printf '// not_implemented("x", "y") is what this used to answer\nfn read() {}\n' > "${root}/${routes}/rest/handler.rs"
expect "a comment naming the call is not a call" 0 "OK: not_implemented(" "$root"

root=$(tree moved)
printf 'fn decline() {}\n' > "${root}/${routes}/studio/mod.rs"
expect "no definition to anchor on exits 2, not 0" 2 "the helper moved or was renamed" "$root"

if [ "$failures" -ne 0 ]; then
  echo "not_implemented_test: ${failures} failure(s)"
  exit 1
fi
echo "not_implemented_test: all cases pass"
