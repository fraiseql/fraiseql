#!/usr/bin/env bash
# Real-composer half of the golden two-subgraph federation suite.
#
# Composes the committed FraiseQL-rendered subgraph SDLs with Apollo Federation v2
# composition (@apollo/composition — the engine `rover supergraph compose` wraps)
# and asserts both golden cases:
#   - catalog + reviews          → composes cleanly
#   - catalog + reviews_conflict → rejected with INVALID_FIELD_SHARING (#497)
#
# The fixtures are kept in lock-step with live FraiseQL output by the Rust test
# `committed_sdl_fixtures_are_current`; run that (or `make federation-compose-check`,
# which runs both) after any change to federation SDL rendering.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixtures="$here/../../crates/fraiseql-core/tests/fixtures/federation_compose"

cd "$here"
if [[ ! -d node_modules ]]; then
  echo "→ installing compose-check deps (npm ci)"
  npm ci --no-audit --no-fund
fi

echo "→ positive: catalog + reviews must compose"
node compose-check.mjs \
  "catalog=$fixtures/catalog.graphql" \
  "reviews=$fixtures/reviews.graphql"

echo "→ negative: catalog + reviews_conflict must be rejected (#497)"
node compose-check.mjs --expect-fail=INVALID_FIELD_SHARING \
  "catalog=$fixtures/catalog.graphql" \
  "reviews_conflict=$fixtures/reviews_conflict.graphql"

echo "✓ federation compose check passed"
