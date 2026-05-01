#!/bin/bash
# Build test WASM component fixtures.
#
# Uses wasm32-unknown-unknown to produce components without WASI imports,
# so the host does not need to provide a full WASI implementation.
# Requires: rustup, wasm-tools

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Ensure wasm32-unknown-unknown target is installed
echo "Checking wasm32-unknown-unknown target..."
rustup target add wasm32-unknown-unknown || true

build_component() {
    local dir_name="$1"   # e.g. guest-identity
    local dir="$SCRIPT_DIR/$dir_name"
    # crate name is the package name in Cargo.toml (kebab → snake)
    local crate_name
    crate_name=$(cargo metadata --no-deps --manifest-path "$dir/Cargo.toml" 2>/dev/null \
        | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['packages'][0]['name'].replace('-','_'))")

    echo "Building $dir_name (crate: $crate_name)..."
    cd "$dir"
    cargo build --target wasm32-unknown-unknown --release

    local core_wasm="target/wasm32-unknown-unknown/release/${crate_name}.wasm"
    local embedded="target/${dir_name}-embedded.wasm"
    local out="$SCRIPT_DIR/${dir_name}.wasm"

    wasm-tools component embed wit "$core_wasm" --world fraiseql-function -o "$embedded"
    wasm-tools component new "$embedded" -o "$out"
    echo "  → $out ($(du -h "$out" | cut -f1))"
}

build_component "guest-identity"
build_component "guest-transform"

echo ""
echo "Build complete!"
ls -lh "$SCRIPT_DIR"/*.wasm
