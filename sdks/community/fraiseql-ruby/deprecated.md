# Deprecated

This SDK has been deprecated and is no longer actively maintained.

**Deprecated since**: v2.0.0
**Last compatible schema version**: v1.x
**v2.0.0 compatibility**: Not supported. The v2 compiled schema format (`schema.compiled.json`) is not compatible with this SDK.

## Recommended Alternative

Use the FraiseQL HTTP/GraphQL API directly via a Ruby GraphQL client (e.g., `graphql-client` gem).

## Reason

Ruby web frameworks typically consume APIs over HTTP. FraiseQL's standard GraphQL-over-HTTP interface works well with existing Ruby GraphQL clients, making a dedicated SDK unnecessary.

## Migration

1. Remove the `fraiseql-ruby` gem.
2. Add `graphql-client` (or `graphlient`) to your `Gemfile`.
3. Point your GraphQL client at the FraiseQL server endpoint.
4. All queries and mutations work over standard GraphQL HTTP — no schema changes required.
