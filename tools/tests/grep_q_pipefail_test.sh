#!/usr/bin/env bash
# Red-capability pin for tools/check-grep-q-pipefail.py.
#
# Run directly:  bash tools/tests/grep_q_pipefail_test.sh
# Exits non-zero if any assertion fails.
#
# Each case is a fixture tree with no `.git`, as the gate's CI container mounts the
# source, holding one script, workflow, Makefile or Dagger file. The cases are the ways
# the check could miss a site (an option after a quoted pattern with a `|`, `--quiet`, a
# sourced file, a function defined above the `set`, each place a shell runs) and the
# ways it could refuse a line that is fine (`|| grep -q`, a comment, no `pipefail`, the
# `grep -c … >/dev/null` rewrite).
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-grep-q-pipefail.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# new_case <id>: a tree with one harmless script, so the scan always reads a script.
new_case() {
    root="$WORK/$1"
    mkdir -p "$root/tools"
    printf '#!/usr/bin/env bash\nset -euo pipefail\necho ok\n' >"$root/tools/base.sh"
}

# expect <rc> <text the output must contain, or ""> <description>
expect() {
    local want_rc="$1" want_text="$2" desc="$3" out rc
    TESTS_RUN=$((TESTS_RUN + 1))
    out="$(GREP_Q_PIPEFAIL_ROOT="$root" python3 "$GATE" 2>&1)"
    rc=$?
    if [ "$rc" = "$want_rc" ] && { [ -z "$want_text" ] || grep -cF -- "$want_text" >/dev/null <<<"$out"; }; then
        printf '  ✅ %s\n' "$desc"
    else
        printf '  ❌ %s (rc=%s, want %s, %s)\n' "$desc" "$rc" "$want_rc" "${want_text:-no text}"
        sed 's/^/        /' <<<"$out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

# not_named <text> <description>: the check's output does not name <text>. The output is
# captured first: under `pipefail`, `gate | grep` takes the gate's exit 1 whatever grep
# finds, and the assertion would pass vacuously.
not_named() {
    local text="$1" desc="$2" out
    TESTS_RUN=$((TESTS_RUN + 1))
    out="$(GREP_Q_PIPEFAIL_ROOT="$root" python3 "$GATE" 2>&1)"
    if grep -cF -- "$text" >/dev/null <<<"$out"; then
        printf '  ❌ %s\n' "$desc"
        sed 's/^/        /' <<<"$out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        printf '  ✅ %s\n' "$desc"
    fi
}

# script <id> <body>: tools/s.sh with the body after a bash shebang.
script() {
    new_case "$1"
    printf '#!/usr/bin/env bash\n%s\n' "$2" >"$root/tools/s.sh"
}

echo "check-grep-q-pipefail.py"

script piped 'set -euo pipefail
if printf "%s" "$out" | grep -qF needle; then echo hit; fi'
expect 1 "tools/s.sh:3:" "printf | grep -q under pipefail is refused, naming its line"

script fixed 'set -euo pipefail
if printf "%s" "$out" | grep -cF needle >/dev/null; then echo hit; fi'
expect 0 "" "the grep -c … >/dev/null rewrite passes"

script nopf 'set -eu
if printf "%s" "$out" | grep -qF needle; then echo hit; fi'
expect 0 "" "a script without pipefail is not checked"

script pfcomment '# Without `set -o pipefail` here, the pipe below is safe.
set -eu
if printf "%s" "$out" | grep -qF needle; then echo hit; fi'
expect 0 "" "a comment naming pipefail does not turn it on"

script late 'check() { printf "%s" "$1" | grep -q needle; }
set -o pipefail
check x'
expect 1 "tools/s.sh:2:" "a function defined above the set still runs under it"

script after 'set -euo pipefail
if printf "%s" "$out" | grep -E "a|b" -q; then echo hit; fi'
expect 1 "tools/s.sh:3:" "-q after a quoted pattern holding a | is still seen"

script long 'set -euo pipefail
cmd | grep --quiet needle'
expect 1 "tools/s.sh:3:" "--quiet is refused"

script orgrep 'set -euo pipefail
[ -f x ] || grep -q needle file'
expect 0 "" "|| grep -q reads a file, no pipe: not refused"

script comment 'set -euo pipefail
# never write: printf "%s" "$x" | grep -q needle'
expect 0 "" "a comment is not a site"

new_case sourced
printf '#!/usr/bin/env bash\nset -euo pipefail\nsource "$(dirname "$0")/lib/helpers.sh"\n' >"$root/tools/caller.sh"
mkdir -p "$root/tools/lib"
printf 'has() { printf "%%s" "$1" | grep -qF "$2"; }\n' >"$root/tools/lib/helpers.sh"
expect 1 "tools/lib/helpers.sh:1:" "a file a pipefail script sources is checked"

new_case unsourced
mkdir -p "$root/tools/lib"
printf 'has() { printf "%%s" "$1" | grep -qF "$2"; }\n' >"$root/tools/lib/helpers.sh"
expect 0 "" "the same file sourced by nothing under pipefail is not"

new_case wfbash
mkdir -p "$root/.github/workflows"
cat >"$root/.github/workflows/w.yml" <<'YML'
name: W
on: push
jobs:
  j:
    runs-on: ubuntu-latest
    steps:
      - name: default shell
        run: |
          echo "$x" | grep -q fine
      - name: bash
        shell: bash
        run: |
          echo "$x" | grep -q refused
YML
expect 1 ".github/workflows/w.yml:13:" "a workflow step with shell: bash is refused"
not_named "w.yml:9:" "a default-shell step (bash -e, no pipefail) is not"

new_case wfset
mkdir -p "$root/.github/workflows"
cat >"$root/.github/workflows/w.yml" <<'YML'
name: W
on: push
jobs:
  j:
    runs-on: ubuntu-latest
    steps:
      - run: |
          set -o pipefail
          echo "$x" | grep -q refused
YML
expect 1 ".github/workflows/w.yml:9:" "a default-shell step that sets pipefail is refused"

new_case make
printf 'a:\n\tset -o pipefail; cmd | grep -q x\nb:\n\tcmd | grep -q y\n' >"$root/Makefile"
expect 1 "Makefile:2:" "a Makefile recipe line that sets pipefail is refused"
not_named "Makefile:4:" "a recipe line without it (sh -c) is not"

new_case dagger
mkdir -p "$root/.dagger"
printf 'package main\n\nvar s = "cargo metadata | grep -q fraiseql"\n' >"$root/.dagger/main.go"
expect 1 ".dagger/main.go:3:" "a .dagger script line is refused"

# This pin's own fixtures are refused lines held as data. The check skips this file by
# its exact path, and only it: the same lines in a sibling file are still refused.
new_case selfpin
mkdir -p "$root/tools/tests"
printf '#!/usr/bin/env bash\nset -euo pipefail\nprintf x | grep -q x\n' >"$root/tools/tests/grep_q_pipefail_test.sh"
printf '#!/usr/bin/env bash\nset -euo pipefail\nprintf x | grep -q x\n' >"$root/tools/tests/other_test.sh"
expect 1 "tools/tests/other_test.sh:3:" "a sibling of this pin holding the same line is refused"
not_named "grep_q_pipefail_test.sh" "this pin's own path is skipped"

root="$WORK/empty"
mkdir -p "$root"
expect 2 "read no shell script" "a tree with no shell script is an error"

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "❌ $TESTS_FAILED of $TESTS_RUN assertions failed"
    exit 1
fi
echo "✅ $TESTS_RUN assertions passed"
