# Contributing to FraiseQL Go

Thank you for considering contributing to the FraiseQL Go authoring library! This guide will help you get started.

## Development Setup

### Prerequisites

- Go 1.22 or later
- Make (for running build tasks)
- Git

### Building and Testing

```bash
# Install dependencies
go mod download

# Run tests
make test

# Run tests with verbose output
make test-verbose

# Run linter
make lint

# Build documentation
make doc
```

## Project Structure

```
fraiseql-go/
├── fraiseql/              # Main package
│   ├── types.go          # Type conversion logic
│   ├── types_test.go     # Type conversion tests
│   ├── registry.go       # Schema registry
│   ├── registry_test.go  # Registry tests
│   ├── decorators.go     # Query/Mutation builders
│   ├── decorators_test.go # Builder tests
│   ├── schema.go         # Schema export
│   ├── analytics.go      # Fact tables and queries
│   └── analytics_test.go # Analytics tests
├── examples/             # Example programs
│   ├── basic_schema.go
│   ├── analytics_schema.go
│   ├── complete_schema.go
│   └── README.md
├── go.mod               # Module definition
├── Makefile            # Build tasks
├── README.md           # Documentation
└── LICENSE             # Apache 2.0
```

## Architecture Overview

FraiseQL Go follows these design principles:

1. **Authoring Only**: Go is used only for schema authoring, not runtime execution
2. **Pure JSON Output**: All schema definitions are exported as JSON, consumed by the Rust compiler
3. **Builder Pattern**: Fluent APIs for defining types, queries, mutations, and fact tables
4. **Thread-Safe Registry**: Schema definitions are collected in a singleton registry with RWMutex
5. **Struct Introspection**: Go reflection extracts field information from structs

## Code Standards

### Type Conversion

All Go types must map to GraphQL types. Update `types.go` with any new type conversions:

```go
case *string:
    return "String", true  // type, nullable
```

### Builder Pattern

Follow the existing pattern for new builders:

```go
type MyBuilder struct {
    // fields
}

func NewMyBuilder(name string) *MyBuilder {
    return &MyBuilder{
        // initialize
    }
}

func (mb *MyBuilder) SomeOption(value string) *MyBuilder {
    mb.field = value
    return mb  // return for chaining
}

func (mb *MyBuilder) Register() {
    // Register with global registry
}
```

### Testing

Write tests for all new functionality:

```go
func TestMyFeature(t *testing.T) {
    defer Reset()  // Clear registry after test

    // Test implementation
    assert.Equal(t, expected, actual)
}
```

Use subtests for grouped test cases:

```go
func TestTypes(t *testing.T) {
    tests := []struct {
        name     string
        input    interface{}
        expected string
    }{
        {
            name:     "int type",
            input:    42,
            expected: "Int",
        },
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            // Test
        })
    }
}
```

## Making Changes

### Before You Start

1. Check if there's an existing issue or discussion
2. Create an issue to discuss major changes
3. Ensure tests pass before starting: `make test`

### During Development

1. Make focused changes (one feature per branch)
2. Write tests for new code
3. Run `make test` frequently
4. Keep commits small and descriptive

### Pull Request Checklist

- [ ] Tests pass: `make test`
- [ ] Linter passes: `make lint`
- [ ] Documentation updated if needed
- [ ] Commit message is clear and descriptive
- [ ] PR title summarizes the change

## Testing Strategy

### Unit Tests

Test individual functions and builders:

```go
func TestNewQuery(t *testing.T) {
    qb := NewQuery("users")
    if qb.name != "users" {
        t.Errorf("expected name 'users', got %q", qb.name)
    }
}
```

### Integration Tests

Test registry and schema generation:

```go
func TestRegistration(t *testing.T) {
    defer Reset()

    NewQuery("users").
        ReturnType(User{}).
        Register()

    schema := GetSchema()
    if len(schema.Queries) != 1 {
        t.Errorf("expected 1 query, got %d", len(schema.Queries))
    }
}
```

### Example Tests

Run examples to verify they work:

```bash
cd examples
go run basic_schema.go
```

## Common Tasks

### Adding a New Type Conversion

1. Edit `types.go` in the `goToGraphQLType` function
2. Add test case in `types_test.go`
3. Update type table in `README.md`
4. Run tests: `make test`

### Adding a New Builder

1. Create struct in appropriate file (decorators.go, analytics.go, etc.)
2. Implement builder methods returning `*Builder`
3. Implement `Register()` method
4. Add tests in corresponding `_test.go` file
5. Update `README.md` with API reference
6. Test with an example

### Modifying the Registry

1. Update `registry.go` and `registry_test.go`
2. Update schema export logic in `schema.go`
3. Test with example generation: `cd examples && go run *.go`
4. Verify schema.json structure is correct

## Documentation

### Updating README

- Keep the Quick Start section concise
- Update Type System table for new types
- Update API Reference for new builders
- Add examples to Features section

### Code Documentation

Use godoc format for exported functions:

```go
// NewQuery creates a new query builder
func NewQuery(name string) *QueryBuilder {
    // ...
}
```

## Performance Considerations

1. **Registry**: Uses RWMutex for thread-safe access
2. **Reflection**: Only used during schema registration, not at runtime
3. **JSON Export**: Uses encoding/json for efficient serialization
4. **No External Dependencies**: Keep dependency count minimal

## Debugging

### Enable Logging

Add debug output temporarily:

```go
log.Printf("DEBUG: field=%s type=%s", field.Name, gqlType)
```

### Test Individual Functions

```bash
go test ./fraiseql -run TestMyFeature -v
```

### Check Generated Schema

Run an example and inspect the output:

```bash
cd examples
go run basic_schema.go
cat schema.json | jq .  # Pretty print with jq
```

## Submitting Changes

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/your-feature`
3. Make changes and commit: `git commit -am "feat: description"`
4. Push to your fork: `git push origin feature/your-feature`
5. Create a Pull Request

## Code Review Process

- PRs should pass CI (tests + linting)
- Clear commit messages and PR descriptions
- One approval before merging
- Squash commits if needed for clarity

## License

By contributing, you agree that your contributions will be licensed under the Apache 2.0 License.

## Questions?

- Check existing issues and discussions
- Review examples in `examples/` directory
- Read the main `README.md` for API reference

## Roadmap

FraiseQL Go is part of the larger FraiseQL v2 project:

1. ✅ Phase 1-5: Core type system, registry, decorators, analytics
2. ⏳ Phase 6: Documentation and examples
3. ⏳ Phase 7: Integration tests and package publishing

See `.claude/GO_AUTHORING_LAYER_PLAN.md` in the main FraiseQL repository for detailed implementation phases.
