# GraphQL Fragments Support

FraiseQL provides comprehensive support for GraphQL fragments, including nested fragment spreads and automatic cycle detection for security.

## Features

### ✅ Nested Fragment Spreads

Fragments can now be used in nested selections, not just at the root level:

```graphql
fragment UserFields on User {
  id
  name
}

query {
  posts {
    id
    title
    author { ...UserFields }  # ✅ Works in v1.8.6+
  }
}
```

### ✅ Fragment Cycle Detection

Automatic protection against circular fragment references prevents DoS attacks:

```graphql
# ❌ This will be rejected at query execution time
fragment A on User { id ...B }
fragment B on User { name ...A }

query { users { ...A } }
# Error: Circular fragment reference: A
```

## Security

Fragment cycle detection prevents infinite recursion and DoS attacks by:

- Tracking visited fragments during query execution
- Rejecting queries with circular dependencies
- Providing clear error messages for debugging
- Maintaining O(n) time complexity for cycle detection

## Performance

- Minimal overhead: < 1μs per fragment
- Efficient cycle detection using set-based tracking
- No impact on queries without fragments
- Backward compatible with existing fragment usage

## Examples

See the [nested fragments examples](../examples/nested-fragments.md) and [fragment cycle handling](../examples/fragment-cycles.md) guides for detailed usage patterns.
