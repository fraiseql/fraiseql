# Archived SDKs

This directory contains SDK implementations that have been superseded or
deprecated. They are preserved for historical reference only and should
not be used in new projects.

## Contents

### `fraiseql-elixir-community/`

Community Elixir SDK — deprecated as of FraiseQL v2.0.0.

**Authoritative replacement**: `sdks/official/fraiseql-elixir/`

The community version targeted the v1.x compiled schema format, which is
no longer supported. Elixir/Phoenix applications should connect to a
FraiseQL v2 server over HTTP/GraphQL using mature Elixir HTTP clients
(Neuron, Tesla, etc.) — a dedicated server-side SDK is unnecessary.

Last compatible version: FraiseQL 1.x
Archived: 2026-03-05 (Remediation Campaign 2, SDK-4)

---

### `fraiseql-dart-community/`

Community Dart SDK — deprecated as of FraiseQL v2.0.0.

**Authoritative replacement**: `sdks/planned/fraiseql-dart/`

FraiseQL is a server-side execution engine. Dart/Flutter clients connect
to a FraiseQL GraphQL endpoint over HTTP using existing Dart GraphQL
client packages (`graphql`, Ferry). A dedicated authoring SDK is not
needed for client-side use.

Last compatible version: FraiseQL 1.x
Archived: 2026-03-05 (Remediation Campaign 2, SDK-4)
