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
