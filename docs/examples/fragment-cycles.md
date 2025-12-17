# Fragment Cycle Detection and Error Handling

FraiseQL automatically detects and prevents circular fragment references to protect against DoS attacks and infinite recursion.

## What Are Fragment Cycles?

A fragment cycle occurs when fragments reference each other in a circular pattern:

```graphql
# ❌ Direct cycle: A → B → A
fragment A on User {
  id
  ...B
}

fragment B on User {
  name
  ...A
}

# ❌ Self-reference: A → A
fragment A on User {
  id
  ...A
}

# ❌ Complex cycle: A → B → C → A
fragment A on User { ...B }
fragment B on User { ...C }
fragment C on User { ...A }
```

## Error Detection

FraiseQL detects cycles at query execution time and returns a clear error:

```graphql
query {
  users {
    ...A
  }
}
```

**Error Response:**
```json
{
  "errors": [
    {
      "message": "Circular fragment reference: A",
      "path": ["users"],
      "locations": [{"line": 3, "column": 5}]
    }
  ]
}
```

## How Cycle Detection Works

1. **Tracking**: Each fragment spread is tracked in a `visited_fragments` set
2. **Detection**: Before expanding a fragment, check if it's already being processed
3. **Prevention**: Reject queries with circular references before execution
4. **Performance**: O(n) complexity where n is the number of fragments

## Common Patterns to Avoid

### Pattern 1: Mutual References

```graphql
# ❌ DON'T DO THIS
fragment UserBasic on User {
  id
  ...UserExtended
}

fragment UserExtended on User {
  name
  email
  ...UserBasic  # Creates cycle
}
```

**Fix:**
```graphql
# ✅ DO THIS INSTEAD
fragment UserBasic on User {
  id
  name
}

fragment UserExtended on User {
  ...UserBasic
  email
  phone
}
```

### Pattern 2: Self-Referencing Fragments

```graphql
# ❌ DON'T DO THIS
fragment RecursiveUser on User {
  id
  friends {
    ...RecursiveUser  # Infinite recursion
  }
}
```

**Fix:**
```graphql
# ✅ DO THIS INSTEAD
fragment UserSummary on User {
  id
  name
}

fragment UserWithFriends on User {
  ...UserSummary
  friends {
    ...UserSummary  # Limited depth
  }
}
```

### Pattern 3: Deep Chain Cycles

```graphql
# ❌ DON'T DO THIS
fragment A on Type { ...B }
fragment B on Type { ...C }
fragment C on Type { ...D }
fragment D on Type { ...A }  # Cycle closes here
```

## Error Messages

FraiseQL provides specific error messages for different cycle scenarios:

- `"Circular fragment reference: FragmentName"` - Basic cycle detection
- Includes location information when available
- Path shows where the cycle was detected in the query

## Testing Cycle Detection

```python
# Test cycle detection in your application
query = """
    fragment A on User { id ...B }
    fragment B on User { name ...A }
    query { users { ...A } }
"""

# This will raise ValueError: Circular fragment reference: A
result = await execute_query(query)
```

## Security Benefits

- **DoS Protection**: Prevents infinite recursion attacks
- **Performance**: Fast O(n) cycle detection
- **Reliability**: Predictable query execution time
- **User Experience**: Clear error messages for debugging

## Best Practices

1. **Design Fragments Hierarchically**: Use composition instead of mutual references
2. **Test Queries**: Validate fragment combinations in development
3. **Limit Depth**: Avoid deeply nested fragment chains
4. **Document Dependencies**: Clearly show fragment relationships in comments

## Troubleshooting

### Error: "Circular fragment reference"

**Cause**: Fragments reference each other in a cycle
**Solution**: Redesign fragment structure to avoid circular dependencies
**Prevention**: Test fragment combinations during development

### Fragments Not Expanding

**Cause**: Usually not a cycle issue, but missing fragment definitions
**Check**: Ensure all referenced fragments are defined in the query
**Note**: Cycle detection only triggers for actual circular references

## Performance Impact

- **Overhead**: < 1μs per fragment for cycle detection
- **Memory**: O(n) space for visited fragment tracking
- **Queries without cycles**: Zero performance impact
- **Cycle rejection**: Fast failure before expensive processing
