# Phase 02: SDK CI Fixes + REST Annotation Parity

## Status
[ ] Not Started

## Objective
Fix the three remaining SDK CI failures and add REST transport annotations
to the PHP, Elixir, and F# SDKs (issue #85).

## Dependencies
- Phase 01 (nightly fmt) must be complete (Format Check blocks all CI)

---

## Cycle 1 — Go SDK: generate go.sum

### Problem
`sdks/official/fraiseql-go/go.sum` does not exist. The CI workflow references
it in two places:
- `cache-dependency-path: sdks/official/fraiseql-go/go.sum` (step fails to find file)
- `go mod verify` (fails with no go.sum)

The multiple `package main` files in `examples/` are **not** a bug — they live in
separate directories and are separate binaries. This issue was resolved by a prior
refactor.

### Fix
```bash
cd sdks/official/fraiseql-go
go mod download       # fetches deps and writes go.sum
go mod verify         # confirms checksums
go vet ./...          # smoke test
```

### Verification
```bash
test -f sdks/official/fraiseql-go/go.sum   # file must exist
cd sdks/official/fraiseql-go && go vet ./... && go test ./...
```

---

## Cycle 2 — REST annotation parity: PHP, Elixir, F# (issue #85)

### Problem
REST transport annotations (`rest_path`, `rest_method`) are missing from three
SDKs. Current status:

| SDK | REST annotations |
|-----|-----------------|
| Python | ✅ `@fraiseql.query(rest_path=…, rest_method=…)` |
| TypeScript | ✅ `restPath`, `restMethod` in `decorators.ts` (line 112/114) |
| Go | ✅ `RestPath`, `RestMethod` in struct tags |
| C# | ✅ `.RestPath()`, `.RestMethod()` builder methods |
| Java | ✅ `restPath`, `restMethod` in annotation |
| PHP | ❌ missing |
| Elixir | ❌ missing |
| F# | ❌ missing |
| Rust SDK | ❌ missing (authoring SDK only, low priority) |

### Fix — PHP (`sdks/official/fraiseql-php/src/`)

Add `rest_path` and `rest_method` optional parameters to `QueryBuilder` and
`MutationBuilder`, emitted as `"rest": {"path": …, "method": …}` in the
generated JSON schema. Pattern: follow Python SDK decorators.py exactly.

```php
// QueryBuilder.php — add fluent methods:
public function restPath(string $path): static
public function restMethod(string $method): static  // GET|POST|PUT|PATCH|DELETE
```

JSON output field: `"rest": {"path": "/users/{id}", "method": "GET"}`.

### Fix — Elixir (`sdks/official/fraiseql-elixir/lib/`)

Add `rest_path` and `rest_method` keyword options to `fraiseql_query/2` and
`fraiseql_mutation/2` macros. Emit under `"rest"` key in the JSON map.

### Fix — F# (`sdks/official/fraiseql-fsharp/src/`)

Add `RestPath` and `RestMethod` optional parameters to the `Query` and
`Mutation` builder types. Emit under `"rest"` in the serialised schema.

### Verification
For each SDK: add a test asserting that a query with `rest_path="/users/{id}"`
and `rest_method="GET"` serialises to JSON containing `"rest":{"path":"/users/{id}","method":"GET"}`.

---

## Success Criteria
- [ ] `go.sum` present in `sdks/official/fraiseql-go/` and committed
- [ ] `cd sdks/official/fraiseql-go && go mod verify && go test ./...` passes
- [ ] PHP: `QueryBuilder::restPath/restMethod` methods exist and are tested
- [ ] Elixir: `fraiseql_query/2` accepts `rest_path:` / `rest_method:` opts
- [ ] F#: `Query`/`Mutation` builders accept `RestPath`/`RestMethod`
- [ ] Each SDK has ≥1 test covering REST annotation serialisation
- [ ] CI workflows for Go, PHP, Elixir, F# pass

## Closes
- Issue #85 (REST annotation parity — PHP, Elixir, F# remaining)
