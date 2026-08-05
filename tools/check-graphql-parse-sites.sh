#!/usr/bin/env bash
# check-graphql-parse-sites.sh — pin the set of production sites that invoke the
# third-party GraphQL parser directly (#976).
#
# WHY THIS GATE EXISTS
# --------------------
# `graphql-parser` 0.4.1 panics on input a client controls. It computes a block
# string's common indent in BYTES (`line.len() - line.trim_start().len()`) but
# strips it with the Unicode-aware `str::trim_start`, then slices the line at that
# byte offset — so a block string indented with U+00A0 slices mid-codepoint and
# panics. A 27-byte document is enough. The crate is unmaintained (last release
# 2022) and the maintained `graphql-parser-hive-fork` carries the identical bug, so
# this is not fixable by upgrading.
#
# `parse_graphql_document` (fraiseql-core, graphql/complexity.rs) is therefore the
# ONE place allowed to call it. It rejects the unsupported indentation before the
# parser sees it, and wraps the rest in `catch_unwind` so an unknown parser panic
# costs one query rather than the connection.
#
# A guard is only worth having if nothing can walk around it. When #976 was found
# there were SIX parse sites: the seam, two more in complexity.rs, the subscription
# name extractor, the REST `validate` endpoint, and `graphql/parser.rs` — and that
# last one was invisible to the obvious `graphql_parser::parse_query` grep because
# it imported the module and called `query::parse_query`. This gate greps for BOTH
# spellings, because the aliased form is the one that hid.
#
# Mirrors the established shell-gate pattern (lint-routes, lint-value-json,
# lint-internal-flag).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# The single permitted seam.
ALLOWED='crates/fraiseql-core/src/graphql/complexity.rs'

# Both spellings of a direct parse call:
#   graphql_parser::parse_query / ::parse_schema      (fully qualified)
#   query::parse_query, query::parse_schema           (via `use graphql_parser::query`)
PATTERN='(graphql_parser|query)::parse_(query|schema)\b'

# Production code only. Test files, fuzz targets and benches may parse directly:
# they are not request paths, and a fuzz target MUST call the raw parser to be
# able to find the next panic in it.
violations=$(
  grep -rEn "$PATTERN" crates/*/src --include='*.rs' \
    | grep -vE '/(tests|[a-z_]+_tests)\.rs:' \
    | grep -vE '^[^:]*/tests/' \
    | grep -vE "^($ALLOWED):" \
    | grep -vE ':[0-9]+:\s*(//|///|//!)' \
    || true
)

if [ -n "$violations" ]; then
  echo "ERROR: GraphQL parser called outside the guarded seam (#976):"
  echo "$violations"
  echo
  echo "The parser panics on client-controlled input. Route this through"
  echo "  fraiseql_core::graphql::complexity::parse_graphql_document"
  echo "or, if this site genuinely must not be guarded, add it to ALLOWED in"
  echo "tools/check-graphql-parse-sites.sh and say why."
  exit 1
fi

echo "OK: the GraphQL parser is only called from the guarded seam"
