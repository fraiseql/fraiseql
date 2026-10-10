#!/usr/bin/env bash
# Self-test for tools/check-dagger-apt.sh: it fails on each form it exists to refuse, passes
# the helper form, and fails loudly when it can see nothing.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
gate="$here/check-dagger-apt.sh"
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
fails=0
case_rc() { # name expected-rc go-source
  local dir="$work/$1"; mkdir -p "$dir/.dagger"
  [ -n "$3" ] && printf '%s\n' "$3" >"$dir/.dagger/main.go"
  DAGGER_APT_ROOT="$dir" bash "$gate" >/dev/null 2>&1; local rc=$?
  if [ "$rc" -eq "$2" ]; then echo "  ok   $1"; else echo "  FAIL $1 (rc $rc, want $2)"; fails=$((fails+1)); fi
}
case_rc "a separate update exec fails" 1 'c.WithExec([]string{"apt-get", "update"})'
case_rc "an install exec on a cached index fails" 1 'c.WithExec([]string{
	"apt-get", "install", "-y", "r-base-core",
})'
case_rc "the helper passes" 0 'c.WithExec(aptInstall("r-base-core"))'
case_rc "a module the gate cannot see fails" 2 ''
echo "dagger apt gate: 4 cases, $fails failed"
[ "$fails" -eq 0 ]
