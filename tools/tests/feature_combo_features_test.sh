#!/usr/bin/env bash
# Red-capability pin for tools/check-feature-combo-features.py.
#
# Run directly:  bash tools/tests/feature_combo_features_test.sh
# Exits non-zero if any assertion fails.
#
# The check exists because five `--no-default-features` combos built their test targets
# with the crate defaults on (#2096): a dev-dependency enabled them, and nothing compared
# what a combo names with what cargo builds. So the cases below are the ways that
# comparison could pass while a combo builds something else: a feature only the test
# targets' edges switch on, an implied feature that is missing, a `cargo tree` that
# fails or prints nothing for the crate, and a combo read with the wrong edge kinds.
#
# `cargo` is stubbed: this pins the check's logic over a four-combo fixture, not the
# workspace's feature graph. The check itself runs `cargo tree` on the real one.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A fixture repo: the two tools, and the real feature-combos.go with its combo list
# replaced by four over one crate, `fx`. The real `cargoArgs()` stays, so the parser's
# pinned hash still matches.
make_fixture() {
    local dir="$1"
    mkdir -p "$dir/tools" "$dir/.dagger" "$dir/bin"
    cp "$REPO_ROOT/tools/feature-combos.py" "$REPO_ROOT/tools/check-feature-combo-features.py" "$dir/tools/"
    python3 - "$REPO_ROOT/.dagger/feature-combos.go" "$dir/.dagger/feature-combos.go" <<'PY'
import re, sys
src = open(sys.argv[1]).read()
combos = """var featureCombos = []featureCombo{
	{name: "fx-lib", crate: "fx", noDefaultFeatures: true, features: []string{"a"}},
	{name: "fx-tests", crate: "fx", noDefaultFeatures: true, allTargets: true, features: []string{"a"}},
	{name: "fx-clippy", crate: "fx", clippy: true, noDefaultFeatures: true, features: []string{"b"}},
	{name: "fx-defaults", crate: "fx", features: []string{"b"}},
}"""
out, n = re.subn(r"^var featureCombos = \[\]featureCombo\{$.*?^\}$", combos, src, flags=re.M | re.S)
assert n == 1
open(sys.argv[2], "w").write(out)
PY
    git -C "$dir" init -q .
    # `fx`'s features: `b` implies `a`, the defaults are `d`, and `opt` is an optional
    # dependency (a node `cargo tree` prints, but not a feature).
    cat >"$dir/metadata.json" <<'JSON'
{"packages": [{"name": "fx", "features": {"default": ["d"], "d": [], "a": ["dep:opt"], "b": ["a", "opt?/f"], "devonly": []}}]}
JSON
    # A `cargo` that logs its argv and answers `metadata` and `tree` as cargo would for
    # `fx`, honestly unless a STUB_* variable says otherwise:
    #   STUB_DEV_ONLY  with every edge kind (test targets), `devonly` and the defaults are on
    #   STUB_DROP      leave this feature out
    #   STUB_TREE_FAIL `cargo tree` exits 101
    #   STUB_NO_ROOT   `cargo tree` prints no line for `fx`
    cat >"$dir/bin/cargo" <<'STUB'
#!/usr/bin/env bash
printf 'cargo %s\n' "$*" >>"$CARGO_STUB_LOG"
case "$1" in
metadata) cat "$STUB_DIR/metadata.json"; exit 0 ;;
tree) ;;
*) echo "stub: unexpected cargo $1" >&2; exit 64 ;;
esac
[ -n "${STUB_TREE_FAIL:-}" ] && { echo "error: stub tree failure" >&2; exit 101; }
features="" defaults=1 edges=""
while [ $# -gt 0 ]; do
    case "$1" in
    --features) features="$2"; shift ;;
    --no-default-features) defaults=0 ;;
    -e) edges="$2"; shift ;;
    esac
    shift
done
on=$(printf '%s' "$features" | tr ',' ' ')
[ "$defaults" = 1 ] && on="$on default d"
case " $on " in *" b "*) on="$on a" ;; esac
[ -n "${STUB_DEV_ONLY:-}" ] && [ "$edges" = "features" ] && on="$on devonly default d"
[ -z "${STUB_NO_ROOT:-}" ] && echo "fx v0.1.0 (/fx)"
echo 'fx feature "opt"'
for f in $on; do
    [ "$f" = "${STUB_DROP:-}" ] && continue
    echo "fx feature \"$f\""
done | sort -u
STUB
    chmod +x "$dir/bin/cargo"
}

# run_case <case-id> <env assignments...>: runs the check in a fresh fixture, leaving
# its exit code in $WORK/<id>/.rc, its output in .out and cargo's argv in .argv.
run_case() {
    local id="$1"; shift
    local dir="$WORK/$id" rc
    make_fixture "$dir"
    set +e
    ( cd "$dir" && env PATH="$dir/bin:$PATH" STUB_DIR="$dir" CARGO_STUB_LOG="$dir/.argv" "$@" \
        python3 tools/check-feature-combo-features.py --root "$dir" >"$dir/.out" 2>&1 )
    rc=$?
    set -e
    echo "$rc" >"$dir/.rc"
}

# check <case-id> <description> <shell condition, run in the case's dir>
check() {
    local id="$1" desc="$2" cond="$3"
    TESTS_RUN=$((TESTS_RUN + 1))
    if ( cd "$WORK/$id" && eval "$cond" ); then
        printf '  ✅ %-12s %s\n' "$id" "$desc"
    else
        printf '  ❌ %-12s %s\n' "$id" "$desc"
        sed 's/^/        /' "$WORK/$id/.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "check-feature-combo-features.py"

run_case honest
check honest "every combo builds what it names: exit 0" '[ "$(cat .rc)" = 0 ]'
check honest "and says how many combos it compared" 'grep -q "all 4 combos" .out'
check honest "an optional dependency node is not read as a feature" '! grep -q opt .out'

run_case devonly STUB_DEV_ONLY=1
check devonly "a feature only the test targets switch on fails: exit 1" '[ "$(cat .rc)" = 1 ]'
check devonly "naming the allTargets combo" 'grep -q "❌ fx-tests" .out'
check devonly "naming the clippy combo, which builds its tests too" 'grep -q "❌ fx-clippy" .out'
check devonly "and the features cargo added" "grep -q \"fx-tests.*\\['d', 'default', 'devonly'\\]\" .out"
check devonly "not the library-only combo" '! grep -q "❌ fx-lib" .out'
check devonly "nor the other library-only combo" '! grep -q "❌ fx-defaults" .out'

run_case missing STUB_DROP=a
check missing "a missing implied feature fails: exit 1" '[ "$(cat .rc)" = 1 ]'
check missing "naming what is left out (b implies a)" "grep -q \"fx-clippy.*leaves out \\['a'\\]\" .out"

run_case nodefault STUB_DROP=d
check nodefault "the defaults left out of a combo that keeps them fails" '[ "$(cat .rc)" = 1 ] && grep -q "❌ fx-defaults" .out'

run_case treefail STUB_TREE_FAIL=1
check treefail "a failing cargo tree is an error, not a pass: exit 2" '[ "$(cat .rc)" = 2 ]'

run_case noroot STUB_NO_ROOT=1
check noroot "cargo tree with no line for the crate is an error: exit 2" '[ "$(cat .rc)" = 2 ]'

run_case edges
check edges "a library-only combo is read without the dev-dependency edges" \
    'grep -q -- "-p fx -e normal,build,features -i fx --no-default-features --features a --locked" .argv'
check edges "an allTargets combo is read with every edge kind" \
    'grep -q -- "-p fx -e features -i fx --no-default-features --features a --locked" .argv'
check edges "a clippy combo is read with every edge kind" \
    'grep -q -- "-p fx -e features -i fx --no-default-features --features b --locked" .argv'
check edges "every cargo call is --locked" '! grep -v -- "--locked" .argv'
check edges "one cargo tree per combo" '[ "$(grep -c "^cargo tree" .argv)" = 4 ]'

# A combo literal the parser cannot read must stop the check, not shorten the list.
mkdir -p "$WORK/unknown"
make_fixture "$WORK/unknown"
sed -i 's/{name: "fx-lib", crate: "fx",/{name: "fx-lib", crate: "fx", env: "X",/' "$WORK/unknown/.dagger/feature-combos.go"
set +e
( cd "$WORK/unknown" && env PATH="$WORK/unknown/bin:$PATH" STUB_DIR="$WORK/unknown" CARGO_STUB_LOG="$WORK/unknown/.argv" \
    python3 tools/check-feature-combo-features.py --root "$WORK/unknown" >"$WORK/unknown/.out" 2>&1 )
echo $? >"$WORK/unknown/.rc"
set -e
check unknown "an unreadable combo literal is an error: exit 2" '[ "$(cat .rc)" = 2 ] && grep -q "unknown featureCombo field" .out'

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "❌ $TESTS_FAILED of $TESTS_RUN assertions failed"
    exit 1
fi
echo "✅ $TESTS_RUN assertions passed"
