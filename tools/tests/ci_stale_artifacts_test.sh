#!/usr/bin/env bash
# ci_stale_artifacts_test.sh — the red capability of tools/ci-stale-artifacts.sh (#1421).
#
# The check's job is to fail on a build that linked a workspace artifact from before the
# leg's source stamp. A green CI run cannot show it would: green is equally what a check
# that matches nothing prints. So each branch is driven here with a synthetic cargo JSON
# log and artifact files whose mtimes are set on purpose:
#
#   * a cached workspace artifact older than the stamp is STALE        → exit 1, named
#   * a cached workspace artifact written after the stamp (an earlier
#     suite of the same leg)                                           → exit 0
#   * a freshly compiled unit ("fresh":false) is never judged          → exit 0
#   * a registry dependency, however old, is never judged             → exit 0
#   * a log line for a package outside the workspace root is ignored, even when its path
#     shares the root as a string prefix (/src vs /srcx)               → exit 0
#   * one stale file among several in one unit's `filenames` is found → exit 1
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
check="${repo_root}/tools/ci-stale-artifacts.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

stamp=1700000000
old="@$((stamp - 3600))"
new="@$((stamp + 3600))"
root="${tmp}/src"
mkdir -p "${root}/target/debug/deps"

artifact() { # artifact <name> <mtime> → path
  local p="${root}/target/debug/deps/$1"
  : > "$p"
  touch -d "$2" "$p"
  echo "$p"
}

line() { # line <package_id> <fresh> <file...>
  local pkg="$1" fresh="$2"
  shift 2
  local files
  files="$(printf '"%s",' "$@")"
  printf '{"reason":"compiler-artifact","package_id":"%s","fresh":%s,"filenames":[%s]}\n' \
    "${pkg}" "${fresh}" "${files%,}"
}

run() { # run <log> → rc in $tmp/rc, output in $tmp/out
  bash "${check}" "$1" "${stamp}" "${root}" > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

expect() { # expect <rc> <substring-or-empty> <label>
  local rc
  rc="$(cat "${tmp}/rc")"
  if [ "${rc}" = "$1" ] && { [ -z "$2" ] || grep -F "$2" "${tmp}/out" >/dev/null; }; then
    pass "$3"
  else
    fail "$3 (rc=${rc})"
  fi
}

ws="path+file://${root}/crates/fraiseql-core#2.15.0"

stale_core="$(artifact libfraiseql_core-stale.rlib "${old}")"
line "${ws}" true "${stale_core}" > "${tmp}/log1"
run "${tmp}/log1"
expect 1 "libfraiseql_core-stale.rlib" "a cached workspace artifact older than the stamp is stale"

fresh_core="$(artifact libfraiseql_core-new.rlib "${new}")"
line "${ws}" true "${fresh_core}" > "${tmp}/log2"
run "${tmp}/log2"
expect 0 "" "a cached workspace artifact written after the stamp is this leg's own"

line "${ws}" false "${stale_core}" > "${tmp}/log3"
run "${tmp}/log3"
expect 0 "" "a freshly compiled unit is not judged"

old_dep="$(artifact libserde-old.rlib "${old}")"
line "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.228" true "${old_dep}" > "${tmp}/log4"
run "${tmp}/log4"
expect 0 "" "a registry dependency is not judged"

line "path+file://${root}x/crates/other#0.1.0" true "${stale_core}" > "${tmp}/log5"
run "${tmp}/log5"
expect 0 "" "a package outside the workspace root (string-prefix sibling) is ignored"

line "${ws}" true "${fresh_core}" "${stale_core}" > "${tmp}/log6"
run "${tmp}/log6"
expect 1 "libfraiseql_core-stale.rlib" "one stale file among a unit's filenames is found"

if [ "${failures}" -ne 0 ]; then
  echo "ci_stale_artifacts_test: ${failures} case(s) failed"
  exit 1
fi
echo "ci_stale_artifacts_test: all cases passed"
