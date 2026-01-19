# FraiseQL Security Patterns

This document describes the security patterns and practices used in FraiseQL to prevent common vulnerabilities and maintain data integrity.

## SQL Injection Prevention

### Strategy: Parameterized Queries

All user input is passed to the database as **query parameters**, never interpolated into SQL strings.

#### Examples

##### Column Names (Compile-Time Only)

Column names come from schema definitions, never from user input:

```rust
// Schema defines columns: User { id: ID, name: String }
// These column names are fixed at compile time

// Generated SQL:
// INSERT INTO users (id, name) VALUES ($1, $2)
// Parameters: (user_id, user_name)
```

**Why safe**: Column names are part of the schema definition, fixed at compile time.
User input only provides VALUES, not column names.

##### LIMIT/OFFSET Parameters

Numeric limits are passed as query parameters:

```rust
// User query: "Get 10 items, skip 20"

// Generated SQL:
// SELECT * FROM items LIMIT $1 OFFSET $2
// Parameters: (10, 20)

// Database driver validates numeric types before execution
```

**Why safe**: u32 type ensures only valid integers are accepted. Database driver
never receives a string to parse.

##### JSON Path Extraction

JSON path segments are escaped before inclusion in SQL operators.

Each database handles JSON paths differently:

**PostgreSQL**: Single quotes in path segments are escaped by doubling (`'` → `''`)
within JSONB operators (`->`, `->>`, `->`):

```rust
// Example: path = ["user'name", "email"]
// Escaped: ["user''name", "email"]
// Generated SQL: data->'user''name'->>'email'
```

**MySQL**: Path segments are escaped within `JSON_EXTRACT` and `JSON_UNQUOTE`
function parameters using backslash escaping (`'` → `\'`):

```rust
// Example: path = ["user'name", "email"]
// Escaped: "$.user\'name.email"
// Generated SQL: JSON_UNQUOTE(JSON_EXTRACT(data, '$.user\'name.email'))
```

**SQLite**: Path segments are escaped within `json_extract` function parameters
using backslash escaping (`'` → `\'`):

```rust
// Example: path = ["user'name", "email"]
// Escaped: "$.user\'name.email"
// Generated SQL: json_extract(data, '$.user\'name.email')
```

**SQL Server**: Path segments are escaped within `JSON_VALUE` function parameters
by doubling single quotes (`'` → `''`):

```rust
// Example: path = ["user'name", "email"]
// Escaped: "$.user''name.email"
// Generated SQL: JSON_VALUE(data, '$.user''name.email')
```

**Implementation**: Escaping is applied in the `path_escape` module and consistently
applied across all WHERE clause generators before SQL string interpolation.

**Tested**: Comprehensive injection test suite verifies all common SQL injection
patterns are neutralized by path escaping.

### Rust Type System Protection

The Rust compiler itself prevents entire classes of SQL injection:

1. **Type Safety**: `u32` limit values can't contain SQL strings
2. **Memory Safety**: No buffer overflows or string manipulation bugs
3. **Compiler Warnings**: Unused interpolation flagged by clippy

### Testing Strategy

Security tests verify:
- SQL queries don't contain unescaped user input
- Parameterized queries are used consistently
- Type boundaries are respected

## Thread Safety Patterns

### Pattern: Interior Mutability for Context State

The WHERE clause generator uses `Cell<usize>` for parameter tracking:

```rust
pub struct PostgresWhereGenerator {
    param_counter: std::cell::Cell<usize>,
    // ...
}
```

**Why safe**:
1. **Single-threaded context**: Each WHERE generator is created for a single
   query execution and isn't shared across async tasks.
2. **Reset per call**: The counter is reset at the start of `generate()`,
   ensuring no state leakage between calls.
3. **Performance**: Avoids mutex overhead for a simple counter.

**Pattern**: Interior mutability is appropriate when:
- State is tied to a single execution context
- Concurrent access doesn't occur (verified by architecture)
- Performance is critical (avoids mutex overhead)

**Not Safe If**:
- Generator is Arc-shared across async tasks (would require AtomicUsize)
- Multiple threads call generate() on same instance

### Database Connection Pooling

Connection pooling uses thread-safe structures:
- `Arc<Pool>`: Shared connection pool reference
- `tokio::sync::Mutex`: Async-aware mutual exclusion
- Connection checkout/return: Atomic operations

## Type System Security

### Identifier Validation

GraphQL identifiers are validated against a regex at parse time:

```rust
// Regex ensures identifiers are valid:
// ^[a-zA-Z_][a-zA-Z0-9_]*$

// Invalid identifiers rejected at compile time:
// - Type name with spaces
// - Field names with special characters
// - Enum values with quotes
```

### Type Checking

GraphQL type checking prevents logic errors:
- Field type mismatches caught at compile time
- Null/Non-null violations caught at validation time
- Circular references detected and rejected

## Denial of Service Prevention

### Query Limits (Current/Planned)

Current:
- Query timeout: Configurable timeout per query execution
- Connection pool limits: Max connections to database
- Result set streaming: Avoid loading entire results in memory

Planned:
- Max result size: Configurable byte limit on responses
- Max nesting depth: Limit deeply nested JSONB extraction
- Max query complexity: Cost-based query limits

### Testing

DOS prevention tests:
- Very deep nesting (100+ levels)
- Large result sets (1M+ rows)
- Slow queries (timeout handling)

## Best Practices

### ✅ DO

- [ ] Use parameterized queries for all values
- [ ] Validate identifiers against regex
- [ ] Use Rust's type system (u32 vs String)
- [ ] Escape SQL string literals
- [ ] Use atomic operations for counters
- [ ] Test security scenarios in unit tests

### ❌ DON'T

- [ ] Interpolate user input into SQL strings
- [ ] Skip identifier validation
- [ ] Mix string types with numeric types
- [ ] Share interior mutable state across threads
- [ ] Use Cell when AtomicUsize is needed
- [ ] Trust database drivers to catch all errors

## Code Review Checklist

When reviewing code for security:

### 1. SQL Generation

- [ ] Are all values parameterized?
- [ ] Are identifiers validated?
- [ ] Are string literals escaped?
- [ ] Are limit/offset values typed correctly?

### 2. Thread Safety

- [ ] Is Cell used only for single-threaded context?
- [ ] Are Arc-shared values thread-safe?
- [ ] Are atomic operations used for counters?
- [ ] Are we using the right synchronization primitive?

### 3. Type Safety

- [ ] Are types used correctly (u32 not String)?
- [ ] Are null checks present?
- [ ] Are error types propagated?
- [ ] Is the type system preventing injection?

### 4. Denial of Service

- [ ] Are there recursion depth limits?
- [ ] Are there result size limits?
- [ ] Are there query timeouts?
- [ ] Are connection pool limits enforced?

## Resources

- [Rust Book: Safety](https://doc.rust-lang.org/book/ch19-01-unsafe-rust.html)
- [OWASP Top 10 - SQL Injection](https://owasp.org/www-community/attacks/SQL_Injection)
- [Tokio: Concurrency Patterns](https://tokio.rs/tokio/tutorial)
- [Parameterized Queries](https://cheatsheetseries.owasp.org/cheatsheets/SQL_Injection_Prevention_Cheat_Sheet.html)

## Related Documentation

- `../CLAUDE.md`: Development standards and patterns
- `../../tests/`: Security-focused test examples
- `../src/db/`: Database adapter implementations
