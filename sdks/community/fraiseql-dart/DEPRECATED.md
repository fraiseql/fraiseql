# Deprecated

This SDK has been deprecated and is no longer actively maintained.

**Deprecated since**: v2.0.0
**Last compatible schema version**: v1.x
**v2.0.0 compatibility**: Not supported. The v2 compiled schema format (`schema.compiled.json`) is not compatible with this SDK.

## Recommended Alternative

Use the FraiseQL HTTP/GraphQL API directly via the `graphql` Dart package or `Ferry` for Flutter.

## Reason

FraiseQL is a server-side database engine — Flutter/Dart clients connect to it over the network via HTTP. A dedicated Dart SDK is not necessary; any standard Dart GraphQL client works out of the box.

## Migration

1. Remove the `fraiseql_dart` pub dependency.
2. Add `graphql` or `ferry` to your `pubspec.yaml`.
3. Point the client at your FraiseQL server endpoint.
4. All queries and mutations work over standard GraphQL HTTP.
