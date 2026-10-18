#!/usr/bin/env bash
# check-graphql-parse-sites.sh — pin the production sites that parse a GraphQL
# executable document.
#
# WHY THIS GATE EXISTS
# --------------------
# A client controls the document, so the parser is attack surface. #976 was a
# panic inside graphql-parser 0.4 on a 27-byte document (a block string indented
# with U+00A0, sliced mid-codepoint), and when it was found there were SIX parse
# sites, one of them invisible to the obvious grep because it imported the module
# and called `query::parse_query`.
#
# Since #1567's review the parser is apollo-parser, read in exactly one place:
#
#   crates/fraiseql-core/src/graphql/document.rs   the only caller of the parser,
#                                                  converting its tree into
#                                                  fraiseql's own `Document`
#   crates/fraiseql-core/src/graphql/complexity.rs `parse_graphql_document`, the
#                                                  only caller of `document::parse`:
#                                                  the panic guard and the
#                                                  request's shared parse (#1562)
#
# Every other site reads a `Document`, so a second parse site (or a second reader
# of a parser's tree) is a guard that can be walked around and a parse that is not
# shared. Both spellings of each call are matched.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Production code only. Test files, fuzz targets and benches may parse directly:
# they are not request paths, and a fuzz target MUST call the raw parser to be
# able to find the next panic in it.
production_hits() {
  local pattern=$1 allowed=$2
  grep -rEn "$pattern" crates/*/src --include='*.rs' \
    | grep -vE '/(tests|[a-z_]+_tests)\.rs:' \
    | grep -vE '^[^:]*/tests/' \
    | grep -vE "^(${allowed}):" \
    | grep -vE ':[0-9]+:\s*(//|///|//!)' \
    || true
}

status=0

# The parser itself: `apollo_parser::Parser` / `Parser::new`, and any other GraphQL
# parser crate a future change might add next to it.
parser=$(production_hits \
  '(apollo_parser|graphql_parser|cynic_parser|async_graphql_parser)::|\bParser::new\(' \
  'crates/fraiseql-core/src/graphql/document\.rs|crates/fraiseql-core/src/graphql/value_json\.rs')
if [ -n "$parser" ]; then
  echo "ERROR: a GraphQL parser is used outside graphql/document.rs:"
  echo "$parser"
  echo
  echo "Read a document through fraiseql_core::graphql::parse_graphql_document and"
  echo "walk its fraiseql_core::graphql::document::Document."
  status=1
fi

# The converter: only the guarded, shared seam calls it.
seam=$(production_hits \
  '\bdocument::parse\(' \
  'crates/fraiseql-core/src/graphql/complexity\.rs')
if [ -n "$seam" ]; then
  echo "ERROR: graphql::document::parse called outside parse_graphql_document:"
  echo "$seam"
  echo
  echo "Call fraiseql_core::graphql::parse_graphql_document: it guards the parser"
  echo "and shares one parse per request (#1562)."
  status=1
fi

[ "$status" -eq 0 ] && echo "OK: one GraphQL parse site, behind the guarded seam"
exit "$status"
