#!/usr/bin/env bash
# Fails if any test file uses bare DATABASE_URL resolution instead of fraiseql-test-utils.
set -euo pipefail

PATTERN='std::env::var\("DATABASE_URL"\)'
MATCHES=$(grep -r "$PATTERN" crates/*/tests/ --include="*.rs" -l 2>/dev/null || true)

if [ -n "$MATCHES" ]; then
  echo "ERROR: Bare DATABASE_URL resolution found in test files."
  echo "Use fraiseql_test_utils::database_url() instead."
  echo ""
  echo "$MATCHES"
  exit 1
fi
echo "OK: No bare DATABASE_URL patterns in test files."
