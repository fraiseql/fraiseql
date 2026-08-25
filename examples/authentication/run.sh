#!/usr/bin/env bash
# Run this example.
#
# It needs no database and no compiled schema — it signs its own tokens and
# validates them in-process. The script exists so every example under examples/
# has the same entry point, including in the smoke leg.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

exec cargo run --quiet -p fraiseql-example-authentication
