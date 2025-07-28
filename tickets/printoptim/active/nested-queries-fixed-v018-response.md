# Response to PrintOptim Team: Nested Query Issues Resolved

## Good News! 🎉

I've fixed both issues you reported and released FraiseQL v0.1.0a18. Your nested queries should now work perfectly.

## What Was Fixed

### v0.1.0a17 - Type Instantiation in Development Mode
- Fixed: Repository wasn't getting the correct mode from config
- Now: Development mode properly instantiates types instead of returning raw dicts

### v0.1.0a18 - Partial Object Instantiation
- Fixed: Nested objects required ALL fields even when only a few were requested
- Now: GraphQL queries can request only the fields they need

## Upgrade Instructions

```bash
pip install fraiseql==0.1.0a18
```

No code changes required on your end!

## Your Queries Will Now Work

This query that was failing:
```graphql
query GetAllocations {
  allocations {
    id
    identifier
    machine {
      id
      identifier  # Only requesting 2 fields - this now works!
    }
  }
}
```

Will now return:
```json
{
  "data": {
    "allocations": [
      {
        "id": "650e8400-e29b-41d4-a716-446655440001",
        "identifier": "ALLOC-001",
        "machine": {
          "id": "1451ff31-5511-0000-0000-000000000001",
          "identifier": "MACHINE-001"
        }
      }
    ]
  }
}
```

## How It Works

1. **Development Mode**: Objects are properly instantiated (not raw dicts)
2. **Partial Fields**: Only requested fields are populated
3. **Missing Fields**: Unrequested required fields are set to `None` internally
4. **Nested Objects**: Works recursively for all levels of nesting

## Important Notes

- This only affects development mode (production returns raw dicts as before)
- You don't need to make fields optional anymore
- Your type definitions can stay as they are with required fields
- The JSONB data structure in your views remains unchanged

## Testing Recommendation

After upgrading, test your queries:

1. Simple queries (should still work)
2. Nested queries with partial fields (now fixed)
3. Complex nested queries with multiple levels (should work)

## Next Steps

1. Run `pip install fraiseql==0.1.0a18`
2. Restart your development server
3. Test your allocation queries with nested machine data
4. Let me know if you encounter any issues

## Technical Details (Optional Reading)

The fix introduces a partial instantiation system that:
- Creates objects with only requested fields in development mode
- Handles dataclasses and regular classes
- Bypasses `__post_init__` validation for missing fields
- Marks instances as partial with `__fraiseql_partial__` attribute

This maintains the GraphQL principle of "ask for what you need, get exactly that" while preserving Python type safety where possible.

## Summary

Your nested queries should now work as expected. The GraphQL principle of requesting only needed fields is restored. No more "missing required argument" errors for unrequested fields!

Please upgrade to v0.1.0a18 and confirm the fix works for your use cases. If you encounter any issues, please report them and I'll address them immediately.
