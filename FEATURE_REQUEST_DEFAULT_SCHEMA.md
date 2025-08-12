# Feature Request: Default Schema Configuration for Mutations and Queries

## Summary
Add support for configuring a default database schema globally in FraiseQL to eliminate repetitive `schema="app"` parameters in mutation and query decorators.

## Problem Statement
Currently, developers must specify the schema parameter in every mutation and query decorator, leading to significant boilerplate when most functions use the same schema:

```python
@fraiseql.mutation(
    function="delete_contract_item",
    schema="app",  # ← Repeated in every mutation
    context_params={"tenant_id": "input_pk_organization", "user": "input_deleted_by"},
)
class DeleteContractItem:
    input: DeletionInput
    success: DeleteContractItemSuccess
    failure: DeleteContractItemError
```

For projects with 50+ mutations where 99% use the same schema, this creates maintenance overhead and violates DRY principles.

## Proposed Solution

### 1. Add Default Schema to FraiseQLConfig

```python
# In fraiseql/fastapi/config.py
@dataclass
class FraiseQLConfig:
    # ... existing fields
    default_mutation_schema: str = "public"
    default_query_schema: str = "public"
```

### 2. App-Level Configuration

```python
# In user's app.py
fraiseql_config = FraiseQLConfig(
    mutation_error_config=STRICT_STATUS_CONFIG,
    default_mutation_schema="app",  # Set once globally
    default_query_schema="app",     # Set once globally
)

fraiseql_app = create_fraiseql_app(config=fraiseql_config)
```

### 3. Simplified Decorators

```python
# Mutation without explicit schema (uses default)
@fraiseql.mutation(
    function="delete_contract_item",
    # schema not needed - uses default_mutation_schema
    context_params={"tenant_id": "input_pk_organization", "user": "input_deleted_by"},
)
class DeleteContractItem:
    input: DeletionInput
    success: DeleteContractItemSuccess
    failure: DeleteContractItemError

# Override when needed
@fraiseql.mutation(
    function="special_function",
    schema="core",  # Explicit override of default
    context_params={},
)
class SpecialMutation:
    # ...
```

### 4. Query Support

```python
# Query without explicit schema
@fraiseql.query(
    function="get_contracts",
    # schema not needed - uses default_query_schema
)
async def get_contracts(info, where: ContractWhereInput = None) -> list[Contract]:
    # ...

# Override when needed
@fraiseql.query(
    function="system_info",
    schema="information_schema",  # Explicit override
)
async def system_info(info) -> SystemInfo:
    # ...
```

## Implementation Details

### Decorator Changes
- Modify `@fraiseql.mutation` and `@fraiseql.query` decorators to check for default schemas when `schema` parameter is not provided
- Access default schema from the app's FraiseQLConfig
- Maintain backward compatibility - explicit schema parameters always take precedence

### Configuration Flow
1. App startup: FraiseQLConfig sets default schemas
2. Decorator registration: Check if explicit schema provided
3. If not provided: Use appropriate default from config
4. If provided: Use explicit schema (override behavior)

### Backward Compatibility
- All existing code continues to work unchanged
- Default values maintain current behavior (`"public"`)
- Explicit schema parameters always override defaults

## Benefits

1. **Reduced Boilerplate**: Eliminate 90% of repetitive `schema="app"` declarations
2. **DRY Principle**: Configure schema once, use everywhere
3. **Maintainability**: Single place to change schema naming
4. **Developer Experience**: Less cognitive load, cleaner code
5. **Flexibility**: Can still override for special cases
6. **Zero Breaking Changes**: Fully backward compatible

## Use Cases

### Primary Use Case
- Multi-tenant SaaS applications with consistent schema usage
- Domain-driven design with all business logic in `app` schema
- Microservices with schema-per-service patterns

### Override Scenarios
- System/admin functions in `public` or `information_schema`
- Cross-schema operations
- Migration or utility functions
- Third-party integrations

## Alternative Approaches Considered

### 1. Helper Decorators (Current Workaround)
```python
def app_mutation(function: str, context_params: dict = None):
    return fraiseql.mutation(function=function, schema="app", context_params=context_params or {})
```
**Issues**: Custom solution, not part of FraiseQL core, requires project-specific setup

### 2. Environment Variables
```bash
FRAISEQL_DEFAULT_SCHEMA=app
```
**Issues**: Environment-specific, not ideal for configuration management

### 3. Decorator Factories
```python
app_mutation = fraiseql.mutation_factory(schema="app")
app_query = fraiseql.query_factory(schema="app")
```
**Issues**: More complex API, separate decorators to maintain

## Implementation Priority

**High Priority** - This addresses a common pain point for production applications and significantly improves developer experience without breaking existing functionality.

## Related Issues

- Reduces boilerplate in mutation-heavy applications
- Aligns with configuration-over-convention principles
- Supports clean architecture patterns

## Testing Requirements

1. **Backward Compatibility Tests**: Ensure all existing decorators work unchanged
2. **Default Schema Tests**: Verify defaults are applied when schema not specified
3. **Override Tests**: Confirm explicit schema parameters take precedence
4. **Configuration Tests**: Validate config loading and application
5. **Integration Tests**: Test with real PostgreSQL schemas

## Documentation Updates

1. Update FraiseQLConfig documentation
2. Add examples to mutation/query documentation
3. Update quickstart guides to show both patterns
4. Migration guide for existing projects

---

**Requested by**: PrintOptim Backend Team
**FraiseQL Version**: v0.1.3
**Priority**: High
**Breaking Changes**: None
**Estimated Impact**: 50+ files, ~200 decorators could be simplified
