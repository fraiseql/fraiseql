#!/usr/bin/env bash
# Run this example.
#
# It needs no database and no compiled schema — it runs its own token issuer on
# loopback and validates its tokens. The script exists so every example under examples/
# has the same entry point, including in the smoke leg.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

exec cargo run --quiet -p fraiseql-example-authentication
