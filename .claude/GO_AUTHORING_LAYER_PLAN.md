# FraiseQL Go Authoring Layer - Implementation Plan

## Overview

Create a Go package (`fraiseql-go`) for schema authoring that mirrors the functionality of existing Python and TypeScript implementations. The package generates `schema.json` files without runtime FFI, following the compile-to-JSON architecture.

**Architecture**:

```
Go Code (struct tags) → schema.json → fraiseql-cli compile → schema.compiled.json → Rust runtime
```

## Key Design Decisions

### 1. Go-Idiomatic API (vs Direct Copy from Python/TS)

**Option: Hybrid Approach (Recommended)**

- Use **struct tags** for type definitions (Go standard pattern)
- Use **builder pattern** for queries/mutations (Go convention)
- Leverage Go's reflection for runtime metadata extraction
- Keep JSON export simple and testable

**Why**:

- Go developers expect tags (`json:`, `db:`, etc.)
- Builder pattern is idiomatic for complex configuration
- Go's reflection can extract type info at build time
- Easier to integrate with existing Go tooling (code generation)

### 2. Module Structure

```
fraiseql-go/
├── go.mod
├── go.sum
├── Makefile
├── README.md
├── LICENSE
│
├── cmd/
│   └── schema-export/          # CLI tool for schema export
│       └── main.go
│
├── fraiseql/
│   ├── decorators.go           # Builder functions for queries/mutations
│   ├── registry.go             # Global schema registry
│   ├── types.go                # Type conversion (Go → GraphQL)
│   ├── schema.go               # Schema export to JSON
│   └── analytics.go            # Fact tables and aggregates
│
├── examples/
│   ├── basic_schema.go         # Basic types, queries, mutations
│   └── analytics_schema.go     # Fact tables example
│
├── tests/
│   ├── decorators_test.go
│   ├── types_test.go
│   └── schema_test.go
│
└── testdata/
    └── expected_schemas/       # Golden test files
```

### 3. API Design

**Type Definition** (using struct tags):

```go
type User struct {
    ID        int       `fraiseql:"id,type=Int"`
    Name      string    `fraiseql:"name,type=String"`
    Email     string    `fraiseql:"email,type=String"`
    CreatedAt time.Time `fraiseql:"created_at,type=String"`
    IsActive  *bool     `fraiseql:"is_active,type=Boolean,nullable=true"`
}
```

**Query Definition** (using builder):

```go
fraiseql.NewQuery("users").
    ReturnType(User{}).
    ReturnsArray(true).
    Config(map[string]interface{}{
        "sql_source": "v_user",
        "auto_params": map[string]bool{
            "limit":     true,
            "offset":    true,
            "where":     true,
            "order_by":  true,
        },
    }).
    Arg("limit", "Int", 10). // type, defaultValue
    Arg("offset", "Int", 0).
    Arg("is_active", "Boolean", nil, true). // nullable
    Description("Get list of users with pagination").
    Register()
```

**Mutation Definition** (using builder):

```go
fraiseql.NewMutation("createUser").
    ReturnType(User{}).
    Config(map[string]interface{}{
        "sql_source": "fn_create_user",
        "operation": "CREATE",
    }).
    Arg("name", "String", nil).
    Arg("email", "String", nil).
    Description("Create a new user").
    Register()
```

**Schema Export**:

```go
func main() {
    if err := fraiseql.RegisterTypes(User{}, Post{}); err != nil {
        log.Fatal(err)
    }

    if err := fraiseql.ExportSchema("schema.json"); err != nil {
        log.Fatal(err)
    }
}
```

## Implementation Steps

### Phase 1: Core Infrastructure (Step 1-2)

**Step 1: Project Setup & Module Structure**

- Create `fraiseql-go/` directory at `/home/lionel/code/fraiseql/fraiseql-go/`
- Initialize Go module: `go.mod` with version 2.0.0-alpha.1
- Create directory structure (fraiseql/, examples/, tests/)
- Create README.md with installation and quick start
- Create Makefile with build, test, lint targets
- Add LICENSE (MIT)

**Files to create**:

- `go.mod`
- `go.sum` (empty initially)
- `Makefile`
- `README.md`
- `LICENSE`
- `.gitignore`
- Directory structure

**Step 2: Type System & Conversion**

- `fraiseql/types.go`: Go → GraphQL type conversion
  - `GoToGraphQLType(reflect.Type)` → (string, bool) // (type, nullable)
  - Type mappings: int→Int, string→String, bool→Boolean, float64→Float
  - Nullable detection: *Type, *sql.Null*, etc.
  - Complex types: arrays ([]T), maps validation
  - Custom type (struct) handling
- Unit tests for all type conversions
- Document supported types in README

**Files to create**:

- `fraiseql/types.go`
- `fraiseql/types_test.go`
- Update `README.md` with type mapping table

### Phase 2: Registry & Decorators (Step 3-4)

**Step 3: Global Schema Registry**

- `fraiseql/registry.go`: Singleton registry pattern
  - `SchemaRegistry` struct with maps for types, queries, mutations, fact_tables
  - `RegisterType(name, fields, description)`
  - `RegisterQuery(name, config)`
  - `RegisterMutation(name, config)`
  - `GetSchema()` → complete schema dict
  - Reset() for testing
- Unit tests for registration logic

**Files to create**:

- `fraiseql/registry.go`
- `fraiseql/registry_test.go`

**Step 4: Query & Mutation Builders**

- `fraiseql/decorators.go`: Builder pattern for operations
  - `QueryBuilder` struct with chainable methods
    - `ReturnType(any)`
    - `ReturnsArray(bool)`
    - `Config(map[string]interface{})`
    - `Arg(name, type, defaultValue, *nullable)`
    - `Description(string)`
    - `Register()`
  - `MutationBuilder` struct (same pattern)
  - `NewQuery(name)` → *QueryBuilder
  - `NewMutation(name)` → *MutationBuilder
- Unit tests for builder pattern
- Test argument order preservation

**Files to create**:

- `fraiseql/decorators.go`
- `fraiseql/decorators_test.go`

### Phase 3: Type Registration & Export (Step 5-6)

**Step 5: Struct Field Extraction**

- Add to `fraiseql/types.go`:
  - `ExtractFields(any) → map[string]FieldInfo`
  - Uses reflection to read struct fields
  - Reads `fraiseql` struct tags for metadata
  - Validates all fields have type info (tag or inferred)
  - Tag format: `fraiseql:"fieldname,type=Int,nullable=true"`
- Unit tests for field extraction
- Handle edge cases (embedded structs, unexported fields)

**Step 6: Schema Export to JSON**

- `fraiseql/schema.go`:
  - `RegisterTypes(types ...any)` → error
  - `ExportSchema(path string)` → error
  - Validates schema before export
  - Pretty-prints JSON with 2-space indent
  - Prints summary (# types, # queries, # mutations)
  - Error handling for file I/O
- Integration test: export schema and validate JSON structure
- Test comparison with Python/TS output

**Files to create**:

- Update `fraiseql/types.go` with field extraction
- `fraiseql/schema.go`
- `fraiseql/schema_test.go`
- `testdata/expected_schemas/basic.json`

### Phase 4: Analytics Support (Step 7)

**Step 7: Fact Tables & Aggregate Queries**

- Add to `fraiseql/analytics.go`:
  - `FactTableBuilder` for defining fact tables
  - `AggregateQueryBuilder` for aggregate queries
  - Follow same pattern as Python/TS analytics
- `NewFactTable(name, tableName)` → *FactTableBuilder
  - `Measures(names ...string)`
  - `DimensionPaths(paths ...map[string]string)`
  - `Register()`
- `NewAggregateQuery(name)` → *AggregateQueryBuilder
  - `FactTable(name)`
  - `AutoGroupBy(bool)`
  - `AutoAggregates(bool)`
  - `Register()`
- Unit tests matching Python analytics tests

**Files to create**:

- `fraiseql/analytics.go`
- `fraiseql/analytics_test.go`
- `examples/analytics_schema.go`

### Phase 5: Examples & Documentation (Step 8-9)

**Step 8: Basic Schema Example**

- `examples/basic_schema.go`:
  - Define User and Post types (same as Python example)
  - Define all queries (users, user, posts)
  - Define all mutations (create_user, update_user, delete_user, create_post)
  - Export schema.json
  - Can be run: `go run examples/basic_schema.go`
- Include copy-paste instructions for using with fraiseql-cli

**Step 9: Analytics Schema Example**

- `examples/analytics_schema.go`:
  - Define Sale fact table with measures and dimensions
  - Define aggregate query
  - Export to schema.json
  - Matches Python analytics example

**Files to create**:

- `examples/basic_schema.go`
- `examples/analytics_schema.go`
- Update `README.md` with examples section

### Phase 6: Testing & Package Publishing (Step 10-11)

**Step 10: Comprehensive Test Suite**

- `fraiseql/decorators_test.go`: Test all builder methods
- `fraiseql/types_test.go`: Type conversion edge cases
- `fraiseql/schema_test.go`: Integration tests
- `fraiseql/analytics_test.go`: Analytics features
- All tests use testdata/ for golden file comparisons
- Achieve 85%+ code coverage
- Run with: `make test`, `make coverage`

**Step 11: Go Module Packaging**

- Finalize `go.mod` with correct versions
- Add `.gitignore` for Go artifacts
- Create `cmd/schema-export/main.go` (optional CLI)
- Add installation instructions to README
- Add go.pkg.dev badge and documentation

**Files to update**:

- Finalize `go.mod`
- Add `go.sum` with all dependencies
- Create `.gitignore`

## Expected Schema JSON Output

When users run `go run schema.go`, the output should match this structure:

```json
{
  "types": [
    {
      "name": "User",
      "fields": [
        {"name": "id", "type": "Int", "nullable": false},
        {"name": "name", "type": "String", "nullable": false},
        {"name": "email", "type": "String", "nullable": false},
        {"name": "created_at", "type": "String", "nullable": false},
        {"name": "is_active", "type": "Boolean", "nullable": true}
      ],
      "description": "User type"
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true,
      "nullable": false,
      "arguments": [
        {"name": "limit", "type": "Int", "nullable": false, "default": 10},
        {"name": "offset", "type": "Int", "nullable": false, "default": 0}
      ],
      "description": "Get list of users",
      "sql_source": "v_user",
      "auto_params": {"limit": true, "offset": true}
    }
  ],
  "mutations": [
    {
      "name": "createUser",
      "return_type": "User",
      "returns_list": false,
      "nullable": false,
      "arguments": [
        {"name": "name", "type": "String", "nullable": false},
        {"name": "email", "type": "String", "nullable": false}
      ],
      "description": "Create a new user",
      "sql_source": "fn_create_user",
      "operation": "CREATE"
    }
  ]
}
```

## Verification Checklist

For each phase:

- [ ] `go build ./...` succeeds with no warnings
- [ ] `go test -v ./...` passes all tests
- [ ] `golangci-lint run` clean (if available)
- [ ] Generated schema.json matches expected output
- [ ] Examples run without errors
- [ ] README has correct syntax examples

## Success Criteria

1. **Functionality**: Package generates identical schema.json as Python/TS
2. **API Design**: Go-idiomatic (struct tags, builder pattern)
3. **Documentation**: README with quick start, examples, API reference
4. **Testing**: 85%+ code coverage, all examples runnable
5. **Quality**: No Go linting issues, passes `go vet`
6. **Compatibility**: Works with fraiseql-cli compile and fraiseql-server

## File Count & Complexity

**Core Package**:

- 5 main files (types, registry, decorators, schema, analytics)
- ~800-1200 lines of code
- Moderate complexity (similar to Python/TS)

**Tests & Examples**:

- 5 test files (~600 lines)
- 2 example files (~150 lines)
- 5 testdata golden files

**Total**: ~2000 lines (comparable to Python/TS implementations)

## Next Steps After Approval

1. Create directory structure and go.mod
2. Implement Phase 1 (infrastructure)
3. Implement Phase 2 (registry & builders)
4. Implement Phase 3 (type extraction & export)
5. Implement Phase 4 (analytics)
6. Implement Phase 5 (examples)
7. Implement Phase 6 (testing & packaging)
8. Run full verification
9. Commit all changes with descriptive message
