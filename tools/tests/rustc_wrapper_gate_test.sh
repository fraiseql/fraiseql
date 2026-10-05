#!/usr/bin/env bash
# Unit tests for tools/check-rustc-wrapper.py.
#
# Run directly:  bash tools/tests/rustc_wrapper_gate_test.sh
# Exits non-zero if any assertion fails.
#
# A fixture workflow that passes, then one mutation per rule: the installer removed, the
# installer moved after the cargo step, the job override removed, a step-level override, a
# local composite action that does and does not install, a different wrapper, an
# unreadable workflow. Each must land on its own verdict.
# shellcheck disable=SC2016  # the backticks in expected messages are the gate's literal text
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-rustc-wrapper.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# good_root <dir>: a workflow-level sccache wrapper, one job that installs it, one job that
# opts out, one job that never runs cargo.
good_root() {
    local r="$1"
    mkdir -p "$r/.github/workflows"
    cat > "$r/.github/workflows/release.yml" <<'EOF'
env:
  RUSTC_WRAPPER: "sccache"
jobs:
  build:
    steps:
      - name: Setup sccache
        uses: mozilla-actions/sccache-action@7d986dd989559c6ecdb630a3fd2557667be217ad
      - name: Build
        run: cargo build --release
  publish:
    env:
      RUSTC_WRAPPER: ""
    steps:
      - name: Publish
        run: cargo publish
  notes:
    steps:
      - name: Echo
        run: echo "no cargo here"
EOF
}

# assert_gate <name> <expected-rc> <expected-substring> <root>
assert_gate() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local name="$1" want_rc="$2" want_sub="$3" root="$4"
    local out rc
    set +e
    out="$(RUSTC_WRAPPER_ROOT="$root" python3 "$GATE" 2>&1)"
    rc=$?
    set -e
    if [[ "$rc" -eq "$want_rc" && "$out" == *"$want_sub"* ]]; then
        echo "  ok: $name"
    else
        echo "  FAIL: $name — rc=$rc (want $want_rc), output did not contain '$want_sub':" >&2
        printf '%s\n' "$out" | sed 's/^/      /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

W=.github/workflows/release.yml

echo "check-rustc-wrapper.py:"
r="$WORK/good"; good_root "$r"
assert_gate "installed, opted-out and cargo-free jobs pass" 0 "rustc wrapper: ok" "$r"

r="$WORK/no-installer"; good_root "$r"
sed -i '/Setup sccache/,+1d' "$r/$W"
assert_gate "cargo with no installer is red" 1 'job `build`, step `Build` runs cargo' "$r"

r="$WORK/installer-late"; good_root "$r"
python3 - "$r/$W" <<'EOF'
import sys
p = sys.argv[1]; s = open(p).read()
setup = "      - name: Setup sccache\n        uses: mozilla-actions/sccache-action@7d986dd989559c6ecdb630a3fd2557667be217ad\n"
build = "      - name: Build\n        run: cargo build --release\n"
open(p, "w").write(s.replace(setup + build, build + setup))
EOF
assert_gate "installer after the cargo step is red" 1 'step `Build` runs cargo' "$r"

r="$WORK/no-override"; good_root "$r"
sed -i '/^  publish:$/,/^    steps:$/{/env:/d;/RUSTC_WRAPPER/d}' "$r/$W"
assert_gate "the publish-rust-sdk shape (no installer, no override) is red" 1 'job `publish`, step `Publish` runs cargo with RUSTC_WRAPPER="sccache"' "$r"

r="$WORK/step-override"; good_root "$r"
sed -i '/^  publish:$/,/^    steps:$/{/env:/d;/RUSTC_WRAPPER/d}' "$r/$W"
sed -i 's/^        run: cargo publish$/        env:\n          RUSTC_WRAPPER: ""\n        run: cargo publish/' "$r/$W"
assert_gate "a step-level override passes" 0 "rustc wrapper: ok" "$r"

r="$WORK/chained"; good_root "$r"
sed -i 's/^        run: echo "no cargo here"$/        run: cd sdk \&\& cargo test/' "$r/$W"
assert_gate "cargo after a shell separator is seen" 1 'job `notes`, step `Echo` runs cargo' "$r"

r="$WORK/composite"; good_root "$r"
mkdir -p "$r/.github/actions/setup-rust"
cat > "$r/.github/actions/setup-rust/action.yml" <<'EOF'
runs:
  using: composite
  steps:
    - name: Setup sccache
      uses: mozilla-actions/sccache-action@v0.0.9
EOF
sed -i 's|uses: mozilla-actions/sccache-action@7d986dd989559c6ecdb630a3fd2557667be217ad|uses: ./.github/actions/setup-rust|' "$r/$W"
assert_gate "a local composite action that installs the wrapper passes" 0 "rustc wrapper: ok" "$r"

sed -i 's|uses: mozilla-actions/sccache-action@v0.0.9|uses: dtolnay/rust-toolchain@stable|' "$r/.github/actions/setup-rust/action.yml"
assert_gate "a local composite action that does not install it is red" 1 'step `Build` runs cargo' "$r"

r="$WORK/other-wrapper"; good_root "$r"
sed -i 's/^  RUSTC_WRAPPER: "sccache"$/  RUSTC_WRAPPER: "cachepot"/' "$r/$W"
assert_gate "installing sccache does not satisfy a different wrapper" 1 'RUSTC_WRAPPER="cachepot"' "$r"

r="$WORK/unreadable"; good_root "$r"
printf 'jobs:\n  build:\n    steps: [unclosed\n' > "$r/$W"
assert_gate "an unreadable workflow is red" 1 "cannot be read" "$r"

r="$WORK/empty"; mkdir -p "$r"
assert_gate "no workflows at all is fatal" 2 "no workflows" "$r"

echo ""
echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[[ "$TESTS_FAILED" -eq 0 ]]
