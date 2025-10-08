# Mutation Result Pattern

> **In this section:** Implement the standardized mutation result pattern for enterprise-grade applications
> **Prerequisites:** Understanding of PostgreSQL types, CQRS principles, GraphQL mutations
> **Time to complete:** 45 minutes

Complete guide to implementing FraiseQL's standardized mutation result pattern, based on enterprise-proven patterns from production systems. This pattern provides consistent mutation responses, comprehensive metadata, audit trails, and structured NOOP handling.

## Overview

The Mutation Result Pattern establishes a standardized structure for all mutation responses in FraiseQL applications. Unlike ad-hoc JSON returns, this pattern provides:

- **Consistent Response Structure** - All mutations return the same `app.mutation_result` type
- **Rich Object Returns** - Complete entity data eliminates additional API calls, dramatically reducing network latency and improving frontend performance
- **Rich Metadata** - Complete audit trails and debugging information
- **Field-Level Change Tracking** - Know exactly which fields were modified
- **Structured NOOP Handling** - Graceful handling of edge cases and validation failures
- **Enterprise Audit Support** - Complete change history for compliance requirements
- **CDC/Debezium Compatibility** - Structure aligns with Change Data Capture patterns for event streaming

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

The `extra_metadata` field provides a structured way to include additional context with mutation results. This JSONB field supports debugging, validation details, and business-specific metadata:

```json
{
  "audit_id": "550e8400-e29b-41d4-a716-446655440001",
  "validation_errors": {
    "email": "Invalid email format",
    "age": "Must be between 18 and 120"
  },
  "debug_context": {
    "function_duration_ms": 45,
    "rows_scanned": 1,
    "constraints_checked": ["users_email_unique", "users_age_check"]
  },
  "business_metadata": {
    "workflow_stage": "approval_pending",
    "notification_sent": true,
    "integration_sync_queued": false
  },
  "request_context": {
    "ip_address": "192.168.1.100",
    "user_agent": "FraiseQL-Client/1.0",
    "api_version": "v1"
  }
}
```

### Debugging Information

Include debugging context to help with troubleshooting:

```sql
-- In your mutation function
DECLARE
    v_start_time TIMESTAMP := clock_timestamp();
    v_debug_metadata JSONB := '{}'::JSONB;
BEGIN
    -- Your business logic here...

    -- Add debug metadata
    v_debug_metadata := jsonb_build_object(
        'function_duration_ms', EXTRACT(MILLISECONDS FROM clock_timestamp() - v_start_time),
        'query_plan_cost', 1.23,
        'cache_hit', false,
        'validation_steps', ARRAY['email_format', 'uniqueness', 'business_rules']
    );

    RETURN core.log_and_return_mutation(
        -- ... other parameters
        input_extra_metadata := v_debug_metadata
    );
END;
```

## Change Tracking

### Updated Fields Array

The `updated_fields` array provides precise field-level change tracking:

```sql
-- Function to calculate changed fields
CREATE OR REPLACE FUNCTION core.calculate_changed_fields(
    before_data JSONB,
    after_data JSONB
) RETURNS TEXT[] AS $$
DECLARE
    changed_fields TEXT[] := ARRAY[]::TEXT[];
    field_name TEXT;
    before_value JSONB;
    after_value JSONB;
BEGIN
    -- Compare each field in the after_data
    FOR field_name IN SELECT jsonb_object_keys(after_data) LOOP
        before_value := before_data -> field_name;
        after_value := after_data -> field_name;

        -- Check if field changed (handles NULL comparisons)
        IF (before_value IS NULL AND after_value IS NOT NULL) OR
           (before_value IS NOT NULL AND after_value IS NULL) OR
           (before_value != after_value) THEN
            changed_fields := array_append(changed_fields, field_name);
        END IF;
    END LOOP;

    RETURN changed_fields;
END;
$$ LANGUAGE plpgsql;
```

Usage in update mutations:

```sql
CREATE OR REPLACE FUNCTION app.update_user(
    input_pk_organization UUID,
    input_pk_user UUID,
    input_updated_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_before_data JSONB;
    v_after_data JSONB;
    v_changed_fields TEXT[];
BEGIN
    -- Get current state
    SELECT to_jsonb(u.*) INTO v_before_data
    FROM public.v_user u
    WHERE u.id = input_pk_user;

    IF v_before_data IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_updated_by,
            input_entity_type := 'user',
            input_entity_id := input_pk_user,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:not_found',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'User not found',
            input_payload_before := NULL,
            input_payload_after := NULL
        );
    END IF;

    -- Update the record
    UPDATE tenant.tb_user
    SET
        data = data || input_payload,
        updated_at = NOW(),
        updated_by = input_updated_by
    WHERE pk_user = input_pk_user
      AND pk_organization = input_pk_organization;

    -- Get updated state
    SELECT to_jsonb(u.*) INTO v_after_data
    FROM public.v_user u
    WHERE u.id = input_pk_user;

    -- Calculate changed fields
    v_changed_fields := core.calculate_changed_fields(v_before_data, v_after_data);

    -- Determine status
    IF array_length(v_changed_fields, 1) = 0 THEN
        -- No changes made
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_updated_by,
            input_entity_type := 'user',
            input_entity_id := input_pk_user,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:no_changes',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'No changes were made',
            input_payload_before := v_before_data,
            input_payload_after := v_after_data
        );
    ELSE
        -- Changes made
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_updated_by,
            input_entity_type := 'user',
            input_entity_id := input_pk_user,
            input_modification_type := 'UPDATE',
            input_change_status := 'updated',
            input_fields := v_changed_fields,
            input_message := format('Updated %s field(s)', array_length(v_changed_fields, 1)),
            input_payload_before := v_before_data,
            input_payload_after := v_after_data,
            input_extra_metadata := jsonb_build_object(
                'changed_field_count', array_length(v_changed_fields, 1)
            )
        );
    END IF;
END;
$$ LANGUAGE plpgsql;
```

### Field-Level Auditing

With precise change tracking, you can implement field-level audit trails:

```sql
-- Audit table for field-level changes
CRETE TABLE audit.tb_field_changes (
    pk_field_change UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pk_organization UUID NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    field_name TEXT NOT NULL,
    old_value JSONB,
    new_value JSONB,
    changed_by UUID NOT NULL,
    changed_at TIMESTAMP DEFAULT NOW(),
    mutation_audit_id UUID REFERENCES audit.tb_mutation_log(pk_mutation_log)
);

-- Enhanced logging function with field-level audit
CREATE OR REPLACE FUNCTION core.log_field_level_changes(
    input_pk_organization UUID,
    input_entity_type TEXT,
    input_entity_id UUID,
    input_changed_by UUID,
    input_before_data JSONB,
    input_after_data JSONB,
    input_changed_fields TEXT[],
    input_mutation_audit_id UUID
) RETURNS VOID AS $$
DECLARE
    field_name TEXT;
BEGIN
    -- Log each changed field
    FOREACH field_name IN ARRAY input_changed_fields LOOP
        INSERT INTO audit.tb_field_changes (
            pk_organization,
            entity_type,
            entity_id,
            field_name,
            old_value,
            new_value,
            changed_by,
            mutation_audit_id
        ) VALUES (
            input_pk_organization,
            input_entity_type,
            input_entity_id,
            field_name,
            input_before_data -> field_name,
            input_after_data -> field_name,
            input_changed_by,
            input_mutation_audit_id
        );
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

## Examples

### Simple Create Mutation

Complete example of a create mutation using the mutation result pattern:

**PostgreSQL Function:**
```sql
CREATE OR REPLACE FUNCTION app.create_blog_post(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_new_post_id UUID;
    v_post_data JSONB;
    v_slug TEXT;
BEGIN
    -- Extract and validate required fields
    IF input_payload->>'title' IS NULL OR trim(input_payload->>'title') = '' THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_created_by,
            input_entity_type := 'blog_post',
            input_entity_id := NULL,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:invalid_title',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Title is required and cannot be empty',
            input_payload_before := NULL,
            input_payload_after := NULL,
            input_extra_metadata := jsonb_build_object(
                'validation_errors', jsonb_build_object('title', 'Required field')
            )
        );
    END IF;

    -- Generate slug
    v_slug := lower(regexp_replace(input_payload->>'title', '[^a-zA-Z0-9]+', '-', 'g'));

    -- Check for duplicate slug
    IF EXISTS (
        SELECT 1 FROM tenant.tb_blog_post
        WHERE pk_organization = input_pk_organization
          AND data->>'slug' = v_slug
    ) THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_created_by,
            input_entity_type := 'blog_post',
            input_entity_id := NULL,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:already_exists',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'A post with this title already exists',
            input_payload_before := NULL,
            input_payload_after := NULL,
            input_extra_metadata := jsonb_build_object(
                'conflict_field', 'slug',
                'conflict_value', v_slug
            )
        );
    END IF;

    -- Create the blog post
    v_new_post_id := gen_random_uuid();
    INSERT INTO tenant.tb_blog_post (
        pk_blog_post,
        pk_organization,
        data,
        created_by,
        created_at
    ) VALUES (
        v_new_post_id,
        input_pk_organization,
        input_payload || jsonb_build_object(
            'slug', v_slug,
            'status', 'draft',
            'view_count', 0
        ),
        input_created_by,
        NOW()
    );

    -- Get the complete post data from view
    SELECT to_jsonb(p.*) INTO v_post_data
    FROM public.v_blog_post p
    WHERE p.id = v_new_post_id;

    -- Return success result
    RETURN core.log_and_return_mutation(
        input_pk_organization := input_pk_organization,
        input_actor := input_created_by,
        input_entity_type := 'blog_post',
        input_entity_id := v_new_post_id,
        input_modification_type := 'INSERT',
        input_change_status := 'new',
        input_fields := ARRAY[]::TEXT[], -- Empty for creates
        input_message := 'Blog post created successfully',
        input_payload_before := NULL,
        input_payload_after := v_post_data,
        input_extra_metadata := jsonb_build_object(
            'generated_slug', v_slug,
            'word_count', array_length(string_to_array(input_payload->>'content', ' '), 1)
        )
    );
END;
$$ LANGUAGE plpgsql;
```

**Python Resolver:**
```python
@mutation
async def create_blog_post(
    info: GraphQLResolveInfo,
    input: CreateBlogPostInput
) -> CreateBlogPostSuccess | CreateBlogPostError:
    """Create a new blog post."""
    result = await info.context["db"].call_function(
        "app.create_blog_post",
        input_pk_organization=info.context["tenant_id"],
        input_created_by=info.context["user_id"],
        input_payload=input.to_dict()
    )

    if result["status"] == "new":
        return CreateBlogPostSuccess(
            blog_post=BlogPost.from_dict(result["object_data"]),
            message=result["message"],
            generated_slug=result["extra_metadata"]["generated_slug"]
        )
    else:
        error_code = result["status"].replace("noop:", "").upper()
        return CreateBlogPostError(
            message=result["message"],
            error_code=error_code
        )
```

### Update with Change Tracking

Update mutation that tracks precisely which fields changed:

**PostgreSQL Function:**
```sql
CREATE OR REPLACE FUNCTION app.update_blog_post(
    input_pk_organization UUID,
    input_pk_blog_post UUID,
    input_updated_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_before_data JSONB;
    v_after_data JSONB;
    v_changed_fields TEXT[];
    v_should_update_slug BOOLEAN := false;
    v_new_slug TEXT;
BEGIN
    -- Get current state
    SELECT to_jsonb(p.*) INTO v_before_data
    FROM public.v_blog_post p
    WHERE p.id = input_pk_blog_post
      AND p.tenant_id = input_pk_organization;

    IF v_before_data IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_updated_by,
            input_entity_type := 'blog_post',
            input_entity_id := input_pk_blog_post,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:not_found',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Blog post not found',
            input_payload_before := NULL,
            input_payload_after := NULL
        );
    END IF;

    -- Check if title changed (requires slug regeneration)
    IF input_payload ? 'title' AND
       input_payload->>'title' != v_before_data->>'title' THEN
        v_should_update_slug := true;
        v_new_slug := lower(regexp_replace(input_payload->>'title', '[^a-zA-Z0-9]+', '-', 'g'));

        -- Check for slug conflicts
        IF EXISTS (
            SELECT 1 FROM tenant.tb_blog_post
            WHERE pk_organization = input_pk_organization
              AND data->>'slug' = v_new_slug
              AND pk_blog_post != input_pk_blog_post
        ) THEN
            RETURN core.log_and_return_mutation(
                input_pk_organization := input_pk_organization,
                input_actor := input_updated_by,
                input_entity_type := 'blog_post',
                input_entity_id := input_pk_blog_post,
                input_modification_type := 'NOOP',
                input_change_status := 'noop:slug_conflict',
                input_fields := ARRAY[]::TEXT[],
                input_message := 'Title change would create conflicting slug',
                input_payload_before := v_before_data,
                input_payload_after := v_before_data,
                input_extra_metadata := jsonb_build_object(
                    'conflicting_slug', v_new_slug
                )
            );
        END IF;
    END IF;

    -- Prepare update payload with slug if needed
    IF v_should_update_slug THEN
        input_payload := input_payload || jsonb_build_object('slug', v_new_slug);
    END IF;

    -- Update the record
    UPDATE tenant.tb_blog_post
    SET
        data = data || input_payload,
        updated_at = NOW(),
        updated_by = input_updated_by
    WHERE pk_blog_post = input_pk_blog_post
      AND pk_organization = input_pk_organization;

    -- Get updated state
    SELECT to_jsonb(p.*) INTO v_after_data
    FROM public.v_blog_post p
    WHERE p.id = input_pk_blog_post;

    -- Calculate changed fields
    v_changed_fields := core.calculate_changed_fields(v_before_data, v_after_data);

    IF array_length(v_changed_fields, 1) = 0 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_updated_by,
            input_entity_type := 'blog_post',
            input_entity_id := input_pk_blog_post,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:no_changes',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'No changes were made',
            input_payload_before := v_before_data,
            input_payload_after := v_after_data
        );
    ELSE
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_updated_by,
            input_entity_type := 'blog_post',
            input_entity_id := input_pk_blog_post,
            input_modification_type := 'UPDATE',
            input_change_status := 'updated',
            input_fields := v_changed_fields,
            input_message := format('Updated %s field(s): %s',
                array_length(v_changed_fields, 1),
                array_to_string(v_changed_fields, ', ')
            ),
            input_payload_before := v_before_data,
            input_payload_after := v_after_data,
            input_extra_metadata := jsonb_build_object(
                'regenerated_slug', v_should_update_slug,
                'field_count', array_length(v_changed_fields, 1)
            )
        );
    END IF;
END;
$$ LANGUAGE plpgsql;
```

### NOOP Handling Scenario

Example showing comprehensive NOOP handling:

**Delete with Dependencies:**
```sql
CREATE OR REPLACE FUNCTION app.delete_blog_post(
    input_pk_organization UUID,
    input_pk_blog_post UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result AS $$
DECLARE
    v_post_data JSONB;
    v_comment_count INTEGER;
BEGIN
    -- Check if post exists
    SELECT to_jsonb(p.*) INTO v_post_data
    FROM public.v_blog_post p
    WHERE p.id = input_pk_blog_post
      AND p.tenant_id = input_pk_organization;

    IF v_post_data IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_deleted_by,
            input_entity_type := 'blog_post',
            input_entity_id := input_pk_blog_post,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:not_found',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Blog post not found or already deleted',
            input_payload_before := NULL,
            input_payload_after := NULL
        );
    END IF;

    -- Check if post is published and has comments
    SELECT COUNT(*) INTO v_comment_count
    FROM tenant.tb_blog_comment
    WHERE pk_blog_post = input_pk_blog_post
      AND deleted_at IS NULL;

    IF v_post_data->>'status' = 'published' AND v_comment_count > 0 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_deleted_by,
            input_entity_type := 'blog_post',
            input_entity_id := input_pk_blog_post,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:cannot_delete_has_children',
            input_fields := ARRAY[]::TEXT[],
            input_message := format('Cannot delete published post with %s comments', v_comment_count),
            input_payload_before := v_post_data,
            input_payload_after := v_post_data,
            input_extra_metadata := jsonb_build_object(
                'comment_count', v_comment_count,
                'post_status', v_post_data->>'status',
                'suggested_action', 'unpublish_first'
            )
        );
    END IF;

    -- Soft delete the post
    UPDATE tenant.tb_blog_post
    SET
        deleted_at = NOW(),
        deleted_by = input_deleted_by,
        data = data || jsonb_build_object('status', 'deleted')
    WHERE pk_blog_post = input_pk_blog_post
      AND pk_organization = input_pk_organization;

    RETURN core.log_and_return_mutation(
        input_pk_organization := input_pk_organization,
        input_actor := input_deleted_by,
        input_entity_type := 'blog_post',
        input_entity_id := input_pk_blog_post,
        input_modification_type := 'DELETE',
        input_change_status := 'deleted',
        input_fields := ARRAY['status']::TEXT[], -- Status changed to deleted
        input_message := 'Blog post deleted successfully',
        input_payload_before := v_post_data,
        input_payload_after := NULL, -- NULL for soft deletes
        input_extra_metadata := jsonb_build_object(
            'deletion_type', 'soft_delete',
            'had_comments', v_comment_count > 0
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### Complex Business Logic

Advanced mutation with multiple business rules and state transitions:

**Publish Blog Post with Workflow:**
```sql
CREATE OR REPLACE FUNCTION app.publish_blog_post(
    input_pk_organization UUID,
    input_pk_blog_post UUID,
    input_published_by UUID,
    input_publish_options JSONB DEFAULT '{}'::JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_post_data JSONB;
    v_before_data JSONB;
    v_after_data JSONB;
    v_user_permissions TEXT[];
    v_scheduled_date TIMESTAMP;
    v_notification_sent BOOLEAN := false;
BEGIN
    -- Get current post state
    SELECT to_jsonb(p.*) INTO v_post_data
    FROM public.v_blog_post p
    WHERE p.id = input_pk_blog_post
      AND p.tenant_id = input_pk_organization;

    v_before_data := v_post_data;

    -- Validation checks
    IF v_post_data IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_published_by,
            input_entity_type := 'blog_post',
            input_entity_id := input_pk_blog_post,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:not_found',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Blog post not found',
            input_payload_before := NULL,
            input_payload_after := NULL
        );
    END IF;

    -- Check current status
    IF v_post_data->>'status' = 'published' THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_published_by,
            input_entity_type := 'blog_post',
            input_entity_id := input_pk_blog_post,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:already_published',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Blog post is already published',
            input_payload_before := v_post_data,
            input_payload_after := v_post_data,
            input_extra_metadata := jsonb_build_object(
                'current_status', v_post_data->>'status',
                'published_at', v_post_data->>'published_at'
            )
        );
    END IF;

    -- Validate required content
    IF v_post_data->>'title' IS NULL OR trim(v_post_data->>'title') = '' OR
       v_post_data->>'content' IS NULL OR length(trim(v_post_data->>'content')) < 100 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_published_by,
            input_entity_type := 'blog_post',
            input_entity_id := input_pk_blog_post,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:invalid_content',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Post requires title and at least 100 characters of content',
            input_payload_before := v_post_data,
            input_payload_after := v_post_data,
            input_extra_metadata := jsonb_build_object(
                'content_length', length(coalesce(v_post_data->>'content', '')),
                'min_required', 100
            )
        );
    END IF;

    -- Handle scheduled publishing
    v_scheduled_date := COALESCE(
        (input_publish_options->>'scheduled_for')::TIMESTAMP,
        NOW()
    );

    -- Check user permissions for future scheduling
    IF v_scheduled_date > NOW() THEN
        SELECT array_agg(permission) INTO v_user_permissions
        FROM user_permissions
        WHERE user_id = input_published_by;

        IF NOT 'schedule_posts' = ANY(v_user_permissions) THEN
            RETURN core.log_and_return_mutation(
                input_pk_organization := input_pk_organization,
                input_actor := input_published_by,
                input_entity_type := 'blog_post',
                input_entity_id := input_pk_blog_post,
                input_modification_type := 'NOOP',
                input_change_status := 'noop:insufficient_permissions',
                input_fields := ARRAY[]::TEXT[],
                input_message := 'User lacks permission to schedule posts',
                input_payload_before := v_post_data,
                input_payload_after := v_post_data,
                input_extra_metadata := jsonb_build_object(
                    'required_permission', 'schedule_posts',
                    'scheduled_for', v_scheduled_date
                )
            );
        END IF;
    END IF;

    -- Update post status
    UPDATE tenant.tb_blog_post
    SET
        data = data || jsonb_build_object(
            'status', CASE
                WHEN v_scheduled_date > NOW() THEN 'scheduled'
                ELSE 'published'
            END,
            'published_at', v_scheduled_date,
            'published_by', input_published_by
        ),
        updated_at = NOW(),
        updated_by = input_published_by
    WHERE pk_blog_post = input_pk_blog_post
      AND pk_organization = input_pk_organization;

    -- Send notification if published immediately
    IF v_scheduled_date <= NOW() THEN
        PERFORM pg_notify('blog_post_published', json_build_object(
            'post_id', input_pk_blog_post,
            'title', v_post_data->>'title'
        )::text);
        v_notification_sent := true;
    END IF;

    -- Get updated state
    SELECT to_jsonb(p.*) INTO v_after_data
    FROM public.v_blog_post p
    WHERE p.id = input_pk_blog_post;

    RETURN core.log_and_return_mutation(
        input_pk_organization := input_pk_organization,
        input_actor := input_published_by,
        input_entity_type := 'blog_post',
        input_entity_id := input_pk_blog_post,
        input_modification_type := 'UPDATE',
        input_change_status := 'updated',
        input_fields := ARRAY['status', 'published_at', 'published_by']::TEXT[],
        input_message := CASE
            WHEN v_scheduled_date > NOW() THEN format('Post scheduled for publishing at %s', v_scheduled_date)
            ELSE 'Post published successfully'
        END,
        input_payload_before := v_before_data,
        input_payload_after := v_after_data,
        input_extra_metadata := jsonb_build_object(
            'scheduled', v_scheduled_date > NOW(),
            'publish_date', v_scheduled_date,
            'notification_sent', v_notification_sent,
            'workflow_step', 'publish_complete'
        )
    );
END;
$$ LANGUAGE plpgsql;
```

## Error Handling Patterns

### Validation Failures

Validation errors should be handled consistently using NOOP status codes:

```sql
-- Multi-field validation example
CREATE OR REPLACE FUNCTION app.validate_user_data(
    input_data JSONB
) RETURNS JSONB AS $$
DECLARE
    validation_errors JSONB := '{}'::JSONB;
BEGIN
    -- Email validation
    IF input_data ? 'email' THEN
        IF input_data->>'email' !~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$' THEN
            validation_errors := validation_errors ||
                jsonb_build_object('email', 'Invalid email format');
        END IF;
    END IF;

    -- Age validation
    IF input_data ? 'age' THEN
        IF (input_data->>'age')::INT < 18 OR (input_data->>'age')::INT > 120 THEN
            validation_errors := validation_errors ||
                jsonb_build_object('age', 'Age must be between 18 and 120');
        END IF;
    END IF;

    -- Phone validation
    IF input_data ? 'phone' THEN
        IF input_data->>'phone' !~ '^\+?[1-9]\d{1,14}$' THEN
            validation_errors := validation_errors ||
                jsonb_build_object('phone', 'Invalid phone number format');
        END IF;
    END IF;

    RETURN validation_errors;
END;
$$ LANGUAGE plpgsql;

-- Usage in mutation functions
CREATE OR REPLACE FUNCTION app.create_user_with_validation(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_validation_errors JSONB;
BEGIN
    -- Validate input data
    v_validation_errors := app.validate_user_data(input_payload);

    IF v_validation_errors != '{}'::JSONB THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_created_by,
            input_entity_type := 'user',
            input_entity_id := NULL,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:validation_failed',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Validation failed for multiple fields',
            input_payload_before := NULL,
            input_payload_after := NULL,
            input_extra_metadata := jsonb_build_object(
                'validation_errors', v_validation_errors
            )
        );
    END IF;

    -- Continue with creation...
END;
$$ LANGUAGE plpgsql;
```

### Business Rule Violations

Business rule violations should use descriptive NOOP status codes:

```sql
-- Business rule validation example
CREATE OR REPLACE FUNCTION app.transfer_project_ownership(
    input_pk_organization UUID,
    input_pk_project UUID,
    input_new_owner_id UUID,
    input_transferred_by UUID
) RETURNS app.mutation_result AS $$
DECLARE
    v_project_data JSONB;
    v_new_owner_data JSONB;
    v_active_tasks_count INTEGER;
BEGIN
    -- Get project data
    SELECT to_jsonb(p.*) INTO v_project_data
    FROM public.v_project p
    WHERE p.id = input_pk_project;

    -- Business Rule 1: Project must exist
    IF v_project_data IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_transferred_by,
            input_entity_type := 'project',
            input_entity_id := input_pk_project,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:not_found',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Project not found',
            input_payload_before := NULL,
            input_payload_after := NULL
        );
    END IF;

    -- Business Rule 2: Cannot transfer to same owner
    IF v_project_data->>'owner_id' = input_new_owner_id::TEXT THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_transferred_by,
            input_entity_type := 'project',
            input_entity_id := input_pk_project,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:same_owner',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Project is already owned by this user',
            input_payload_before := v_project_data,
            input_payload_after := v_project_data
        );
    END IF;

    -- Business Rule 3: New owner must exist and be active
    SELECT to_jsonb(u.*) INTO v_new_owner_data
    FROM public.v_user u
    WHERE u.id = input_new_owner_id
      AND u.tenant_id = input_pk_organization
      AND u.status = 'active';

    IF v_new_owner_data IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_transferred_by,
            input_entity_type := 'project',
            input_entity_id := input_pk_project,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:invalid_new_owner',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'New owner must be an active user in this organization',
            input_payload_before := v_project_data,
            input_payload_after := v_project_data,
            input_extra_metadata := jsonb_build_object(
                'attempted_new_owner_id', input_new_owner_id
            )
        );
    END IF;

    -- Business Rule 4: Cannot transfer project with active critical tasks
    SELECT COUNT(*) INTO v_active_tasks_count
    FROM tenant.tb_task
    WHERE pk_project = input_pk_project
      AND data->>'status' IN ('in_progress', 'blocked')
      AND data->>'priority' = 'critical';

    IF v_active_tasks_count > 0 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_transferred_by,
            input_entity_type := 'project',
            input_entity_id := input_pk_project,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:has_critical_tasks',
            input_fields := ARRAY[]::TEXT[],
            input_message := format('Cannot transfer project with %s active critical tasks', v_active_tasks_count),
            input_payload_before := v_project_data,
            input_payload_after := v_project_data,
            input_extra_metadata := jsonb_build_object(
                'critical_tasks_count', v_active_tasks_count,
                'suggested_action', 'complete_or_reassign_critical_tasks'
            )
        );
    END IF;

    -- All business rules passed, perform the transfer
    UPDATE tenant.tb_project
    SET
        data = data || jsonb_build_object(
            'owner_id', input_new_owner_id,
            'previous_owner_id', v_project_data->>'owner_id',
            'transferred_at', NOW()
        ),
        updated_by = input_transferred_by,
        updated_at = NOW()
    WHERE pk_project = input_pk_project;

    -- Return success
    RETURN core.log_and_return_mutation(
        input_pk_organization := input_pk_organization,
        input_actor := input_transferred_by,
        input_entity_type := 'project',
        input_entity_id := input_pk_project,
        input_modification_type := 'UPDATE',
        input_change_status := 'updated',
        input_fields := ARRAY['owner_id', 'previous_owner_id', 'transferred_at']::TEXT[],
        input_message := 'Project ownership transferred successfully',
        input_payload_before := v_project_data,
        input_payload_after := (SELECT to_jsonb(p.*) FROM public.v_project p WHERE p.id = input_pk_project),
        input_extra_metadata := jsonb_build_object(
            'transfer_type', 'ownership_change',
            'new_owner_email', v_new_owner_data->>'email'
        )
    );
END;
$$ LANGUAGE plpgsql;
```

## Migration Guide

### Converting Existing Mutations

**Step 1: Identify Current Return Pattern**

First, catalog your existing mutation return formats:

```sql
-- OLD: Ad-hoc return format
CREATE FUNCTION fn_old_create_user(input_data JSON)
RETURNS JSON AS $$
BEGIN
    -- ... logic
    RETURN json_build_object(
        'success', true,
        'user_id', new_user_id,
        'message', 'User created'
    );
EXCEPTION
    WHEN unique_violation THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Email already exists'
        );
END;
$$ LANGUAGE plpgsql;
```

**Step 2: Create New Function with Mutation Result Pattern**

```sql
-- NEW: Mutation result pattern
CREATE FUNCTION app.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_new_user_id UUID;
    v_user_data JSONB;
BEGIN
    -- Check for duplicate email
    IF EXISTS (
        SELECT 1 FROM tenant.tb_user
        WHERE pk_organization = input_pk_organization
          AND data->>'email' = input_payload->>'email'
    ) THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_created_by,
            input_entity_type := 'user',
            input_entity_id := NULL,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:already_exists',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Email already exists',
            input_payload_before := NULL,
            input_payload_after := NULL,
            input_extra_metadata := jsonb_build_object(
                'conflict_field', 'email',
                'conflict_value', input_payload->>'email'
            )
        );
    END IF;

    -- Create user
    v_new_user_id := gen_random_uuid();
    INSERT INTO tenant.tb_user (pk_user, pk_organization, data, created_by)
    VALUES (v_new_user_id, input_pk_organization, input_payload, input_created_by);

    -- Get complete user data
    SELECT to_jsonb(u.*) INTO v_user_data
    FROM public.v_user u WHERE u.id = v_new_user_id;

    RETURN core.log_and_return_mutation(
        input_pk_organization := input_pk_organization,
        input_actor := input_created_by,
        input_entity_type := 'user',
        input_entity_id := v_new_user_id,
        input_modification_type := 'INSERT',
        input_change_status := 'new',
        input_fields := ARRAY[]::TEXT[],
        input_message := 'User created successfully',
        input_payload_before := NULL,
        input_payload_after := v_user_data
    );
END;
$$ LANGUAGE plpgsql;
```

**Step 3: Update Python Resolvers**

```python
# OLD: Direct JSON handling
@mutation
async def create_user_old(info, input: CreateUserInput):
    result = await info.context["db"].execute_function(
        "fn_old_create_user", input.to_dict()
    )

    if result["success"]:
        user = await get_user_by_id(result["user_id"])
        return CreateUserSuccess(user=user)
    else:
        return CreateUserError(message=result["error"])

# NEW: Mutation result pattern
@mutation
async def create_user(
    info: GraphQLResolveInfo,
    input: CreateUserInput
) -> CreateUserSuccess | CreateUserError:
    result = await info.context["db"].call_function(
        "app.create_user",
        input_pk_organization=info.context["tenant_id"],
        input_created_by=info.context["user_id"],
        input_payload=input.to_dict()
    )

    if result["status"] == "new":
        return CreateUserSuccess(
            user=User.from_dict(result["object_data"]),
            message=result["message"]
        )
    else:
        error_code = result["status"].replace("noop:", "").upper()
        return CreateUserError(
            message=result["message"],
            error_code=error_code
        )
```

### Backward Compatibility

**Option 1: Wrapper Functions**

Create wrapper functions that convert mutation results back to old format:

```sql
-- Compatibility wrapper
CREATE FUNCTION fn_create_user_compat(input_data JSON)
RETURNS JSON AS $$
DECLARE
    v_result app.mutation_result;
BEGIN
    -- Call new function
    v_result := app.create_user(
        (input_data->>'tenant_id')::UUID,
        (input_data->>'user_id')::UUID,
        input_data
    );

    -- Convert to old format
    IF v_result.status IN ('new', 'updated') THEN
        RETURN json_build_object(
            'success', true,
            'user_id', v_result.id,
            'message', v_result.message
        );
    ELSE
        RETURN json_build_object(
            'success', false,
            'error', v_result.message,
            'code', v_result.status
        );
    END IF;
END;
$$ LANGUAGE plpgsql;
```

**Option 2: Gradual Migration**

1. Deploy new functions alongside old ones
2. Update resolvers one by one
3. Migrate client code gradually
4. Remove old functions when migration is complete

## Best Practices

### Do's and Don'ts

**✅ DO:**

- **Always use core.log_and_return_mutation** for consistent results
- **Use descriptive NOOP status codes** like `noop:invalid_email` instead of generic `noop:validation_failed`
- **Include meaningful metadata** in the `extra_metadata` field for debugging
- **Calculate changed fields accurately** using proper before/after comparison
- **Validate input early** and return NOOP status for validation failures
- **Use consistent field naming** across all mutation functions
- **Log audit information** for all mutation attempts, even NOOPs
- **Return complete object data** from views, not internal table data

**❌ DON'T:**

- **Don't throw exceptions** for business logic failures - use NOOP status codes
- **Don't return inconsistent message formats** - stick to the mutation result pattern
- **Don't skip audit logging** - every mutation should be logged
- **Don't expose internal IDs** - use the proper UUID mapping from command to query side
- **Don't forget tenant context** - always validate pk_organization
- **Don't return sensitive data** in error messages or metadata
- **Don't perform mutations without proper authorization checks**

### Performance Benefits

**Network Efficiency Through Rich Object Returns**

The mutation result pattern dramatically improves frontend performance by returning complete entity data in the mutation response, eliminating the need for additional API calls:

```graphql
# EFFICIENT: Single mutation call returns complete data
mutation CreateUser($input: CreateUserInput!) {
  createUser(input: $input) {
    __typename
    ... on CreateUserSuccess {
      user {
        id
        email
        name
        department {
          id
          name
        }
        permissions {
          id
          name
          scope
        }
        profile {
          avatar_url
          timezone
          preferences
        }
      }
      message
      changedFields
    }
  }
}

# INEFFICIENT: Traditional approach requires multiple calls
# 1. Create user
# 2. Fetch user details
# 3. Fetch department info
# 4. Fetch permissions
# 5. Fetch profile data
```

**Performance Metrics:**

- **Network Requests**: Reduced from 3-5 requests to 1 request
- **Network Latency**: 70-80% reduction in total request time
- **Bandwidth Usage**: Optimized through single comprehensive response
- **Frontend Complexity**: Simplified state management with complete data
- **Backend Load**: Reduced API server load from fewer concurrent requests

**Resource Optimization:**
```python
# BEFORE: Multiple API calls
async def create_user_old_way(input_data):
    # 1. Create user (120ms network round-trip)
    user_response = await api.create_user(input_data)
    user_id = user_response["user_id"]

    # 2. Fetch complete user data (150ms)
    user_details = await api.get_user(user_id)

    # 3. Fetch department data (100ms)
    department = await api.get_department(user_details["department_id"])

    # 4. Fetch permissions (130ms)
    permissions = await api.get_user_permissions(user_id)

    # Total: 500ms + processing time + potential retry delays

# AFTER: Single mutation with rich object return
async def create_user_new_way(input_data):
    # Single call returns everything (180ms)
    result = await api.create_user_comprehensive(input_data)
    # Complete user object with all relationships included
    # Total: 180ms - 64% time reduction
```

### Debezium/CDC Integration

The mutation result pattern naturally aligns with Change Data Capture (CDC) patterns, making it ideal for event streaming architectures:

**Debezium-Compatible Event Structure:**
```json
{
  "schema": {
    "type": "struct",
    "fields": [
      {"field": "before", "type": "struct", "optional": true},
      {"field": "after", "type": "struct", "optional": true},
      {"field": "source", "type": "struct"},
      {"field": "op", "type": "string"},
      {"field": "ts_ms", "type": "int64"}
    ]
  },
  "payload": {
    "before": null,
    "after": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "email": "user@example.com",
      "status": "active"
    },
    "source": {
      "version": "1.9.0",
      "connector": "fraiseql",
      "name": "app_mutations",
      "ts_ms": 1640995200000,
      "db": "app_db",
      "schema": "public",
      "table": "v_user"
    },
    "op": "c",
    "ts_ms": 1640995200000
  }
}
```

**FraiseQL to Debezium Transformation:**
```python
def mutation_result_to_debezium_event(mutation_result: dict) -> dict:
    """Transform FraiseQL mutation result to Debezium-compatible event."""

    # Map FraiseQL status to Debezium operation
    status_to_op = {
        "new": "c",      # Create
        "updated": "u",  # Update
        "deleted": "d"   # Delete
    }

    op = status_to_op.get(mutation_result["status"], "c")

    return {
        "schema": get_debezium_schema(mutation_result["entity_type"]),
        "payload": {
            "before": mutation_result["payload_before"],
            "after": mutation_result["object_data"],
            "source": {
                "version": "2.0.0",
                "connector": "fraiseql-mutation-log",
                "name": f"fraiseql_{mutation_result['entity_type']}",
                "ts_ms": int(mutation_result["extra_metadata"]["created_at"].timestamp() * 1000),
                "db": "fraiseql_app",
                "schema": "public",
                "table": f"v_{mutation_result['entity_type']}",
                "mutation_id": mutation_result["extra_metadata"]["audit_id"],
                "actor": mutation_result["extra_metadata"]["actor_id"],
                "changed_fields": mutation_result["updated_fields"]
            },
            "op": op,
            "ts_ms": int(mutation_result["extra_metadata"]["created_at"].timestamp() * 1000)
        }
    }

# Usage in event streaming
async def stream_mutation_events(mutation_result: dict):
    """Stream mutation as Debezium-compatible event."""

    # Convert to Debezium format
    debezium_event = mutation_result_to_debezium_event(mutation_result)

    # Send to Kafka/event stream
    await kafka_producer.send(
        topic=f"app.mutations.{mutation_result['entity_type']}",
        value=debezium_event
    )

    # Also send enriched business event
    business_event = {
        "event_type": f"{mutation_result['entity_type']}.{mutation_result['status']}",
        "entity_id": mutation_result["id"],
        "entity_data": mutation_result["object_data"],
        "changed_fields": mutation_result["updated_fields"],
        "metadata": mutation_result["extra_metadata"],
        "timestamp": mutation_result["extra_metadata"]["created_at"]
    }

    await kafka_producer.send(
        topic=f"business-events.{mutation_result['entity_type']}",
        value=business_event
    )
```

**Event Streaming Benefits:**

- **Real-time Sync** - Immediate propagation of changes to downstream systems
- **Audit Trail** - Complete change history for compliance and debugging
- **Microservice Integration** - Other services can react to entity changes
- **Analytics Pipeline** - Rich metadata feeds business intelligence systems
- **Cache Invalidation** - Automatic cache updates based on change events

### Performance Considerations

**1. Minimize View Queries**
```sql
-- GOOD: Get view data once at the end
DECLARE
    v_user_data JSONB;
BEGIN
    -- ... do all mutations first

    -- Get final state once
    SELECT to_jsonb(u.*) INTO v_user_data
    FROM public.v_user u WHERE u.id = v_user_id;

    RETURN core.log_and_return_mutation(...);
END;

-- BAD: Multiple view queries
BEGIN
    SELECT to_jsonb(u.*) INTO v_before FROM public.v_user u WHERE u.id = v_user_id;
    -- ... mutations
    SELECT to_jsonb(u.*) INTO v_after FROM public.v_user u WHERE u.id = v_user_id;
    -- ... more queries
END;
```

**2. Use Efficient Change Detection**
```sql
-- GOOD: Smart field comparison
v_changed_fields := core.calculate_changed_fields(v_before_data, v_after_data);

-- BAD: Manual field-by-field comparison
IF v_before_data->>'email' != v_after_data->>'email' THEN
    v_changed_fields := array_append(v_changed_fields, 'email');
END IF;
-- ... repeat for every field
```

**3. Batch Audit Logging**
```sql
-- For batch operations, consider batched audit inserts
INSERT INTO audit.tb_mutation_log (...)
SELECT ... FROM unnest($1::UUID[], $2::TEXT[], ...) AS batch_data;
```

## Troubleshooting

### Common Issues

**1. "Function does not exist" Error**
```
ERROR: function app.create_user(uuid, uuid, jsonb) does not exist
```

**Solution:**

- Verify function exists in the correct schema
- Check parameter types match exactly
- Ensure migrations have been applied

**2. "Column 'id' does not exist" Error**
```
ERROR: column "id" does not exist in relation "tb_user"
```

**Solution:**

- Use `pk_[entity]` for command-side tables
- Use `id` only when querying views
- Check your table structure

**3. Empty `object_data` Field**
```json
{
  "status": "new",
  "object_data": null
}
```

**Solution:**

- Ensure the view query returns data
- Check tenant isolation - entity may not be visible
- Verify the view includes the new entity

**4. Audit Log Not Created**

**Solution:**

- Check if `audit` schema exists
- Verify permissions for audit table
- Ensure `core.log_and_return_mutation` is being called

**5. NOOP Status Not Handled in Client**
```
GraphQL Error: Cannot return null for non-nullable field
```

**Solution:**

- Ensure all NOOP statuses are mapped to error types in resolver
- Add proper error handling for unexpected status codes

### Debugging Techniques

**1. Add Debug Metadata**
```sql
input_extra_metadata := jsonb_build_object(
    'debug_info', jsonb_build_object(
        'function_name', 'app.create_user',
        'input_size_bytes', octet_length(input_payload::text),
        'execution_time_ms', EXTRACT(MILLISECONDS FROM clock_timestamp() - v_start_time),
        'checks_performed', ARRAY['email_unique', 'org_valid', 'permissions']
    )
);
```

**2. Enable SQL Logging**
```sql
-- Temporarily log intermediate states
RAISE NOTICE 'Before state: %', v_before_data;
RAISE NOTICE 'After state: %', v_after_data;
RAISE NOTICE 'Changed fields: %', v_changed_fields;
```

**3. Query Audit Logs**
```sql
-- Find recent mutations for debugging
SELECT
    created_at,
    entity_type,
    change_status,
    message,
    extra_metadata
FROM audit.tb_mutation_log
WHERE entity_id = 'your-entity-uuid'
ORDER BY created_at DESC
LIMIT 10;
```

**4. Validate Mutation Result Structure**
```python
# In your resolver
result = await db.call_function(...)

# Add validation
required_fields = ['id', 'status', 'message', 'object_data', 'updated_fields', 'extra_metadata']
for field in required_fields:
    if field not in result:
        logger.error(f"Missing required field '{field}' in mutation result")
        raise ValueError(f"Invalid mutation result structure")
```

## Integration Points

### Authentication and Authorization

The mutation result pattern integrates seamlessly with FraiseQL's authentication and authorization systems:

```python
@mutation
async def secure_mutation(
    info: GraphQLResolveInfo,
    input: SecureMutationInput
) -> SecureMutationSuccess | SecureMutationError:
    """Mutation with integrated auth checks."""

    # Authentication check
    user_id = info.context.get("user_id")
    if not user_id:
        return SecureMutationError(
            message="Authentication required",
            error_code="UNAUTHENTICATED"
        )

    # Authorization check
    permissions = info.context.get("permissions", [])
    if "manage_users" not in permissions:
        return SecureMutationError(
            message="Insufficient permissions",
            error_code="INSUFFICIENT_PERMISSIONS"
        )

    # Call mutation function with authenticated context
    result = await info.context["db"].call_function(
        "app.secure_mutation",
        input_pk_organization=info.context["tenant_id"],
        input_actor=user_id,
        input_payload=input.to_dict()
    )

    # Authorization failures are handled as NOOPs in the database
    if result["status"] == "noop:insufficient_permissions":
        return SecureMutationError(
            message=result["message"],
            error_code="INSUFFICIENT_PERMISSIONS"
        )

    # Handle success cases
    if result["status"] in ["new", "updated"]:
        return SecureMutationSuccess(
            entity=Entity.from_dict(result["object_data"]),
            message=result["message"]
        )
```

**Database-Level Authorization:**
```sql
CREATE OR REPLACE FUNCTION app.secure_user_update(
    input_pk_organization UUID,
    input_pk_user UUID,
    input_actor UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
BEGIN
    -- Check if actor has permission to modify this user
    IF NOT EXISTS (
        SELECT 1 FROM public.v_user_permissions up
        WHERE up.user_id = input_actor
          AND up.permission = 'manage_users'
          AND up.tenant_id = input_pk_organization
    ) THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_actor,
            input_entity_type := 'user',
            input_entity_id := input_pk_user,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:insufficient_permissions',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'User lacks permission to modify users',
            input_payload_before := NULL,
            input_payload_after := NULL,
            input_extra_metadata := jsonb_build_object(
                'required_permission', 'manage_users',
                'actor_permissions', (
                    SELECT array_agg(permission)
                    FROM public.v_user_permissions
                    WHERE user_id = input_actor
                )
            )
        );
    END IF;

    -- Continue with authorized mutation...
END;
$$ LANGUAGE plpgsql;
```

### Multi-Tenant Patterns

Tenant isolation is enforced at every level of the mutation result pattern:

```sql
-- Tenant-aware mutation function
CREATE OR REPLACE FUNCTION app.create_document(
    input_pk_organization UUID,  -- Always first parameter
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_document_id UUID;
    v_document_data JSONB;
BEGIN
    -- Verify user belongs to the organization
    IF NOT EXISTS (
        SELECT 1 FROM public.v_user u
        WHERE u.id = input_created_by
          AND u.tenant_id = input_pk_organization
          AND u.status = 'active'
    ) THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_created_by,
            input_entity_type := 'document',
            input_entity_id := NULL,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:tenant_mismatch',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'User does not belong to this organization',
            input_payload_before := NULL,
            input_payload_after := NULL
        );
    END IF;

    -- Check for duplicate title within tenant
    IF EXISTS (
        SELECT 1 FROM tenant.tb_document
        WHERE pk_organization = input_pk_organization  -- Tenant isolation
          AND data->>'title' = input_payload->>'title'
    ) THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization := input_pk_organization,
            input_actor := input_created_by,
            input_entity_type := 'document',
            input_entity_id := NULL,
            input_modification_type := 'NOOP',
            input_change_status := 'noop:already_exists',
            input_fields := ARRAY[]::TEXT[],
            input_message := 'Document with this title already exists',
            input_payload_before := NULL,
            input_payload_after := NULL
        );
    END IF;

    -- Create document with tenant context
    v_document_id := gen_random_uuid();
    INSERT INTO tenant.tb_document (
        pk_document,
        pk_organization,  -- Tenant foreign key
        data,
        created_by
    ) VALUES (
        v_document_id,
        input_pk_organization,
        input_payload,
        input_created_by
    );

    -- Get document data (view automatically filters by tenant)
    SELECT to_jsonb(d.*) INTO v_document_data
    FROM public.v_document d
    WHERE d.id = v_document_id
      AND d.tenant_id = input_pk_organization;  -- Explicit tenant check

    RETURN core.log_and_return_mutation(
        input_pk_organization := input_pk_organization,
        input_actor := input_created_by,
        input_entity_type := 'document',
        input_entity_id := v_document_id,
        input_modification_type := 'INSERT',
        input_change_status := 'new',
        input_fields := ARRAY[]::TEXT[],
        input_message := 'Document created successfully',
        input_payload_before := NULL,
        input_payload_after := v_document_data
    );
END;
$$ LANGUAGE plpgsql;
```

### Cache Invalidation

Mutation results can trigger cache invalidation automatically:

```python
@mutation
async def update_user_with_cache(
    info: GraphQLResolveInfo,
    id: UUID,
    input: UpdateUserInput
) -> UpdateUserSuccess | UpdateUserError:
    """Update user with automatic cache invalidation."""

    result = await info.context["db"].call_function(
        "app.update_user",
        input_pk_organization=info.context["tenant_id"],
        input_pk_user=id,
        input_updated_by=info.context["user_id"],
        input_payload=input.to_dict()
    )

    # Handle successful updates with cache invalidation
    if result["status"] == "updated":
        # Get the list of changed fields from mutation result
        changed_fields = result["updated_fields"]

        # Invalidate specific caches based on what changed
        cache = info.context["cache"]

        if "email" in changed_fields:
            await cache.invalidate(f"user:email:{result['object_data']['email']}")
            await cache.invalidate(f"user:old_email:{result['extra_metadata'].get('previous_email')}")

        if "name" in changed_fields:
            await cache.invalidate(f"user:search:*")  # Invalidate name-based searches

        if "status" in changed_fields:
            await cache.invalidate(f"users:active:tenant:{info.context['tenant_id']}")

        # Always invalidate the specific user cache
        await cache.invalidate(f"user:{id}")

        return UpdateUserSuccess(
            user=User.from_dict(result["object_data"]),
            message=result["message"],
            changed_fields=changed_fields
        )
    elif result["status"] == "noop:no_changes":
        # No cache invalidation needed for no-op
        return UpdateUserSuccess(
            user=User.from_dict(result["object_data"]),
            message="No changes were needed",
            changed_fields=[]
        )
    else:
        error_code = result["status"].replace("noop:", "").upper()
        return UpdateUserError(
            message=result["message"],
            error_code=error_code
        )
```

**Cache Invalidation Patterns:**
```python
class CacheInvalidationService:
    def __init__(self, cache_client):
        self.cache = cache_client

    async def invalidate_for_mutation_result(
        self,
        entity_type: str,
        entity_id: UUID,
        result: dict
    ):
        """Automatically invalidate caches based on mutation result."""

        if result["status"] in ["new", "updated", "deleted"]:
            # Always invalidate the specific entity
            await self.cache.invalidate(f"{entity_type}:{entity_id}")

            # Invalidate list caches that might include this entity
            tenant_id = result["extra_metadata"].get("tenant_id")
            if tenant_id:
                await self.cache.invalidate(f"{entity_type}:list:tenant:{tenant_id}")

            # Field-specific invalidations
            if "updated_fields" in result and result["updated_fields"]:
                for field in result["updated_fields"]:
                    if field in ["name", "title", "slug"]:
                        # Invalidate search caches
                        await self.cache.invalidate(f"{entity_type}:search:*")
                    elif field == "status":
                        # Invalidate status-filtered lists
                        await self.cache.invalidate(f"{entity_type}:status:*")

            # Handle relationships
            if entity_type == "user" and "department_id" in result.get("updated_fields", []):
                # User changed departments, invalidate department user lists
                old_dept = result["payload_before"].get("department_id")
                new_dept = result["object_data"].get("department_id")

                if old_dept:
                    await self.cache.invalidate(f"department:{old_dept}:users")
                if new_dept:
                    await self.cache.invalidate(f"department:{new_dept}:users")
```

## Summary

The Mutation Result Pattern establishes a standardized foundation for all mutations in FraiseQL applications. By using the `app.mutation_result` type and `core.log_and_return_mutation` function, you gain:

**✅ Benefits Achieved:**

- **Consistent API** - All mutations return the same structured response
- **Network Performance** - Rich object returns eliminate 70-80% of follow-up API calls, dramatically reducing latency
- **Resource Efficiency** - Single comprehensive response reduces backend load and frontend complexity
- **Complete Audit Trail** - Every mutation is logged with full context
- **Field-Level Change Tracking** - Know exactly what changed
- **Graceful Error Handling** - NOOP patterns eliminate exceptions
- **Enterprise Compliance** - Built-in audit logging and metadata
- **CDC/Event Streaming** - Debezium-compatible structure for real-time data integration
- **Debugging Support** - Rich metadata for troubleshooting
- **Performance Optimization** - Efficient change detection and caching hooks

**🚀 Next Steps:**

1. Implement the `app.mutation_result` type in your database
2. Create the `core.log_and_return_mutation` helper function
3. Migrate one mutation at a time using the patterns shown
4. Update your GraphQL resolvers to handle the new result structure
5. Add comprehensive audit logging to your application

This pattern scales from simple CRUD operations to complex business workflows while maintaining consistency and providing the metadata needed for enterprise applications.

## See Also

- [PostgreSQL Function-Based Mutations](./postgresql-function-based.md) - Core mutation patterns
- [Migration Guide](./migration-guide.md) - Converting existing mutations
- [Multi-Tenancy](../advanced/multi-tenancy.md) - Tenant context patterns
- [CQRS](../advanced/cqrs.md) - Command-query separation principles
- [Audit System](../advanced/audit-system.md) - Enterprise audit logging
- [Cache Invalidation](../advanced/mutation-cache-patterns.md) - Cache management with mutations
