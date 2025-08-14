# Mutation Result Pattern

> **In this section:** Implement the standardized mutation result pattern for enterprise-grade applications
> **Prerequisites:** Understanding of PostgreSQL types, CQRS principles, GraphQL mutations
> **Time to complete:** 45 minutes

Complete guide to implementing FraiseQL's standardized mutation result pattern, based on enterprise-proven patterns from production systems. This pattern provides consistent mutation responses, comprehensive metadata, audit trails, and structured NOOP handling.

## Overview

The Mutation Result Pattern establishes a standardized structure for all mutation responses in FraiseQL applications. Unlike ad-hoc JSON returns, this pattern provides:

- **Consistent Response Structure** - All mutations return the same `app.mutation_result` type
- **Rich Metadata** - Complete audit trails and debugging information
- **Field-Level Change Tracking** - Know exactly which fields were modified
- **Structured NOOP Handling** - Graceful handling of edge cases and validation failures
- **Enterprise Audit Support** - Complete change history for compliance requirements

## Type Definition

### The app.mutation_result Type

The `app.mutation_result` type is the foundation of FraiseQL's mutation response system. Every mutation function returns this structured type, ensuring consistency across all database operations:

```sql
CREATE TYPE app.mutation_result AS (
    id UUID,                    -- Entity primary key (pk_[entity])
    updated_fields TEXT[],      -- Array of field names that were changed
    status TEXT,                -- Status code indicating operation outcome
    message TEXT,               -- Human-readable message for debugging/UI
    object_data JSONB,          -- Complete entity data from view (v_* or tv_*)
    extra_metadata JSONB        -- Debug context, validation info, audit data
);
```

**Field Descriptions:**

- **`id`** - The UUID primary key of the affected entity. For create operations, this is the newly generated ID. For updates/deletes, it's the existing entity ID.
- **`updated_fields`** - String array listing exactly which fields changed during the operation. Empty array for create operations, populated array for updates.
- **`status`** - Standardized status code indicating the operation outcome (see Status Code Semantics below).
- **`message`** - Human-readable description of what happened. Used for debugging and can be displayed to end users.
- **`object_data`** - Complete entity data as returned by the corresponding view (`v_[entity]` or `tv_[entity]`). Contains the final state after the operation.
- **`extra_metadata`** - JSON object containing additional context like validation details, debug information, and audit metadata.

### Status Code Semantics

Status codes follow a structured pattern that enables consistent handling across all mutations:

#### Success Status Codes

- **`new`** - Entity was successfully created
- **`updated`** - Entity was successfully modified (one or more fields changed)
- **`deleted`** - Entity was successfully removed or soft-deleted

#### NOOP Status Codes (No Operation Performed)

NOOP statuses use the prefix `noop:` followed by a specific reason:

**Creation NOOPs:**
- **`noop:already_exists`** - Attempted to create entity with duplicate unique constraint
- **`noop:invalid_parent`** - Referenced parent entity doesn't exist or isn't accessible

**Update NOOPs:**
- **`noop:not_found`** - Entity with specified ID doesn't exist
- **`noop:no_changes`** - Update attempted but all provided values match current values
- **`noop:invalid_status`** - Status transition not allowed by business rules
- **`noop:invalid_[field]`** - Specific field validation failed (e.g., `noop:invalid_email`)

**Delete NOOPs:**
- **`noop:not_found`** - Entity doesn't exist or already deleted
- **`noop:cannot_delete_has_children`** - Entity has dependent child records
- **`noop:cannot_delete_referenced`** - Entity is referenced by other entities
- **`noop:cannot_delete_protected`** - Entity is marked as protected/system entity

**Authorization NOOPs:**
- **`noop:insufficient_permissions`** - User lacks required permissions for this operation
- **`noop:tenant_mismatch`** - Entity belongs to different tenant context

#### Error Handling Philosophy

The NOOP pattern eliminates the need for exceptions in most business logic scenarios. Instead of throwing errors, mutations return structured information about why an operation couldn't be completed. This approach:

- Maintains transaction consistency (no rollbacks needed)
- Provides clear feedback for API consumers
- Enables graceful degradation in batch operations
- Simplifies error handling in GraphQL resolvers

## Logging Function

### Core Logging Mechanism

The `core.log_and_return_mutation` function is the central mechanism for creating standardized mutation results. This function handles three critical responsibilities:

1. **Audit Logging** - Records the complete mutation in the audit log
2. **Change Tracking** - Calculates which fields were modified
3. **Result Construction** - Builds the standardized `app.mutation_result` response

All mutation functions should use this helper to ensure consistent logging and response structure.

### Function Signature

```sql
CREATE OR REPLACE FUNCTION core.log_and_return_mutation(
    input_pk_organization UUID,        -- Tenant context
    input_actor UUID,                  -- User performing the action
    input_entity_type TEXT,            -- Entity type (e.g., 'user', 'contract')
    input_entity_id UUID,              -- Entity primary key
    input_modification_type TEXT,       -- Operation type: INSERT, UPDATE, DELETE, NOOP
    input_change_status TEXT,          -- Status code: new, updated, deleted, noop:*
    input_fields TEXT[],               -- Array of changed field names
    input_message TEXT,                -- Human-readable message
    input_payload_before JSONB,        -- Entity state before change (NULL for creates)
    input_payload_after JSONB,         -- Entity state after change (NULL for deletes)
    input_extra_metadata JSONB DEFAULT '{}'::JSONB  -- Additional debug/audit metadata
) RETURNS app.mutation_result AS $$
DECLARE
    v_result app.mutation_result;
    v_audit_id UUID;
BEGIN
    -- Log the mutation for audit purposes
    INSERT INTO audit.tb_mutation_log (
        pk_organization,
        actor_id,
        entity_type,
        entity_id,
        modification_type,
        change_status,
        changed_fields,
        payload_before,
        payload_after,
        extra_metadata,
        created_at
    ) VALUES (
        input_pk_organization,
        input_actor,
        input_entity_type,
        input_entity_id,
        input_modification_type,
        input_change_status,
        input_fields,
        input_payload_before,
        input_payload_after,
        input_extra_metadata,
        NOW()
    ) RETURNING pk_mutation_log INTO v_audit_id;

    -- Construct the mutation result
    v_result.id := input_entity_id;
    v_result.updated_fields := input_fields;
    v_result.status := input_change_status;
    v_result.message := input_message;
    v_result.object_data := input_payload_after;
    v_result.extra_metadata := input_extra_metadata ||
                              jsonb_build_object('audit_id', v_audit_id);

    RETURN v_result;
END;
$$ LANGUAGE plpgsql;
```

### Parameter Details

**Context Parameters:**
- **`input_pk_organization`** - UUID of the tenant/organization for multi-tenant isolation
- **`input_actor`** - UUID of the user performing the mutation (for audit trails)

**Entity Parameters:**
- **`input_entity_type`** - String identifying the entity type (matches table name without prefix)
- **`input_entity_id`** - UUID primary key of the affected entity

**Operation Parameters:**
- **`input_modification_type`** - Database operation type:
  - `INSERT` - New record created
  - `UPDATE` - Existing record modified
  - `DELETE` - Record deleted/soft-deleted
  - `NOOP` - No database changes made
- **`input_change_status`** - Semantic status code for the operation outcome

**Change Tracking:**
- **`input_fields`** - Array of field names that were modified (empty for creates, populated for updates)
- **`input_payload_before`** - Complete entity state before modification (NULL for creates)
- **`input_payload_after`** - Complete entity state after modification (NULL for deletes)

**Metadata:**
- **`input_message`** - Human-readable description of what happened
- **`input_extra_metadata`** - Additional context like validation details, debug info, or business metadata

### Usage Pattern in Mutations

Every mutation function should end by calling this logging function:

```sql
CREATE OR REPLACE FUNCTION app.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_new_user_id UUID;
    v_user_data JSONB;
BEGIN
    -- Validation and business logic here...

    -- Create the user
    INSERT INTO tenant.tb_user (pk_user, pk_organization, data, created_by)
    VALUES (gen_random_uuid(), input_pk_organization, input_payload, input_created_by)
    RETURNING pk_user INTO v_new_user_id;

    -- Get the complete user data from view
    SELECT to_jsonb(u.*) INTO v_user_data
    FROM public.v_user u
    WHERE u.id = v_new_user_id;

    -- Log and return standardized result
    RETURN core.log_and_return_mutation(
        input_pk_organization := input_pk_organization,
        input_actor := input_created_by,
        input_entity_type := 'user',
        input_entity_id := v_new_user_id,
        input_modification_type := 'INSERT',
        input_change_status := 'new',
        input_fields := ARRAY[]::TEXT[],  -- Empty for creates
        input_message := 'User created successfully',
        input_payload_before := NULL,     -- No previous state
        input_payload_after := v_user_data,
        input_extra_metadata := jsonb_build_object(
            'input_validation', 'passed',
            'created_via', 'api'
        )
    );
END;
$$ LANGUAGE plpgsql;
```

## GraphQL Integration

### Python Resolver Patterns

FraiseQL's Python resolvers automatically handle `app.mutation_result` responses and convert them into GraphQL union types. The resolver pattern follows a consistent structure that parses the mutation result and maps it to appropriate success or error types.

```python
import fraiseql
from fraiseql import mutation, input, success, failure
from uuid import UUID
from graphql import GraphQLResolveInfo

# Define input type
@input
class CreateUserInput:
    email: str
    name: str
    roles: list[str] | None = None

# Define success response
@success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

# Define error response
@failure
class CreateUserError:
    message: str
    error_code: str
    validation_errors: dict[str, str] | None = None

# Mutation resolver
@mutation
async def create_user(
    info: GraphQLResolveInfo,
    input: CreateUserInput
) -> CreateUserSuccess | CreateUserError:
    """Create a new user account."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]
    user_id = info.context.get("user_id")

    # Call PostgreSQL function that returns app.mutation_result
    result = await db.call_function(
        "app.create_user",
        input_pk_organization=tenant_id,
        input_created_by=user_id,
        input_payload=input.to_dict()
    )

    # Parse mutation result
    return _parse_user_mutation_result(result, CreateUserSuccess, CreateUserError)

def _parse_user_mutation_result(
    result: dict,
    success_type,
    error_type
):
    """Helper to parse mutation result into GraphQL types."""
    status = result["status"]

    # Handle success cases
    if status in ["new", "updated"]:
        user_data = result["object_data"]
        return success_type(
            user=User.from_dict(user_data),
            message=result["message"]
        )

    # Handle NOOP cases as errors
    elif status.startswith("noop:"):
        error_code = status.replace("noop:", "").upper()
        return error_type(
            message=result["message"],
            error_code=error_code,
            validation_errors=_extract_validation_errors(result)
        )

    # Handle unexpected status
    else:
        return error_type(
            message=f"Unexpected mutation status: {status}",
            error_code="INTERNAL_ERROR"
        )

def _extract_validation_errors(result: dict) -> dict[str, str] | None:
    """Extract validation errors from mutation result metadata."""
    extra_metadata = result.get("extra_metadata", {})
    return extra_metadata.get("validation_errors")
```

### Success/Error Type Mapping

The mutation result status codes map directly to GraphQL response types:

| Status Pattern | GraphQL Type | Description |
|----------------|--------------|-------------|
| `new` | Success | Entity created successfully |
| `updated` | Success | Entity modified successfully |
| `deleted` | Success | Entity removed successfully |
| `noop:*` | Error | Operation failed, return reason |

### Advanced Resolver Pattern

For more complex scenarios, you can access all mutation result fields:

```python
@mutation
async def update_user_profile(
    info: GraphQLResolveInfo,
    id: UUID,
    input: UpdateUserInput
) -> UpdateUserSuccess | UpdateUserError:
    """Update user profile with detailed change tracking."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    result = await db.call_function(
        "app.update_user",
        input_pk_organization=tenant_id,
        input_pk_user=id,  # Note: Use pk_ for function parameters
        input_updated_by=info.context["user_id"],
        input_payload=input.to_dict()
    )

    status = result["status"]

    if status == "updated":
        return UpdateUserSuccess(
            user=User.from_dict(result["object_data"]),
            message=result["message"],
            changed_fields=result["updated_fields"],  # Field-level tracking
            audit_id=result["extra_metadata"]["audit_id"]
        )
    elif status == "noop:no_changes":
        return UpdateUserSuccess(
            user=User.from_dict(result["object_data"]),
            message="No changes were needed",
            changed_fields=[]  # Empty array for no changes
        )
    elif status.startswith("noop:"):
        error_code = status.replace("noop:", "").upper()
        return UpdateUserError(
            message=result["message"],
            error_code=error_code,
            entity_id=result["id"]
        )
    else:
        return UpdateUserError(
            message=f"Unexpected status: {status}",
            error_code="INTERNAL_ERROR"
        )
```

### Batch Mutation Handling

For batch operations, parse each mutation result individually:

```python
@mutation
async def batch_update_users(
    info: GraphQLResolveInfo,
    updates: list[BatchUserUpdate]
) -> BatchUpdateUsersResult:
    """Update multiple users in a batch operation."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    # Call batch function that returns array of mutation_result
    results = await db.call_function(
        "app.batch_update_users",
        input_pk_organization=tenant_id,
        input_updates=[update.to_dict() for update in updates]
    )

    successful_updates = []
    failed_updates = []

    for result in results:
        if result["status"] in ["updated", "noop:no_changes"]:
            successful_updates.append(
                UserUpdateResult(
                    user=User.from_dict(result["object_data"]),
                    changed_fields=result["updated_fields"],
                    status=result["status"]
                )
            )
        else:
            failed_updates.append(
                UserUpdateError(
                    entity_id=result["id"],
                    error_code=result["status"].replace("noop:", "").upper(),
                    message=result["message"]
                )
            )

    return BatchUpdateUsersResult(
        successful=successful_updates,
        failed=failed_updates,
        total_processed=len(results)
    )
```

### Context Integration

Mutation resolvers automatically receive context from FraiseQL:

```python
@mutation
async def delete_user(
    info: GraphQLResolveInfo,
    id: UUID
) -> DeleteUserSuccess | DeleteUserError:
    """Delete a user with proper authorization."""

    # Context automatically provided by FraiseQL
    db = info.context["db"]              # Database connection
    tenant_id = info.context["tenant_id"] # Multi-tenant context
    current_user = info.context.get("user_id")  # Authenticated user
    permissions = info.context.get("permissions", [])

    # Authorization check
    if "delete_user" not in permissions:
        return DeleteUserError(
            message="Insufficient permissions to delete users",
            error_code="INSUFFICIENT_PERMISSIONS"
        )

    result = await db.call_function(
        "app.delete_user",
        input_pk_organization=tenant_id,
        input_pk_user=id,
        input_deleted_by=current_user
    )

    if result["status"] == "deleted":
        return DeleteUserSuccess(
            deleted_id=result["id"],
            message=result["message"]
        )
    else:
        error_code = result["status"].replace("noop:", "").upper()
        return DeleteUserError(
            message=result["message"],
            error_code=error_code
        )
```

## Metadata Patterns

### Extra Metadata Structure

[Content placeholder - What goes in extra_metadata field]

### Debugging Information

[Content placeholder - Debug context patterns]

## Change Tracking

### Updated Fields Array

[Content placeholder - How updated_fields array works]

### Field-Level Auditing

[Content placeholder - Tracking specific field changes]

## Examples

### Simple Create Mutation

[Content placeholder - Complete create example]

### Update with Change Tracking

[Content placeholder - Update example with field tracking]

### NOOP Handling Scenario

[Content placeholder - NOOP example with proper status codes]

### Complex Business Logic

[Content placeholder - Advanced mutation example]

## Error Handling Patterns

### Validation Failures

[Content placeholder - Handling validation errors]

### Business Rule Violations

[Content placeholder - Business logic error patterns]

## Migration Guide

### Converting Existing Mutations

[Content placeholder - How to migrate from ad-hoc returns]

### Backward Compatibility

[Content placeholder - Maintaining compatibility during migration]

## Best Practices

### Do's and Don'ts

[Content placeholder - Best practices for using mutation result pattern]

### Performance Considerations

[Content placeholder - Performance implications and optimizations]

## Troubleshooting

### Common Issues

[Content placeholder - Common problems and solutions]

### Debugging Techniques

[Content placeholder - How to debug mutation result issues]

## Integration Points

### Authentication and Authorization

[Content placeholder - How mutation results work with auth]

### Multi-Tenant Patterns

[Content placeholder - Tenant context in mutation results]

### Cache Invalidation

[Content placeholder - Cache invalidation with mutation results]

## See Also

- [PostgreSQL Function-Based Mutations](./postgresql-function-based.md) - Core mutation patterns
- [Migration Guide](./migration-guide.md) - Converting existing mutations
- [Multi-Tenancy](../advanced/multi-tenancy.md) - Tenant context patterns
- [CQRS](../advanced/cqrs.md) - Command-query separation principles
