#!/usr/bin/env bash
# Compile the schema this example reads, then run it.
#
# schema.compiled.json is a build artifact and is gitignored, so `cargo run` on a
# fresh clone has nothing to load. This script makes it first. It is the one
# command the README documents, and it is what the examples smoke leg runs.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

cargo run --quiet -p fraiseql-cli -- \
    compile "../streaming/schema.json" -o "../streaming/schema.compiled.json"

exec cargo run --quiet -p fraiseql-example-subscriptions
