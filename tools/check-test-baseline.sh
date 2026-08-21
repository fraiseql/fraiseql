#!/usr/bin/env bash
# Turn "the suite is known-red locally" into a checked statement.
#
# A local `cargo nextest run` without a reachable Postgres fails ~80 tests, so a
# reviewer reads the summary, shrugs, and greps for their own error strings. That
# is a workaround, not a gate, and it is how a real regression eventually gets
# missed: the eightieth failure and the eighty-first look identical.
#
# This runs the suite and classifies every failure by **what it printed**, not by
# its name:
#
#   * environmental — the failure carries the `DATABASE_URL` sentinel, i.e. the test never
#     reached the code under test. Expected on a workstation; these run for real in the Dagger
#     integration leg.
#   * allow-listed — a named test with a written reason, below. Exactly one entry, and adding
#     another should feel expensive.
#   * anything else — a real failure. Named, and the gate exits non-zero.
#
# It also fails when an allow-listed test *passes*, so the list cannot go stale
# unnoticed.
#
# Usage:  tools/check-test-baseline.sh [nextest args…]
# Default selection is the four crates a workstation can build quickly.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

# Test name → why it cannot pass in this configuration.
#
# `every_manifest_entry_matches_a_real_leaf` walks `ServerConfig::default()` and
# asserts every manifest entry matches a live leaf. Feature-gated subsystems
# (saml, observers, cdc_outbound, export, flight, mailbox, send, sources,
# webhooks) contribute no leaves when their features are off, so the manifest
# reads as stale. It is meaningful only under `--all-features`, which is where CI
# runs it.
ALLOWED_NAMES=(
  "every_manifest_entry_matches_a_real_leaf"
)

# What an "I never reached the code under test" failure prints.
ENV_SENTINEL='DATABASE_URL must be set'

SELECTION=("$@")
if [ ${#SELECTION[@]} -eq 0 ]; then
  SELECTION=(-p fraiseql-core -p fraiseql-db -p fraiseql-cli -p fraiseql-codegen -p fraiseql-server)
fi

OUT=$(mktemp)
trap 'rm -f "$OUT"' EXIT

cargo nextest run "${SELECTION[@]}" --no-fail-fast > "$OUT" 2>&1
# Deliberately not `$?` on a pipeline: a red suite is the normal case here, and
# the classification below is what decides the exit status.

# Strip ANSI: nextest colours its output, and a colour code between the line
# start and `FAIL` makes every pattern below miss.
sed -i -E 's/\x1b\[[0-9;]*m//g' "$OUT"

mapfile -t FAILED < <(grep -E '^\s+FAIL' "$OUT" | sed -E 's/.*\) //' | sort -u)

if [ ${#FAILED[@]} -eq 0 ]; then
  echo "OK: no failures."
  exit 0
fi

# Which tests printed the sentinel, read from the *block structure* rather than
# by grepping around a name: nextest prints each failure's output under its own
# `FAIL … name` line, and a fixed `-A N` window either misses a long block or
# swallows the next test's — the second direction would classify a real failure
# as environmental, which is the one mistake this script must not make.
mapfile -t ENVIRONMENTAL < <(awk -v sentinel="$ENV_SENTINEL" '
  /^[[:space:]]+FAIL/ { sub(/.*\) /, ""); current = $0; next }
  /^[[:space:]]+(PASS|SKIP)/ { current = ""; next }
  /^[[:space:]]*Summary/ { current = ""; next }
  current != "" && index($0, sentinel) { print current; current = "" }
' "$OUT" | sort -u)

is_environmental() {
  local needle=$1
  local seen
  for seen in ${ENVIRONMENTAL+"${ENVIRONMENTAL[@]}"}; do
    [ "$seen" = "$needle" ] && return 0
  done
  return 1
}

env_count=0
allowed_count=0
real=()

for entry in "${FAILED[@]}"; do
  name=${entry##* }

  if is_environmental "$entry"; then
    env_count=$((env_count + 1))
    continue
  fi

  is_allowed=false
  for allowed in "${ALLOWED_NAMES[@]}"; do
    if [ "$name" = "$allowed" ]; then
      is_allowed=true
      break
    fi
  done

  if $is_allowed; then
    allowed_count=$((allowed_count + 1))
  else
    real+=("$entry")
  fi
done

# A stale allow-list is a silent hole: the entry stops describing anything and
# the next real failure of that name is waved through.
for allowed in "${ALLOWED_NAMES[@]}"; do
  if ! printf '%s\n' "${FAILED[@]}" | grep -qF "$allowed"; then
    if grep -qF "$allowed" "$OUT"; then
      echo "ERROR: '$allowed' is allow-listed but did not fail — drop it from ALLOWED_NAMES."
      exit 1
    fi
  fi
done

echo "failures: ${#FAILED[@]} (environmental: $env_count, allow-listed: $allowed_count, real: ${#real[@]})"

if [ ${#real[@]} -gt 0 ]; then
  echo
  echo "ERROR: failures that are neither environmental nor allow-listed:"
  printf '  %s\n' "${real[@]}"
  exit 1
fi

echo "OK: every failure is environmental (no reachable Postgres) or allow-listed."
