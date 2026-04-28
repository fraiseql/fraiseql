#!/bin/bash
# Build test WASM component fixtures

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Ensure wasm32-wasip2 target is installed
echo "Checking wasm32-wasip2 target..."
rustup target add wasm32-wasip2 || true

# Build guest-identity
echo "Building guest-identity..."
cd "$SCRIPT_DIR/guest-identity"
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/fraiseql_guest_identity.wasm "$SCRIPT_DIR/guest-identity.wasm"

# Build guest-transform
echo "Building guest-transform..."
cd "$SCRIPT_DIR/guest-transform"
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/fraiseql_guest_transform.wasm "$SCRIPT_DIR/guest-transform.wasm"

echo "Build complete!"
ls -lh "$SCRIPT_DIR"/*.wasm
