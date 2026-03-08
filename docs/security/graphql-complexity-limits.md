# GraphQL Complexity Limits

Audit of all GraphQL query complexity and abuse protections in FraiseQL.

## Implemented Protections

| Limit | Default | Configurable | Location |
|-------|---------|-------------|----------|
| **Alias amplification** | 30 aliases | No (hardcoded) | `crates/fraiseql-server/src/validation.rs:459` |
| **Query depth** | 10 levels | Yes (`max_query_depth` in `fraiseql.toml`) | `crates/fraiseql-server/src/validation.rs:457` |
| **Complexity error rate** | 30 errors/60s per key | Yes (`complexity_errors_max_requests`) | `crates/fraiseql-core/src/validation/rate_limiting.rs:67` |
| **Federation batch** | 1000 representations | No (hardcoded) | `crates/fraiseql-server/src/federation/` |

### Alias Amplification (Hardcoded at 30)

```
Location: crates/fraiseql-server/src/validation.rs
  Line 459: max_aliases_per_query: 30 (default in RequestValidator)
  Line 179: if alias_count > self.max_aliases_per_query → 429 TooManyRequests
```

A query with 31+ aliases on the same field is rejected. This prevents a client from
forcing the server to resolve the same field 1000+ times via aliasing.

### Query Depth (Default 10, Configurable)

```
Location: crates/fraiseql-server/src/validation.rs
  Line 457: max_depth: 10 (default)
  Line 86:  pub const fn with_max_depth(mut self, max_depth: usize) -> Self

fraiseql.toml override:
  [fraiseql.security]
  max_query_depth = 15
```

A query exceeding the depth limit is rejected with a `QueryTooDeep` error before reaching
the database.

### Complexity Error Rate Limiting

When queries fail complexity validation (depth/alias), the error itself is rate-limited
to prevent probing attacks:

```
Location: crates/fraiseql-core/src/validation/rate_limiting.rs
  Line 67: complexity_errors_max_requests: 30 (per 60-second window)
```

---

## Not Implemented

| Attack | Status | Notes |
|--------|--------|-------|
| **Cost/complexity budget** | ❌ Not implemented | Planned for v2.2.0. Static complexity scoring per field not yet present. |
| **Fragment cycle detection** | ❓ Unverified | `graphql-parser` crate handles AST parsing; cycle detection depends on the library. |
| **Introspection disable** | ❓ Unverified | No `disable_introspection` flag found in `validation.rs`. Check `routes/graphql/handler.rs`. |
| **Batch query amplification** | ❓ Unverified | HTTP batching (array of operations) not confirmed present or absent. |
| **Field count explosion** | ❌ Not implemented | No `max_fields_per_query` limit. |

---

## Configuration

```toml
# fraiseql.toml
[fraiseql.security]
# Max query nesting depth (default: 10 in RequestValidator)
max_query_depth = 10

# Max query complexity score — NOT YET IMPLEMENTED
# max_query_complexity = 1000

[fraiseql.security.rate_limiting]
# Rate limit for complexity error responses (default: 30 per 60s)
complexity_errors_max_requests = 30
complexity_errors_window_secs  = 60
```

---

## Recommended Gaps to Address

1. **Fragment cycle detection** — confirm `graphql-parser` handles this or add explicit check
2. **Introspection control** — add `allow_introspection: bool` flag to `RequestValidator`
3. **Cost budget** — implement field-level cost scoring for v2.2.0
4. **Field count limit** — add `max_fields_per_selection_set: usize` to `RequestValidator`
