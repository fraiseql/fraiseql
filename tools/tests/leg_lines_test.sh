#!/usr/bin/env bash
# Red-capability pin for tools/leg-lines.py, the reader `make test-leg` and
# `make test-integration-postgres` run their lines through.
#
# Each case plants one shape into a copy of .dagger/main.go and expects the reader
# to fail with a named message: a mirror that silently runs less than the shard is
# the defect the reader exists to prevent. The last cases run `make test-leg` over
# the real lines with a stub cargo, so a recipe that dropped a line, or carried on
# past a failing one, fails here too.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
READER="$repo_root/tools/leg-lines.py"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
failed=0
cases=0

case_dir() {
    local d="$work/$1"
    mkdir -p "$d/.dagger"
    cp "$repo_root/.dagger/main.go" "$d/.dagger/main.go"
    printf '%s\n' "$d"
}

# A plant must change the file; a sed that matched nothing would make the case a
# second baseline.
planted() {
    if cmp -s "$repo_root/.dagger/main.go" "$1/.dagger/main.go"; then
        echo "FAIL: $2: the plant changed nothing"
        failed=$((failed + 1))
        return 1
    fi
}

expect_green() {
    local name="$1" d="$2" shard="$3"
    cases=$((cases + 1))
    if LEG_LINES_ROOT="$d" python3 "$READER" "$shard" >"$work/out" 2>&1; then
        echo "ok: $name"
    else
        echo "FAIL: $name: expected green"
        sed 's/^/    /' "$work/out"
        failed=$((failed + 1))
    fi
}

expect_red() {
    local name="$1" d="$2" shard="$3" want="$4"
    cases=$((cases + 1))
    planted "$d" "$name" || return
    if LEG_LINES_ROOT="$d" python3 "$READER" "$shard" >"$work/out" 2>&1; then
        echo "FAIL: $name: expected red, got green"
        failed=$((failed + 1))
    elif ! grep -qF -- "$want" "$work/out"; then
        echo "FAIL: $name: red, but without: $want"
        sed 's/^/    /' "$work/out"
        failed=$((failed + 1))
    else
        echo "ok: $name"
    fi
}

# --- green --------------------------------------------------------------------
d="$(case_dir baseline)"
expect_green "Test reads" "$d" Test
expect_green "integrationPostgres reads" "$d" integrationPostgres

cases=$((cases + 1))
LEG_LINES_ROOT="$d" python3 "$READER" Test >"$work/test.lines"
LEG_LINES_ROOT="$d" python3 "$READER" integrationPostgres >"$work/pg.lines"
if grep -qx 'set -e' "$work/test.lines" ||
    grep -q 'ci-target-canary' "$work/test.lines" "$work/pg.lines" ||
    grep -q '/tmp/uv\|/tmp/confiture' "$work/pg.lines" ||
    ! grep -q '^cargo build ' "$work/test.lines"; then
    echo "FAIL: the CI-only lines are dropped and the canary build is unwrapped"
    failed=$((failed + 1))
else
    echo "ok: the CI-only lines are dropped and the canary build is unwrapped"
fi

d="$(case_dir echo-local)"
sed -i '0,/^\t\t"echo \x27### cargo build --all-features\x27",$/s//\t\t"echo \x27### build " + someLocal + "\x27",/' "$d/.dagger/main.go"
cases=$((cases + 1))
if planted "$d" "an unresolvable piece in an echo line is only printed" &&
    LEG_LINES_ROOT="$d" python3 "$READER" Test >"$work/out" 2>&1 &&
    grep -qx "echo '### build <someLocal>'" "$work/out"; then
    echo "ok: an unresolvable piece in an echo line is only printed"
else
    echo "FAIL: an unresolvable piece in an echo line is only printed"
    failed=$((failed + 1))
fi

# --- red ----------------------------------------------------------------------
d="$(case_dir no-func)"
sed -i 's/^func (m \*FraiseqlCi) Test(/func (m *FraiseqlCi) TestRenamed(/' "$d/.dagger/main.go"
expect_red "the shard function is gone" "$d" Test 'no `Test` function'

d="$(case_dir no-script)"
python3 - "$d/.dagger/main.go" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
head = re.search(r"^func \(m \*FraiseqlCi\) integrationPostgres\(", s, re.M).start()
i = s.index("script := strings.Join([]string{", head)
s = s[:i] + "script := strings.Join(append([]string{}, []string{" + s[i + len("script := strings.Join([]string{"):]
open(p, "w").write(s)
PY
expect_red "the script literal changed shape" "$d" integrationPostgres \
    '`integrationPostgres` has no `script := strings.Join'

d="$(case_dir cargo-local)"
sed -i '0,/^\t\t"cargo test -p fraiseql-saga --lib",$/s//\t\t"cargo test -p fraiseql-saga " + sagaArgs,/' "$d/.dagger/main.go"
expect_red "a cargo line built from a piece the reader cannot resolve" "$d" Test \
    'builds a line from `sagaArgs`'

d="$(case_dir canary-other)"
sed -i '0,/"bash tools\/ci-target-canary.sh -- build /s//"bash tools\/ci-target-canary.sh -- check /' "$d/.dagger/main.go"
expect_red "a canary line of neither form" "$d" Test 'a canary line of neither form'

d="$(case_dir outside-script)"
python3 - "$d/.dagger/main.go" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
head = re.search(r"^func \(m \*FraiseqlCi\) Test\(", s, re.M).start()
i = s.index("script := strings.Join([]string{", head)
s = s[:i] + 'extra := "cargo test -p fraiseql-planted --lib"\n\t_ = extra\n\t' + s[i:]
open(p, "w").write(s)
PY
expect_red "a cargo test line the shard runs outside its script" "$d" Test \
    'read different cargo test lines'

d="$(case_dir blind)"
python3 - "$d/.dagger/main.go" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
head = re.search(r"^func \(m \*FraiseqlCi\) Test\(", s, re.M).start()
end = re.search(r"^}", s[head:], re.M).end() + head
s = s[:head] + s[head:end].replace('"cargo test ', '"cargo run ') + s[end:]
open(p, "w").write(s)
PY
expect_red "a shard with no cargo test line" "$d" Test 'no cargo test line read'

# --- the make targets run every line, and stop at the first failure ---------------
stub="$work/stub"
mkdir -p "$stub"
cat >"$stub/cargo" <<'SH'
#!/usr/bin/env bash
echo "$*" >>"$CARGO_LOG"
n="$(wc -l <"$CARGO_LOG")"
[ "$n" != "${CARGO_FAIL_AT:-0}" ]
SH
printf '#!/bin/sh\necho stub-rustc\n' >"$stub/rustc"
chmod +x "$stub/cargo" "$stub/rustc"
want="$(grep -c '^cargo ' "$work/test.lines")"

run_leg() {
    : >"$work/cargo.log"
    env -u DATABASE_URL -u TEST_DATABASE_URL -u REDIS_URL -u NATS_URL \
        PATH="$stub:$PATH" CARGO_LOG="$work/cargo.log" CARGO_FAIL_AT="$1" \
        LEG_LINES_ROOT="${2:-$repo_root}" \
        make -C "$repo_root" --no-print-directory test-leg >"$work/make.out" 2>&1
}

cases=$((cases + 1))
if run_leg 0 && [ "$(wc -l <"$work/cargo.log")" = "$want" ]; then
    echo "ok: make test-leg runs each of the $want cargo lines"
else
    echo "FAIL: make test-leg ran $(wc -l <"$work/cargo.log") of $want cargo lines"
    tail -5 "$work/make.out" | sed 's/^/    /'
    failed=$((failed + 1))
fi

cases=$((cases + 1))
if ! run_leg 3 && [ "$(wc -l <"$work/cargo.log")" = 3 ]; then
    echo "ok: make test-leg fails at a failing line and runs nothing after it"
else
    echo "FAIL: make test-leg with the third cargo line failing: rc ok or $(wc -l <"$work/cargo.log") lines run"
    failed=$((failed + 1))
fi

# test-integration-postgres needs a database, so its line is held to test-leg's shape.
cases=$((cases + 1))
pg_line="$(printf '\t@script="$$(python3 tools/leg-lines.py integrationPostgres)" && bash -ec "$$script"')"
if grep -qxF -- "$pg_line" "$repo_root/Makefile" &&
    [ "$(grep -c 'leg-lines.py integrationPostgres)' "$repo_root/Makefile")" = 1 ]; then
    echo "ok: test-integration-postgres runs its lines the way test-leg does"
else
    echo "FAIL: test-integration-postgres does not run: $pg_line"
    failed=$((failed + 1))
fi

cases=$((cases + 1))
if ! run_leg 0 "$work/no-func" && [ ! -s "$work/cargo.log" ] &&
    grep -qF 'no `Test` function' "$work/make.out"; then
    echo "ok: make test-leg fails when the reader cannot read the shard, and runs nothing"
else
    echo "FAIL: make test-leg over an unreadable shard: rc ok or cargo ran"
    failed=$((failed + 1))
fi

echo
echo "$cases cases, $failed failed"
if [ "$failed" -ne 0 ]; then
    exit 1
fi
echo "OK: tools/leg-lines.py refuses every shape it cannot read, and the mirror runs every line."
