#!/usr/bin/env bash
# crate_growth_test.sh — the red capability of tools/check-crate-growth.py.
#
# Each case builds a git repository with one crate, commits growth of a known size, and
# runs the check over the range. A red must name what grew; a green must say how many
# commits it checked, so a range that resolved to nothing cannot read as a pass.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-crate-growth.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
cases=0
n=0

export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid
export GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null

# A fresh repository on `main` whose crate `demo` holds 11 production lines.
repo() {
    n=$((n + 1))
    r="${tmp}/r${n}"
    git init -q -b main "$r"
    add_prod demo 10
    commit "chore: the crate"
}

# add_prod <crate> <lines>: that many production lines, in a file of their own.
add_prod() {
    mkdir -p "$r/crates/$1/src"
    local f="$r/crates/$1/src/p$((++n)).rs"
    for ((i = 0; i < $2; i++)); do printf 'pub fn f%s_%s() {}\n' "$n" "$i"; done >"$f"
    [ -f "$r/crates/$1/src/lib.rs" ] || printf 'pub fn root() {}\n' >"$r/crates/$1/src/lib.rs"
}

# add_free <crate> <lines>: docs, blanks and a unit-test module, which the count leaves out.
add_free() {
    local f="$r/crates/$1/src/free$((++n)).rs"
    {
        for ((i = 0; i < $2; i++)); do printf '/// doc %s\n\n' "$i"; done
        printf '#[cfg(test)]\nmod t {\n'
        for ((i = 0; i < $2; i++)); do printf '    fn t%s() {}\n' "$i"; done
        printf '}\n'
    } >"$f"
}

commit() { git -C "$r" add -A && git -C "$r" commit -q --cleanup=verbatim -m "$1"; }
sha() { git -C "$r" rev-parse "${1:-HEAD}"; }

# expect <rc> <needle> <label> [args…]: run the check over the repository.
expect() {
    local want="$1" needle="$2" label="$3"
    shift 3
    cases=$((cases + 1))
    CRATE_GROWTH_ROOT="$r" python3 "$gate" --fallback main "$@" >"${tmp}/out" 2>&1
    local rc=$?
    if [ "$rc" -eq "$want" ] && grep -qF -- "$needle" "${tmp}/out"; then
        echo "  ok   $label"
    else
        echo "  FAIL $label (rc=$rc, want $want, needle: $needle)"
        sed 's/^/       /' "${tmp}/out"
        failures=$((failures + 1))
    fi
}

echo "check-crate-growth.py:"

# --- the threshold, and what it counts ---------------------------------------------
repo; base="$(sha)"
add_prod demo 300; commit "feat: 300 lines"
expect 0 "1 commit(s) checked" "a commit adding 300 production lines passes" --base "$base"

repo; base="$(sha)"
add_prod demo 301; commit "feat: 301 lines"
expect 1 "demo grows by 301 production lines (11 → 312)" \
    "a commit adding 301 production lines with no trailer fails" --base "$base"

repo; base="$(sha)"
add_prod demo 10; add_free demo 500; commit "test: lots of tests and docs"
expect 0 "1 commit(s) checked" "docs, blanks and unit tests are not growth" --base "$base"

# --- the trailer ------------------------------------------------------------------
repo; base="$(sha)"
add_prod demo 400
commit "$(printf 'feat: a new surface\n\nBody.\n\nCrate-Growth: demo +400 the new surface is one parser and its errors')"
expect 0 "1 commit(s) checked" "a trailer covering the growth admits it" --base "$base"

repo; base="$(sha)"
add_prod demo 400
commit "$(printf 'feat: a new surface\n\nCrate-Growth: other +400 the new surface is one parser and its errors')"
expect 1 "demo grows by 400" "a trailer naming another crate admits nothing" --base "$base"

repo; base="$(sha)"
add_prod demo 400
commit "$(printf 'feat: a new surface\n\nCrate-Growth: demo +350 the new surface is one parser and its errors')"
expect 1 "more than the +350 its \`Crate-Growth\` trailer admits" \
    "a trailer below the measured growth fails" --base "$base"

repo; base="$(sha)"
add_prod demo 400
commit "$(printf 'feat: a new surface\n\nCrate-Growth: demo +400 see #1776')"
expect 1 "gives no reason in words" "a trailer whose reason is an issue reference fails" --base "$base"

repo; base="$(sha)"
add_prod demo 400
commit "$(printf 'feat: a new surface\n\nCrate-Growth: demo +400')"
expect 1 "is not \`Crate-Growth: <crate> +<lines> <reason>\`" \
    "a trailer with no reason fails" --base "$base"

repo; base="$(sha)"
add_prod demo 400
commit "$(printf 'feat: a new surface\n\nCrate-Growth: demo +400 the new surface is one parser\nCrate-Growth: demo +500 and the errors it reports to callers')"
expect 1 "two \`Crate-Growth\` trailers name \`demo\`" "two trailers for one crate fail" --base "$base"

repo; base="$(sha)"
add_prod demo 400
commit "$(printf 'feat: Crate-Growth: demo +400 the new surface is one parser and its errors\n\nBody text.')"
expect 1 "demo grows by 400" "the trailer text in the subject admits nothing" --base "$base"

repo; base="$(sha)"
add_prod demo 400
commit "$(printf 'feat: a new surface\n\nCrate-Growth: demo +400 the new surface is one parser and its errors\n\nA closing paragraph that is not a trailer block.')"
expect 1 "demo grows by 400" "the trailer text mid-body admits nothing" --base "$base"

# --- a stack is checked commit by commit -------------------------------------------
repo; base="$(sha)"
add_prod demo 100; commit "feat: one"
add_prod demo 400; commit "feat: two"; middle="$(sha)"
add_prod demo 100; commit "feat: three"
expect 1 "${middle:0:12} (feat: two): demo grows by 400" \
    "only the middle commit of a stack is over, and it is the one named" --base "$base"
cases=$((cases + 1))
if [ "$(grep -c 'grows by' "${tmp}/out")" = 1 ]; then echo "  ok   ...and nothing else is named"; else
    echo "  FAIL ...and nothing else is named"; failures=$((failures + 1)); fi

repo; base="$(sha)"
add_prod demo 200; commit "feat: one"
add_prod demo 200; commit "feat: two"
expect 0 "2 commit(s) checked" "a stack over 300 in total, each commit under, passes" --base "$base"

# --- new, deleted and merged crates -------------------------------------------------
repo; base="$(sha)"
add_prod fresh 300; commit "feat: a new crate"
expect 1 "fresh grows by 301 production lines (0 → 301)" \
    "a new crate counts from zero" --base "$base"

repo; add_prod gone 200; commit "feat: a crate to delete"; base="$(sha)"
git -C "$r" rm -rq crates/gone; commit "chore: delete it"
expect 0 "1 commit(s) checked" "deleting a crate passes" --base "$base"

repo; base="$(sha)"
git -C "$r" switch -q -c side
add_prod demo 400; commit "feat: on a side branch"; side="$(sha)"
git -C "$r" switch -q main
add_prod other 5; commit "feat: on main"
git -C "$r" merge -q --no-ff --no-edit side
expect 1 "${side:0:12} (feat: on a side branch): demo grows by 400" \
    "a merged side commit over the limit is named" --base "$base"
grep -qF "1 merge(s) skipped" "${tmp}/out" && echo "  ok   ...and the merge itself is skipped" ||
    { echo "  FAIL ...and the merge itself is skipped"; failures=$((failures + 1)); }
cases=$((cases + 1))

# --- the range ----------------------------------------------------------------------
repo; base="$(sha)"
add_prod demo 10; commit "feat: one"
add_prod demo 10; commit "feat: two"
add_prod demo 10; commit "feat: three"
expect 0 "3 commit(s) checked" "a three-commit range reports three" --base "$base"
expect 0 "0 commit(s) checked" "an empty range passes and says so" --base HEAD
git -C "$r" switch -q -c rewritten "$base"
add_prod demo 10; commit "feat: rewritten"
add_prod demo 10; commit "feat: rewritten again"
expect 0 "2 commit(s) checked" "a branch-creating push (all-zero base) checks from the fallback" \
    --base 0000000000000000000000000000000000000000
# A force-push: the old tip sat on an over-limit commit the new tip still carries. A
# range from the old tip would leave that commit out, as an ancestor of the old tip.
git -C "$r" switch -q -c pushed "$base"
add_prod demo 400; commit "feat: over, before the force-push"; over="$(sha)"
add_prod demo 10; commit "feat: the old tip"; old_tip="$(sha)"
git -C "$r" switch -q -c forced "$over"
add_prod demo 10; commit "feat: the new tip"
expect 1 "${over:0:12} (feat: over, before the force-push): demo grows by 400" \
    "a force-push (base not an ancestor) checks from the fallback" --base "$old_tip"
expect 1 "crate growth: FAIL — git rev-parse" "an unresolvable base fails" --base no-such-rev

repo; base="$(sha)"
add_prod demo 10; commit "feat: one"
add_prod demo 10; commit "feat: two"
git clone -q --depth 1 "file://$r" "${tmp}/shallow"
r="${tmp}/shallow"
expect 1 "the repository is shallow" "a shallow clone fails rather than passing" --base HEAD~1

echo
echo "$cases cases, $failures failed"
[ "$failures" -eq 0 ] || exit 1
echo "OK: tools/check-crate-growth.py refuses a commit growing a crate by more than 300 production lines without a trailer that covers it."
