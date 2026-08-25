#!/usr/bin/env bash
# Compile the schema this example reads, then run it.
#
# schema.compiled.json is a build artifact and is gitignored, so `cargo run` on a
# fresh clone has nothing to load. This script makes it first. It is the one
# command the README documents, and it is what the examples smoke leg runs.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

if [ -z "${DATABASE_URL:-}" ]; then
    echo "DATABASE_URL is not set." >&2
    echo >&2
    echo "  createdb fraiseql_example" >&2
    echo "  psql -v ON_ERROR_STOP=1 -d fraiseql_example -f ../ecommerce/sql/setup.sql" >&2
    echo "  export DATABASE_URL=postgresql://localhost/fraiseql_example" >&2
    exit 2
fi

cargo run --quiet -p fraiseql-cli -- \
    compile "../ecommerce/schema.json" -o "../ecommerce/schema.compiled.json"

exec cargo run --quiet -p fraiseql-example-complex-queries
