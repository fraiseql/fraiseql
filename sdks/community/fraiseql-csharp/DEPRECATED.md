# Deprecated

This SDK has been deprecated and is no longer actively maintained.

**Deprecated since**: v2.0.0
**Last compatible schema version**: v1.x
**v2.0.0 compatibility**: Not supported. The v2 compiled schema format (`schema.compiled.json`) is not compatible with this SDK.

## Recommended Alternative

Use the FraiseQL HTTP/GraphQL API directly via any .NET HTTP client (e.g., `HttpClient`, `HotChocolate.Client`, or `GraphQL.Client`).

## Reason

.NET is not a priority target for a dedicated SDK. FraiseQL exposes a standard GraphQL-over-HTTP interface that any .NET GraphQL client can consume without a dedicated SDK.

## Migration

1. Remove the `fraiseql-csharp` NuGet package.
2. Add a standard .NET GraphQL client (e.g., `GraphQL.Client` or `Strawberry Shake`).
3. Point it at your FraiseQL server endpoint (e.g., `https://api.example.com/graphql`).
4. All queries, mutations, and subscriptions work over the standard GraphQL HTTP protocol — no SDK changes required in your schema definitions.
