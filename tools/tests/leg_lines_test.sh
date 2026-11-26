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

# plant_in_test <dir> <old> <new>: replace the first <old> inside the body of `Test`, the
# function the reader reads for the `Test` shard. Located by the function, not by the
# first match in the file: a line of the same text in a function above `Test` would take
# the plant into code the reader never reads, and the case would pass on nothing. An
# <old> that `Test` does not hold fails the case through `planted`.
plant_in_test() {
    python3 - "$1/.dagger/main.go" "$2" "$3" <<'PY'
import re, sys
path, old, new = sys.argv[1:]
s = open(path).read()
head = re.search(r"^func \(m \*FraiseqlCi\) Test\(", s, re.M)
if head is None:
    sys.exit(0)
end = re.search(r"^func ", s[head.end():], re.M)
stop = head.end() + end.start() if end else len(s)
i = s.find(old, head.start(), stop)
if i < 0:
    sys.exit(0)
open(path, "w").write(s[:i] + new + s[i + len(old):])
PY
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
plant_in_test "$d" $'\n\t\t"echo \'### cargo build --all-features\'",\n' \
    $'\n\t\t"echo \'### build " + someLocal + "\'",\n'
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
plant_in_test "$d" $'\n\t\t"cargo test -p fraiseql-saga --lib",\n' \
    $'\n\t\t"cargo test -p fraiseql-saga " + sagaArgs,\n'
expect_red "a cargo line built from a piece the reader cannot resolve" "$d" Test \
    'builds a line from `sagaArgs`'

d="$(case_dir canary-other)"
plant_in_test "$d" '"bash tools/ci-target-canary.sh -- build ' '"bash tools/ci-target-canary.sh -- check '
expect_red "a canary line of neither form" "$d" Test 'a canary line of neither form'

# The same plant with a canary build line in a function above `Test`: located by first
# match, the plant would land in that function, which the reader never reads, and the
# case above would pass without testing anything.
d="$(case_dir canary-other-decoy)"
python3 - "$d/.dagger/main.go" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
i = re.search(r"^func \(m \*FraiseqlCi\) Test\(", s, re.M).start()
decoy = (
    "func (m *FraiseqlCi) decoyAboveTest() []string {\n"
    "\treturn []string{\n"
    "\t\t\"bash tools/ci-target-canary.sh -- build -p decoy\",\n"
    "\t}\n"
    "}\n\n"
)
open(p, "w").write(s[:i] + decoy + s[i:])
PY
decoy_at=$(grep -nF 'ci-target-canary.sh -- build -p decoy' "$d/.dagger/main.go" | cut -d: -f1)
test_at=$(grep -n '^func (m \*FraiseqlCi) Test(' "$d/.dagger/main.go" | cut -d: -f1)
if [ -z "$decoy_at" ] || [ -z "$test_at" ] || [ "$decoy_at" -ge "$test_at" ]; then
    echo "FAIL: the decoy does not sit above Test, so it decoys nothing"
    failed=$((failed + 1))
fi
plant_in_test "$d" '"bash tools/ci-target-canary.sh -- build ' '"bash tools/ci-target-canary.sh -- check '
expect_red "a canary line of neither form, under a decoy above Test" "$d" Test \
    'a canary line of neither form'

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

# --- --crates: a crate's lines out of the shards ----------------------------------
crates_expect() {
    local name="$1" d="$2" want="$3"
    shift 3
    cases=$((cases + 1))
    if [ "$d" != "$repo_root" ] && cmp -s "$repo_root/.dagger/main.go" "$d/.dagger/main.go" &&
        [ "${name#baseline}" = "$name" ]; then
        echo "FAIL: $name: the plant changed nothing"
        failed=$((failed + 1))
        return
    fi
    LEG_LINES_ROOT="$d" python3 "$READER" --crates "$@" >"$work/out" 2>&1
    local rc=$?
    if [ -n "$want" ] && [ $rc -ne 0 ] && grep -qF -- "$want" "$work/out"; then
        echo "ok: $name"
    else
        echo "FAIL: $name: rc=$rc, wanted red with: $want"
        sed 's/^/    /' "$work/out"
        failed=$((failed + 1))
    fi
}

# Every line printed tests one of the crates, every crate has a line, and each line is
# one the shard runs, verbatim.
d="$(case_dir crates-baseline)"
cases=$((cases + 1))
if LEG_LINES_ROOT="$d" python3 "$READER" --crates fraiseql-core,fraiseql-db Test integrationPostgres >"$work/crates.out" 2>&1 &&
    [ "$(grep -c -- '-p fraiseql-core ' "$work/crates.out")" -ge 2 ] &&
    [ "$(grep -c -- '-p fraiseql-db ' "$work/crates.out")" -ge 2 ] &&
    [ "$(grep -cv -- '^cargo test -p fraiseql-\(core\|db\) ' "$work/crates.out")" = 0 ] &&
    [ "$(grep -cxFf "$work/crates.out" "$work/test.lines" "$work/pg.lines" | awk -F: '{n += $2} END {print n}')" = "$(wc -l <"$work/crates.out")" ]; then
    echo "ok: --crates prints the crates' lines as the shards run them, and only those"
else
    echo "FAIL: --crates prints the crates' lines as the shards run them, and only those"
    sed 's/^/    /' "$work/crates.out"
    failed=$((failed + 1))
fi

# A line for another crate is left out, one whose name the crate's is a prefix of too.
d="$(case_dir crates-other)"
plant_in_test "$d" $'\n\t\t"cargo test -p fraiseql-core --lib",\n' \
    $'\n\t\t"cargo test -p fraiseql-db-planted --lib",\n'
cases=$((cases + 1))
if planted "$d" "a line for another crate is excluded" &&
    LEG_LINES_ROOT="$d" python3 "$READER" Test | grep -c -- '-p fraiseql-db-planted ' >/dev/null &&
    LEG_LINES_ROOT="$d" python3 "$READER" --crates fraiseql-db Test >"$work/out" 2>&1 &&
    [ "$(grep -c 'fraiseql-db-planted\|fraiseql-core' "$work/out")" = 0 ] &&
    [ "$(grep -c -- '-p fraiseql-db ' "$work/out")" = 1 ]; then
    echo "ok: a line for another crate is excluded"
else
    echo "FAIL: a line for another crate is excluded"
    sed 's/^/    /' "$work/out"
    failed=$((failed + 1))
fi

crates_expect "an unknown crate" "$repo_root" 'no shard tests `fraiseql-nope`' \
    fraiseql-core,fraiseql-nope Test integrationPostgres

d="$(case_dir crates-empty)"
plant_in_test "$d" $'\n\t\t"cargo test -p fraiseql-db --lib --features \'" + dbTestFeatures + "\'",\n' \
    $'\n\t\t"cargo test -p fraiseql-time --lib --features \'" + dbTestFeatures + "\'",\n'
crates_expect "a crate the named shards run no line for" "$d" \
    '`Test` run no cargo test line for `fraiseql-db`' fraiseql-core,fraiseql-db Test

crates_expect "a name that is not a shard" "$repo_root" '(not a shard: integrationRedis)' \
    fraiseql-db Test integrationRedis

# --- coverage: CoveragePostgres measures core and db, coverage.yml the rest -------
cov_dir() {
    local d
    d="$(case_dir "$1")"
    mkdir -p "$d/.github/workflows"
    cp "$repo_root/.github/workflows/coverage.yml" "$d/.github/workflows/coverage.yml"
    printf '%s\n' "$d"
}

# Replace the first line holding `$2` in the case's coverage.yml with `$3`, newlines
# written as \n.
cov_plant() {
    python3 - "$1/.github/workflows/coverage.yml" "$2" "$3" <<'PY'
import sys
p, anchor, new = sys.argv[1:]
lines = open(p).read().split("\n")
hits = [i for i, l in enumerate(lines) if anchor in l]
assert hits, anchor
i = hits[0]
indent = lines[i][: len(lines[i]) - len(lines[i].lstrip())]
lines[i : i + 1] = [indent + l for l in new.split("\\n")]
open(p, "w").write("\n".join(lines))
PY
}

cov_expect() {
    local name="$1" d="$2" want="$3"
    cases=$((cases + 1))
    if cmp -s "$repo_root/.github/workflows/coverage.yml" "$d/.github/workflows/coverage.yml" &&
        cmp -s "$repo_root/.dagger/main.go" "$d/.dagger/main.go" && [ "$name" != baseline ]; then
        echo "FAIL: $name: the plant changed nothing"
        failed=$((failed + 1))
        return
    fi
    LEG_LINES_ROOT="$d" python3 "$READER" --coverage >"$work/out" 2>&1
    local rc=$?
    if [ -z "$want" ] && [ $rc -eq 0 ]; then
        echo "ok: $name"
    elif [ -n "$want" ] && [ $rc -ne 0 ] && grep -qF -- "$want" "$work/out"; then
        echo "ok: $name"
    else
        echo "FAIL: $name: rc=$rc, wanted ${want:-green}"
        sed 's/^/    /' "$work/out"
        failed=$((failed + 1))
    fi
}

d="$(cov_dir cov-baseline)"
cov_expect baseline "$d" ""

d="$(cov_dir cov-literal)"
cov_plant "$d" '--all-features \' '--features "test-postgres,arrow,kafka" \'
cov_expect "a literal --features in a run block" "$d" 'a literal `--features test-postgres,arrow,kafka`'

d="$(cov_dir cov-continued)"
cov_plant "$d" '--all-features \' '--features \\n  "postgres,wire-backend" \'
cov_expect "a literal --features split over a continuation" "$d" 'a literal `--features postgres,wire-backend`'

d="$(cov_dir cov-equals)"
cov_plant "$d" '--all-features \' '--features=postgres,wire-backend \'
cov_expect "a literal --features=" "$d" 'a literal `--features postgres,wire-backend`'

d="$(cov_dir cov-yaml-comment)"
cov_plant "$d" '# Collect profdata across multiple runs' '# Collect profdata across multiple runs\n# cargo llvm-cov --features "arrow,kafka" -p fraiseql-core'
cov_expect "a literal in a YAML comment runs nothing" "$d" ""

d="$(cov_dir cov-shell-comment)"
cov_plant "$d" '--all-features \' '--all-features \\n# was: cargo llvm-cov --features "arrow,kafka" -p fraiseql-db \'
cov_expect "a literal in a shell comment runs nothing" "$d" ""

# In bash a comment line inside a continued command ends it, trailing `\` and all: the
# line after it runs as a command of its own, and its literal is read.
d="$(cov_dir cov-comment-continued)"
cov_plant "$d" '--all-features \' '# an old note \\n--features "arrow,kafka" \'
cov_expect "a comment line ending a continued command hides no literal after it" "$d" 'a literal `--features arrow,kafka`'

d="$(cov_dir cov-echo)"
cov_plant "$d" 'run: cargo llvm-cov clean --workspace' 'run: |\n  echo was cargo llvm-cov report -p fraiseql-db --features arrow,kafka\n  cargo llvm-cov clean --workspace'
cov_expect "a literal in an echo runs nothing" "$d" ""

d="$(cov_dir cov-unknown-const)"
cov_plant "$d" '--all-features \' '--features "$(python3 tools/leg-lines.py --const dbFeatures)" \'
cov_expect "a --const naming no const" "$d" '`--const dbFeatures` is not a const'

d="$(cov_dir cov-threshold)"
cov_plant "$d" 'run: cargo llvm-cov report -p fraiseql-federation --fail-under-lines 60' 'run: cargo llvm-cov report -p fraiseql-db --fail-under-lines 50'
cov_expect "the workflow measures a crate CoveragePostgres measures" "$d" 'measures `fraiseql-db`, which CoveragePostgres measures'

d="$(cov_dir cov-unexcluded)"
cov_plant "$d" '--exclude fraiseql-core \' '\'
cov_expect "a --workspace run that does not exclude core" "$d" 'measures `fraiseql-core`, which CoveragePostgres measures'

d="$(cov_dir cov-func-crate)"
sed -i 's/leg-lines.py --crates fraiseql-core,fraiseql-db /leg-lines.py --crates fraiseql-core /' "$d/.dagger/main.go"
cov_expect "CoveragePostgres reads no line for db" "$d" "CoveragePostgres does not read \`fraiseql-db\`'s lines"

d="$(cov_dir cov-func-gone)"
sed -i 's/^func (m \*FraiseqlCi) CoveragePostgres(/func (m *FraiseqlCi) CoverageRenamed(/' "$d/.dagger/main.go"
cov_expect "CoveragePostgres is gone" "$d" 'no `CoveragePostgres` function'

# A feature planted in a const is what --const prints.
d="$(cov_dir cov-flow)"
sed -i 's/^\(\s*coreTestFeatures\s*=\s*"\)/\1planted-feature,/' "$d/.dagger/main.go"
cases=$((cases + 1))
if planted "$d" "--const prints a feature planted in the const" &&
    LEG_LINES_ROOT="$d" python3 "$READER" --const coreTestFeatures | grep -c '^planted-feature,' >/dev/null; then
    echo "ok: --const prints a feature planted in the const"
else
    echo "FAIL: --const prints a feature planted in the const"
    failed=$((failed + 1))
fi

d="$(cov_dir cov-missing-const)"
cases=$((cases + 1))
if ! LEG_LINES_ROOT="$d" python3 "$READER" --const noSuchFeatures >"$work/out" 2>&1 &&
    grep -qF '`noSuchFeatures` is not a const of .dagger/main.go' "$work/out"; then
    echo "ok: --const refuses a name main.go does not define"
else
    echo "FAIL: --const refuses a name main.go does not define"
    failed=$((failed + 1))
fi

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
echo "OK: tools/leg-lines.py refuses every shape it cannot read, the mirror runs every line, --crates prints a crate's lines and nothing else, and coverage leaves core and db to CoveragePostgres."
