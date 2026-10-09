#!/usr/bin/env bash
# PostgreSQL 18 evaluations (#1468, #1469, #1470): run one rig against $DATABASE_URL.
#
#   tools/bench/pg18/run.sh uuidv7   [rows] [repeats]   # default 1000000 3
#   tools/bench/pg18/run.sh sql_json [rows] [repeats]   # default 100000 7
#   tools/bench/pg18/run.sh returning [rows] [repeats]  # default 10000 7
#
# Each rig creates its objects in its own scratch schema and drops it. Results print as
# psql tables; record them, with `SELECT version()` and the machine, in
# docs/benchmarks/pg18-evaluations.md.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
rig=${1:?usage: run.sh uuidv7|sql_json|returning [rows] [repeats]}
: "${DATABASE_URL:?set DATABASE_URL to a PostgreSQL 18 database}"
psql "$DATABASE_URL" -Atc "SELECT version()"
case "$rig" in
  uuidv7)
    for _ in $(seq "${3:-3}"); do
      psql "$DATABASE_URL" -q -v n="${2:-1000000}" -f "$here/uuidv7.sql"
    done
    ;;
  sql_json)
    psql "$DATABASE_URL" -q -v n="${2:-100000}" -v reps="${3:-7}" -f "$here/sql_json.sql"
    ;;
  returning)
    psql "$DATABASE_URL" -q -v n="${2:-10000}" -v reps="${3:-7}" -f "$here/returning.sql"
    ;;
  *)
    echo "unknown rig: $rig" >&2
    exit 2
    ;;
esac
