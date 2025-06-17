# FraiseQL 0.1.0a5 Bug Report

## Issue: Mutation Decorator Breaking Change

### Summary
FraiseQL 0.1.0a5 introduced a breaking change in the `@fraiseql.mutation` decorator that is not documented and breaks the examples provided in the FEEDBACK_RESPONSE.md.

### Error Details
```python
TypeError: Mutation create_branch must define 'success' type
```

### Problem
The mutation decorator now requires mutations to define 'success' and 'error' types, following a CQRS pattern. However:

1. The examples in FEEDBACK_RESPONSE.md don't follow this pattern
2. The quickstart examples don't show this requirement
3. The error message doesn't explain how to fix it

### Code That Fails (from your examples)
```python
@fraiseql.mutation
async def create_branch(info, input: CreateBranchInput) -> Branch:
    """Create a new branch"""
    # ... implementation
    return new_branch
```

### Expected Behavior
Either:
1. Support simple mutations that return a type directly (backward compatibility)
2. Update all examples to show the new success/error pattern
3. Provide clear migration guide for the breaking change

### Current Workaround
Had to remove all mutations from the pgGit demo to get it working with 0.1.0a5.

### Additional Note: Snake Case Fields
I noticed GraphQL field names use snake_case (e.g., `default_branch`, `commits_count`). While this follows Python conventions, it differs from typical GraphQL camelCase. This is fine if intentional, but should be documented.

### Impact
- Cannot implement mutations as shown in documentation
- Demo had to be simplified, removing key functionality

### Suggestions
1. Fix the examples to match the new API
2. Document the breaking changes clearly
3. Consider supporting both patterns for mutations
4. Add migration guide from 0.1.0a4 to 0.1.0a5

---
*Reported during pgGit demo implementation*
*FraiseQL version: 0.1.0a5*
*Date: June 17, 2025*