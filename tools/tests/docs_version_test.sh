#!/usr/bin/env bash
# docs_version_test.sh — the red capability of tools/check-docs-version.sh.
#
# Each case is a tree with a workspace version, a docs page, README.md's install snippet and
# the committed fraiseql.toml JSON Schema. One of them names another version (or is missing),
# and the gate must fail, naming it.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-docs-version.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

URL='https://raw.githubusercontent.com/fraiseql/fraiseql/v2.19.1/docs/schemas/fraiseql.toml.schema.json'

# new_case <n>: a tree every check passes on.
new_case() {
  root="${tmp}/case$1"
  mkdir -p "${root}/docs/schemas"
  printf '[workspace]\n\n[workspace.package]\nversion = "2.19.1"\n' > "${root}/Cargo.toml"
  printf '# FraiseQL\n\n```toml\nfraiseql = { version = "2.19.1" }\n```\n' > "${root}/README.md"
  printf '# Status\n\nv2.19.1 released.\n\nSchema: %s\n' "$URL" > "${root}/docs/status.md"
  printf '{\n  "$schema": "https://json-schema.org/draft/2020-12/schema",\n  "$id": "%s",\n  "title": "fraiseql.toml"\n}\n' \
    "$URL" > "${root}/docs/schemas/fraiseql.toml.schema.json"
}

run_gate() {
  DOCS_VERSION_ROOT="$root" bash "$gate" > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

expect_green() {
  if [ "$(cat "${tmp}/rc")" = 0 ]; then pass "$1"; else fail "$1"; fi
}

expect_red() { # expect_red <text that must be printed> <label>
  if [ "$(cat "${tmp}/rc")" != 0 ] && grep -F -- "$1" "${tmp}/out" >/dev/null; then
    pass "$2"
  else
    fail "$2"
  fi
}

new_case 1
run_gate
expect_green "every version names the workspace's"

new_case 2
sed -i 's/v2.19.1 released/v2.18.0 released/' "${root}/docs/status.md"
run_gate
expect_red "v2.18.0 released" "a status line naming another release"

new_case 3
sed -i 's/version = "2.19.1" }/version = "2.8" }/' "${root}/README.md"
run_gate
expect_red "README.md pins fraiseql 2.8" "an install snippet pinning another version"

new_case 4
sed -i 's|fraiseql/v2.19.1/docs/schemas|fraiseql/v2.18.0/docs/schemas|' \
  "${root}/docs/schemas/fraiseql.toml.schema.json"
run_gate
expect_red "\$id names v2.18.0" "a schema \$id naming another version"

new_case 5
rm "${root}/docs/schemas/fraiseql.toml.schema.json"
run_gate
expect_red "fraiseql.toml.schema.json is missing" "a missing schema file"

new_case 6
sed -i 's|fraiseql/v2.19.1/docs/schemas|fraiseql/v2.18.0/docs/schemas|' "${root}/docs/status.md"
run_gate
expect_red "docs/status.md:5:https://raw.githubusercontent.com/fraiseql/fraiseql/v2.18.0" \
  "a documented schema URL naming another version"

new_case 7
sed -i '/^Schema:/d' "${root}/docs/status.md"
run_gate
expect_red "no page in docs/ gives the fraiseql.toml JSON Schema URL" "no page giving the schema URL"

new_case 8
printf 'Taplo: %s\n' "$(echo "$URL" | sed 's/v2.19.1/v2.18.0/')" >> "${root}/README.md"
run_gate
expect_red "README.md:" "a schema URL in README.md naming another version"

echo
if [ "$failures" -ne 0 ]; then
  echo "docs_version_test: ${failures} case(s) failed"
  exit 1
fi
echo "docs_version_test: all cases passed"
