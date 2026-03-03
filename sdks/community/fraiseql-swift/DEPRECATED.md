# Deprecated

This SDK has been deprecated and is no longer actively maintained.

**Deprecated since**: v2.0.0
**Last compatible schema version**: v1.x
**v2.0.0 compatibility**: Not supported. The v2 compiled schema format (`schema.compiled.json`) is not compatible with this SDK.

## Recommended Alternative

Use the FraiseQL HTTP/GraphQL API directly via `URLSession` or a Swift GraphQL client (e.g., `Apollo iOS`).

## Reason

FraiseQL is a server-side database engine — iOS/macOS clients connect to it over the network via HTTP. A dedicated Swift SDK is not necessary; any standard GraphQL client for Swift works out of the box.

## Migration

1. Remove the `fraiseql-swift` Swift Package dependency.
2. Add `Apollo iOS` or `Graphaello` to your project.
3. Point the client at your FraiseQL server endpoint.
4. All queries and mutations work over standard GraphQL HTTP.
