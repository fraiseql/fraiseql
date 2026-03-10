# Deprecated

This SDK has been deprecated and is no longer actively maintained.

**Deprecated since**: v2.0.0
**Last compatible schema version**: v1.x
**v2.0.0 compatibility**: Not supported. The v2 compiled schema format (`schema.compiled.json`) is not compatible with this SDK.

## Recommended Alternative

Use the FraiseQL HTTP/GraphQL API directly via `Neuron` or `AbsintheClient` for Elixir.

## Reason

Elixir's ecosystem for consuming external GraphQL APIs is mature. FraiseQL's standard GraphQL-over-HTTP interface works well with existing Elixir HTTP clients, making a dedicated SDK unnecessary.

## Migration

1. Remove the `:fraiseql` hex dependency.
2. Add `neuron` or `tesla` to your `mix.exs`.
3. Point the HTTP client at your FraiseQL server endpoint.
4. All queries and mutations work over standard GraphQL HTTP.
