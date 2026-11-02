#!/usr/bin/env bash
# Real-composer half of the golden two-subgraph federation suite.
#
# Composes the committed FraiseQL-rendered subgraph SDLs with Apollo Federation v2
# composition (@apollo/composition — the engine `rover supergraph compose` wraps)
# and asserts the golden cases:
#   - catalog + reviews              → composes cleanly, and the supergraph keeps the
#     role tags catalog's gated query and its type carry (#1717), which is what a
#     gateway contract filters on
#   - catalog + reviews_conflict     → rejected with INVALID_FIELD_SHARING (#497)
#   - default_literals               → every published default is a GraphQL literal of
#     its type (#1888): graphql-js keeps each SDL and introspection default, and an
#     operation omitting a defaulted non-null field validates
#   - cascade_orders + cascade_users → composes cleanly (#698): two cascade-enabled
#     subgraphs synthesize the identical envelope value types, which compose only
#     because the cli marks them @shareable. Without the fix this fails with 21
#     INVALID_FIELD_SHARING errors (one per envelope field).
#
# The catalog/reviews fixtures are kept in lock-step with live FraiseQL output by the
# Rust test `committed_sdl_fixtures_are_current` (fraiseql-core); the cascade_* fixtures
# by `committed_cascade_sdl_fixtures_are_current` (fraiseql-cli — they must be rendered
# from the cli converter, where cascade synthesis and the #698 fix live). Run those (or
# `make federation-compose-check`, which runs all three) after any change to federation
# SDL rendering.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixtures="$here/../../crates/fraiseql-core/tests/fixtures/federation_compose"

cd "$here"
if [[ ! -d node_modules ]]; then
  echo "→ installing compose-check deps (npm ci)"
  npm ci --no-audit --no-fund
fi

echo "→ positive: catalog + reviews must compose, keeping catalog's role tags and input defaults"
node compose-check.mjs \
  '--expect-supergraph=^  direction: SortDirection = ASC$' \
  '--expect-supergraph=^  productCosts: .* @tag\(name: "requires-role/catalog-admin"\)$' \
  '--expect-supergraph=^type ProductCost\n(  @.*\n)*  @tag\(name: "requires-role/catalog-admin"\)\n\{' \
  "catalog=$fixtures/catalog.graphql" \
  "reviews=$fixtures/reviews.graphql"

echo "→ negative: catalog + reviews_conflict must be rejected (#497)"
node compose-check.mjs --expect-fail=INVALID_FIELD_SHARING \
  "catalog=$fixtures/catalog.graphql" \
  "reviews_conflict=$fixtures/reviews_conflict.graphql"

echo "→ positive: cascade_orders + cascade_users must compose (#698)"
node compose-check.mjs \
  "cascade_orders=$fixtures/cascade_orders.graphql" \
  "cascade_users=$fixtures/cascade_users.graphql"

echo "→ default literals: graphql-js keeps every published default (#1888)"
node default-literals-check.mjs \
  "$fixtures/default_literals.graphql" \
  "$fixtures/default_literals.introspection.json" \
  'mutation { createItem(input: {name: "x"}) { id } }'

echo "✓ federation compose check passed"
