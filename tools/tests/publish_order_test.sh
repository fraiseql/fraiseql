#!/usr/bin/env bash
# Unit tests for tools/publish-order.py.
#
# Run directly:  bash tools/tests/publish_order_test.sh
# Exits non-zero if any assertion fails.
#
# The order is read from saved `cargo metadata` documents and the crate set from a
# fixture workspace, so this runs in the toolchain-free ShellGates container.
#
# What is worth pinning, in order of how badly each would hurt:
#
#   1. **An optional dependency is still a dependency.** crates.io resolves every
#      dependency a manifest declares: `fraiseql-cdc-sinks`, optional in
#      fraiseql-server, failed the server's publish when it was not published first.
#   2. **A crate that opts out is never published, and one that opts in always is.**
#   3. **A dev-dependency is not an edge.** It never reaches the registry's resolver,
#      and counting it turns a crate's tests into a cycle.
#   4. **A real cycle stops the release** instead of publishing a partial order.
#   5. **The order is the same on every run.**
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TOOL="$REPO_ROOT/tools/publish-order.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

pass() { echo "  ok    $1"; }
fail() { echo "  FAIL  $1"; TESTS_FAILED=$((TESTS_FAILED + 1)); }

# assert_order <name> <metadata.json> <expected order, space-separated>
assert_order() {
    local name="$1" metadata="$2" want="$3" got
    TESTS_RUN=$((TESTS_RUN + 1))
    if ! got="$(python3 "$TOOL" --metadata "$metadata" 2>&1 | tr '\n' ' ' | sed 's/ $//')"; then
        fail "$name (exited non-zero: $got)"
    elif [ "$got" = "$want" ]; then
        pass "$name"
    else
        fail "$name"
        echo "          want: $want"
        echo "          got:  $got"
    fi
}

# metadata <file> <package JSON objects…>: a `cargo metadata --no-deps` document.
metadata() {
    local file="$1"; shift
    local IFS=,
    printf '{"packages":[%s]}\n' "$*" > "$file"
}

# pkg <name> <publish JSON> [<dependency JSON>…]
pkg() {
    local name="$1" publish="$2"; shift 2
    local IFS=,
    printf '{"name":"%s","publish":%s,"dependencies":[%s]}' "$name" "$publish" "$*"
}

# dep <name> <kind JSON> [optional]
dep() {
    printf '{"name":"%s","kind":%s,"optional":%s}' "$1" "$2" "${3:-false}"
}

echo "publish-order.py"

# ── 1. An optional dependency publishes first ────────────────────────────────
# `server` sorts before `sinks` by name, so only the optional edge puts sinks first.
metadata "$WORK/optional.json" \
    "$(pkg server null "$(dep sinks null true)")" \
    "$(pkg sinks null)"
assert_order "an optional dependency is published before its dependent" \
    "$WORK/optional.json" "sinks server"

# A build-dependency resolves on crates.io the same way.
metadata "$WORK/build.json" \
    "$(pkg app null "$(dep codegen '"build"')")" \
    "$(pkg codegen null)"
assert_order "a build-dependency is published before its dependent" \
    "$WORK/build.json" "codegen app"

# ── 2. Who publishes ─────────────────────────────────────────────────────────
metadata "$WORK/publish.json" \
    "$(pkg internal '[]' )" \
    "$(pkg registry '["crates-io"]')" \
    "$(pkg plain null "$(dep internal '"dev"')" "$(dep serde null)")"
assert_order "publish = false is skipped; a registry list and the default publish; crates.io deps are not ours" \
    "$WORK/publish.json" "plain registry"

# ── 3. A dev-dependency is not an edge ───────────────────────────────────────
# core's tests use server, and server depends on core: counted, that is a cycle.
metadata "$WORK/dev.json" \
    "$(pkg core null "$(dep server '"dev"')")" \
    "$(pkg server null "$(dep core null)")"
assert_order "a dev-dependency neither orders nor cycles" \
    "$WORK/dev.json" "core server"

# ── 4. A cycle stops the release ─────────────────────────────────────────────
metadata "$WORK/cycle.json" \
    "$(pkg a null "$(dep b null)")" \
    "$(pkg b null "$(dep a null)")" \
    "$(pkg c null)"
TESTS_RUN=$((TESTS_RUN + 1))
if out="$(python3 "$TOOL" --metadata "$WORK/cycle.json" 2>&1)"; then
    fail "a dependency cycle exits non-zero (printed: $(echo "$out" | tr '\n' ' '))"
elif grep -q "cycle: \['a', 'b'\]" <<<"$out"; then
    pass "a dependency cycle exits non-zero and names its crates"
else
    fail "a dependency cycle names its crates (got: $out)"
fi

# ── 5. The same order every run ──────────────────────────────────────────────
# Three independent crates listed out of order, and a diamond: d needs b and c.
metadata "$WORK/ties.json" \
    "$(pkg zeta null)" \
    "$(pkg d null "$(dep c null)" "$(dep b null)")" \
    "$(pkg c null "$(dep a null)")" \
    "$(pkg b null "$(dep a null)")" \
    "$(pkg a null)" \
    "$(pkg mid null)"
assert_order "ties break by name, and every crate follows all it needs" \
    "$WORK/ties.json" "a b c d mid zeta"

# ── The crate set, read from manifests without cargo ─────────────────────────
mkdir -p "$WORK/ws/crates/"{on,off,opt-in,listed}
cat > "$WORK/ws/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/on", "crates/off", "crates/opt-in", "crates/listed"]
EOF
printf '[package]\nname = "on"\n' > "$WORK/ws/crates/on/Cargo.toml"
printf '[package]\nname = "off"\npublish = false\n' > "$WORK/ws/crates/off/Cargo.toml"
printf '[package]\nname = "opt-in"\npublish = true\n' > "$WORK/ws/crates/opt-in/Cargo.toml"
printf '[package]\nname = "listed"\npublish = ["crates-io"]\n' > "$WORK/ws/crates/listed/Cargo.toml"
TESTS_RUN=$((TESTS_RUN + 1))
got="$(python3 - "$TOOL" "$WORK/ws" <<'EOF'
import importlib.util, sys
from pathlib import Path
spec = importlib.util.spec_from_file_location("publish_order", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
print(" ".join(module.publishable_workspace_crates(Path(sys.argv[2]))))
EOF
)"
if [ "$got" = "on opt-in listed" ]; then
    pass "the manifests' crate set: default, true and a registry list publish; false does not"
else
    fail "the manifests' crate set (got: $got)"
fi

echo
echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
