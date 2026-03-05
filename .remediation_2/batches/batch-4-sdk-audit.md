# Batch 4 — SDK Audit and Parity

## Problem

11 official SDK CI workflows were added in a single batch commit (`481e56650`).
A spot-check reveals that some run only `build` steps without meaningful
functional tests. Community SDKs have two test files each (export-types,
scope-extraction) which is a minimal baseline but not a functional contract.

## Findings Per SDK

### Official SDKs

| SDK | CI file | Test command | Functional tests found? |
|-----|---------|-------------|------------------------|
| Python | `python-sdk.yml` | `uv run pytest tests -v` | ✅ Yes — 10 test files including `test_decorators.py`, `test_golden_schema.py` |
| TypeScript | `typescript-sdk.yml` | `jest` (verify) | Needs verification — `jest.config.js` exists |
| Go | `go-sdk.yml` | `go build ./...` | ❌ No test directory found — CI runs only `go build` |
| Java | `java-sdk.yml` | (check file) | Needs verification |
| PHP | `php-sdk.yml` | (check file) | Needs verification |
| Rust | `rust-sdk.yml` | `cargo test` | ✅ Likely yes |
| C# | `csharp-sdk.yml` | `dotnet test` | ✅ CI runs `dotnet test` — needs content check |
| Ruby | `ruby-sdk.yml` | (check file) | Needs verification |
| Dart | `dart-sdk.yml` | (check file) | Needs verification |
| F# | `fsharp-sdk.yml` | (check file) | Needs verification |
| Elixir | `elixir-sdk.yml` | (check file) | Partial — community version has tests |

### Community SDKs

All have exactly 2 test files:
- `export_types_test.*`
- `scope_extraction_test.*`

These test data model construction only, not the full decorator → schema.json pipeline.

### Elixir/Dart Duplication

Both `sdks/official/fraiseql-elixir/` and `sdks/community/fraiseql-elixir/`
exist. Same for Dart. One is authoritative and one is stale. This needs a
decision and cleanup.

---

## Required Actions

### SDK-1 — Audit all official SDK CI workflows

For each of the 11 SDK workflow files, verify that:
1. The `test` step runs a test runner (not just `build`)
2. The test runner is invoked with a non-zero timeout
3. At least one test file exists in the SDK's `tests/` directory

**Audit checklist** (fill in after inspection):

| SDK | Build-only? | At least 1 functional test? | Action needed |
|-----|------------|----------------------------|---------------|
| Python | No | Yes | — |
| TypeScript | (check) | (check) | — |
| Go | Yes | No | SDK-2 |
| Java | (check) | (check) | — |
| PHP | (check) | (check) | — |
| Rust | No | Yes | — |
| C# | No | (check content) | — |
| Ruby | (check) | (check) | — |
| Dart | (check) | (check) | SDK-4 |
| F# | (check) | (check) | — |
| Elixir | (check) | Partial | SDK-4 |

---

### SDK-2 — Go SDK: add minimum viable test suite

The Go SDK currently only has `go build ./...` in CI. Add:

```
sdks/official/fraiseql-go/fraiseql/
├── decorator_test.go    ← new
├── schema_export_test.go ← new
└── scope_test.go        ← new
```

**`decorator_test.go`** (minimum required):
```go
package fraiseql_test

import (
    "encoding/json"
    "testing"
    "github.com/fraiseql/fraiseql-go/fraiseql"
)

func TestTypeDecoratorRegistersType(t *testing.T) {
    registry := fraiseql.NewSchemaRegistry()
    // Register a type
    registry.RegisterType("User", fraiseql.TypeDef{
        Fields: []fraiseql.FieldDef{
            {Name: "id", Type: "ID", Required: true},
            {Name: "name", Type: "String"},
        },
    })
    // Export to JSON
    schema, err := registry.Export()
    if err != nil {
        t.Fatalf("Export failed: %v", err)
    }
    // Verify the type appears in output
    var output map[string]any
    if err := json.Unmarshal(schema, &output); err != nil {
        t.Fatalf("Export is not valid JSON: %v", err)
    }
    types, ok := output["types"].([]any)
    if !ok || len(types) == 0 {
        t.Error("Expected at least one type in schema export")
    }
}
```

Update `go-sdk.yml` to run `go test ./...`:
```yaml
- name: Test
  run: go test ./... -v -race -timeout 60s
```

---

### SDK-3 — Community SDKs: add schema roundtrip test

Each community SDK (clojure, groovy, kotlin, scala, swift, nodejs) must add
one test that exercises the full decorator → export pipeline, not just data
model construction.

**Minimum test contract** (adapt to each language):

```
Given: A schema with one @type, one @query, one @mutation
When:  Registry.export() is called
Then:  The JSON output matches a known-good fixture (golden test)
       AND all three entries appear at the correct paths
       AND the output is parseable as the schema.json format
```

This test is the contract between the SDK and the compiler. If the SDK produces
malformed JSON, the compiler will reject it — but today that failure is silent
during SDK development.

---

### SDK-4 — Resolve Elixir/Dart duplication

**Elixir**: `sdks/official/fraiseql-elixir/` and `sdks/community/fraiseql-elixir/`
**Dart**: `sdks/official/fraiseql-dart/` and `sdks/community/fraiseql-dart/`

Actions:
1. Compare the two versions of each (file count, last commit date, test depth)
2. Archive the stale one by moving it to `sdks/archived/` with a README
3. Update the CI workflow to point only at the authoritative version
4. Update the SDK list in the main README

---

### SDK-5 — Cross-SDK parity CI job (optional but high-value)

Add `.github/workflows/sdk-parity.yml`:

```yaml
name: SDK Schema Parity

on:
  push:
    paths:
      - 'sdks/official/**'
      - 'examples/basic/**'

jobs:
  parity:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Generate schema via Python SDK
        working-directory: sdks/official/fraiseql-python
        run: |
          uv run python examples/basic/schema.py --output /tmp/schema_python.json

      - name: Generate schema via TypeScript SDK
        working-directory: sdks/official/fraiseql-typescript
        run: |
          npx ts-node examples/basic/schema.ts --output /tmp/schema_ts.json

      - name: Compare outputs
        run: |
          python3 -c "
          import json, sys
          with open('/tmp/schema_python.json') as f: p = json.load(f)
          with open('/tmp/schema_ts.json') as f: t = json.load(f)
          if p != t:
              import pprint
              print('Python output:', pprint.pformat(p))
              print('TypeScript output:', pprint.pformat(t))
              sys.exit(1)
          print('SDK parity check passed')
          "
```

---

## Verification Checklist

- [ ] Go SDK CI runs `go test ./...` (not just `go build`)
- [ ] Go SDK has at least 3 test functions
- [ ] All 11 official SDK `test` CI steps invoke a test runner, not just a build step
- [ ] `sdks/official/fraiseql-elixir/` or `sdks/community/fraiseql-elixir/` is archived
- [ ] Same for Dart
- [ ] Community SDKs each have a schema roundtrip golden test
- [ ] SDK-5 parity job passes on current codebase
