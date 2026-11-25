#!/usr/bin/env bash
# crate_lists_test.sh — the red capability of tools/check-crate-lists.py.
#
# Each case writes one release file into a fixture workspace of four publishable
# crates (alpha, beta, gamma, delta) and one that is not (hidden), and runs the check
# over it. A red must name the file and line; a green must say it scanned.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-crate-lists.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
cases=0

# A fresh fixture workspace; `release-file <path>` then takes the file's text on stdin.
workspace() {
    rm -rf "$tmp/ws"
    mkdir -p "$tmp/ws/.github/workflows" "$tmp/ws/.dagger" "$tmp/ws/tools"
    printf '[workspace]\nmembers = ["crates/*"]\n' >"$tmp/ws/Cargo.toml"
    for c in alpha beta gamma delta hidden; do
        mkdir -p "$tmp/ws/crates/$c"
        printf '[package]\nname = "%s"\nversion = "0.1.0"\n' "$c" >"$tmp/ws/crates/$c/Cargo.toml"
    done
    printf 'publish = false\n' >>"$tmp/ws/crates/hidden/Cargo.toml"
}
release-file() { mkdir -p "$(dirname "$tmp/ws/$1")" && cat >"$tmp/ws/$1"; }

# expect <rc> <needle> <label>
expect() {
    local want="$1" needle="$2" label="$3"
    cases=$((cases + 1))
    CRATE_LISTS_ROOT="$tmp/ws" python3 "$gate" >"$tmp/out" 2>&1
    local rc=$?
    if [ "$rc" -eq "$want" ] && grep -qF -- "$needle" "$tmp/out"; then
        echo "  ok   $label"
    else
        echo "  FAIL $label (rc=$rc, want $want, needle: $needle)"
        sed 's/^/       /' "$tmp/out"
        failures=$((failures + 1))
    fi
}

echo "check-crate-lists.py:"

workspace
release-file tools/publish.sh <<<'echo "nothing listed here"'
expect 0 "none lists the 4 publishable crates" "a release file with no list passes"

# --- the floor ---------------------------------------------------------------------
workspace
release-file tools/smoke.sh <<<'for bin in alpha beta; do "$bin" --version; done'
expect 0 "crate lists: ok" "two names are a pair, not a list"

workspace
release-file tools/publish.sh <<<'for c in alpha beta gamma; do cargo publish -p "$c"; done'
expect 1 "tools/publish.sh:1: 3 crates listed" "three names unmarked are a list"

workspace
release-file tools/publish.sh <<<'echo alpha alpha alpha'
expect 0 "crate lists: ok" "one name three times is not three crates"

# --- the shapes --------------------------------------------------------------------
workspace
release-file .github/workflows/release.yml <<'EOF'
jobs:
  verify:
    steps:
      - run: |
          CRATES="alpha beta \
                  gamma delta"
EOF
expect 1 ".github/workflows/release.yml:5: 4 crates listed" \
    "a workflow list continued over lines is one list"

workspace
release-file .dagger/release.go <<'EOF'
var order = []string{"alpha", "beta", "gamma"}
EOF
expect 1 ".dagger/release.go:1: 3 crates listed" "a Go []string on one line"

workspace
release-file .dagger/release.go <<'EOF'
var order = []string{
	"alpha",
	"beta",
	"gamma",
}
EOF
expect 1 ".dagger/release.go:2: 3 crates listed" "a Go []string one name per line"

workspace
release-file releasing.md <<'EOF'
**Order of publication:**

1. alpha (no dependencies)
2. beta (depends on alpha)
3. gamma (depends on beta)
EOF
expect 1 "releasing.md:3: 3 crates listed" "a markdown numbered list in the release guide"

workspace
release-file releasing.md <<<'Publish alpha, then beta, and then gamma when both are indexed.'
expect 0 "crate lists: ok" "names in a sentence are prose, not a list"

workspace
release-file tools/publish.sh <<<'for c in alpha beta hidden; do :; done'
expect 0 "crate lists: ok" "a crate saying publish = false is not counted"

# --- the marker --------------------------------------------------------------------
workspace
release-file tools/semver.sh <<'EOF'
# crate-subset: the crates whose API is load-bearing
for c in alpha beta gamma; do cargo semver-checks -p "$c"; done
EOF
expect 0 "crate lists: ok" "a subset marked on the line above passes"

workspace
release-file tools/semver.sh <<'EOF'
for c in alpha beta gamma; do cargo semver-checks -p "$c"; done  # crate-subset: the API surface we promise
EOF
expect 0 "crate lists: ok" "a subset marked on its own line passes"

workspace
release-file releasing.md <<'EOF'
<!-- crate-subset: the three crates this page documents -->
1. alpha
2. beta
3. gamma
EOF
expect 0 "crate lists: ok" "a markdown list marked with an HTML comment passes"

workspace
release-file tools/semver.sh <<'EOF'
# crate-subset: semver
for c in alpha beta gamma; do :; done
EOF
expect 1 "gives no reason in words" "a marker with a one-word reason fails"

workspace
release-file tools/semver.sh <<'EOF'
# crate-subset: the crates whose API is load-bearing

for c in alpha beta gamma; do :; done
EOF
expect 1 "tools/semver.sh:3: 3 crates listed" "a marker two lines away marks nothing"

# --- the crate set -----------------------------------------------------------------
workspace
for c in beta gamma delta; do printf 'publish = false\n' >>"$tmp/ws/crates/$c/Cargo.toml"; done
expect 1 "publishes 1 crate(s); nothing to check" "a workspace publishing one crate is refused"

echo
echo "$cases cases, $failures failed"
[ "$failures" -eq 0 ] || exit 1
echo "OK: tools/check-crate-lists.py refuses a release file listing three or more publishable crates without a reason."
