# RESOLVED: GraphQL Field Names Not Using camelCase Convention

**Status**: ✅ Resolved in version 0.1.0a8

## Summary
FraiseQL 0.1.0a7 exposes GraphQL fields using snake_case instead of the standard camelCase convention, making it incompatible with most GraphQL client expectations.

## Current Behavior
When defining a type with snake_case fields in Python:
```python
@fraiseql.type
class Repository:
    default_branch: str
    total_commits: int
```

The GraphQL schema exposes these as:
- `default_branch` (snake_case)
- `total_commits` (snake_case)

## Expected Behavior
GraphQL fields should follow the camelCase convention:
- `defaultBranch` (camelCase)
- `totalCommits` (camelCase)

## Impact
1. **Client Compatibility**: Most GraphQL clients expect camelCase fields
2. **GraphQL Best Practices**: The GraphQL specification recommends camelCase for field names
3. **Developer Experience**: Developers need to remember to use snake_case in queries, which is unusual for GraphQL

## Example Error
```graphql
query {
  repository {
    defaultBranch  # Error: Cannot query field 'defaultBranch'. Did you mean 'default_branch'?
  }
}
```

## Reproduction Steps
1. Install FraiseQL 0.1.0a7
2. Create a type with snake_case fields
3. Query using camelCase conventions
4. Receive field not found error

## Suggested Solutions
1. **Automatic Conversion**: Convert snake_case Python fields to camelCase GraphQL fields by default
2. **Opt-in Setting**: Add a configuration option like `camel_case_fields=True` in `create_fraiseql_app()`
3. **Field Mapping**: Support `graphql_name` parameter in `fraise_field()` to allow custom naming

## Example Implementation
```python
# Option 1: Automatic conversion
@fraiseql.type
class Repository:
    default_branch: str  # Exposed as 'defaultBranch' in GraphQL

# Option 2: Configuration
app = fraiseql.create_fraiseql_app(
    types=[Repository],
    camel_case_fields=True  # Enable camelCase conversion
)

# Option 3: Explicit mapping
@fraiseql.type
class Repository:
    default_branch: str = fraise_field(graphql_name="defaultBranch")
```

## Solution Implemented

FraiseQL now supports automatic camelCase conversion with the following features:

1. **Automatic Conversion (Default)**: Snake_case fields are automatically converted to camelCase
2. **Configuration Option**: `camel_case_fields` parameter in `build_fraiseql_schema()`
3. **Explicit Naming**: `graphql_name` parameter in `fraise_field()`
4. **Full Support**: Works with output types, input types, queries, mutations, and subscriptions

See [CAMELCASE_FIELD_SUPPORT.md](CAMELCASE_FIELD_SUPPORT.md) for detailed documentation.

## Priority
High - This affects API compatibility and developer experience

## Environment
- FraiseQL version: 0.1.0a7
- Python version: 3.11
- Use case: pgGit demo for Hacker News launch

---
*Reported during pgGit demo implementation*
*Date: June 17, 2025*
*Resolved: June 17, 2025*
