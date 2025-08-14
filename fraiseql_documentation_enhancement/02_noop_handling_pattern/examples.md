# NOOP Handling Pattern - Examples

## Overview

The NOOP (No Operation) pattern in FraiseQL mutations represents scenarios where a mutation request cannot be fulfilled due to validation failures, missing data, authorization issues, or business rule violations. Instead of throwing errors, the system returns a structured NOOP response that maintains the mutation result pattern consistency.

## Basic NOOP Structure

Every NOOP response follows this pattern through `core.log_and_return_mutation`:

```sql
RETURN core.log_and_return_mutation(
    input_pk_organization,     -- Organization context
    input_fk_user,            -- User who initiated the action
    'entity_type',            -- Entity being operated on
    entity_id,                -- Entity ID (NULL for create operations)
    'NOOP',                   -- Operation type (always 'NOOP')
    'noop:specific_reason',   -- Specific error code
    v_fields,                 -- Fields involved in the validation
    'Human readable message', -- User-friendly error message
    v_payload_before,         -- Entity state before operation
    NULL,                     -- No payload_after for NOOP
    jsonb_build_object(       -- Metadata for debugging/logging
        'trigger', 'api_create',
        'reason', 'validation_failed'
    )
);
```

## Core NOOP Categories

### 1. Input Validation NOOPs

**Empty/Null Required Fields:**
```sql
-- Title validation in post creation
IF input_data.title IS NULL OR length(trim(input_data.title)) = 0 THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'post',
        NULL,
        'NOOP',
        'noop:invalid_title',
        ARRAY['title'],
        'Title is required',
        NULL,
        NULL,
        jsonb_build_object(
            'trigger', 'api_create',
            'reason', 'empty_title'
        )
    );
END IF;
```

**Content Length Validation:**
```sql
-- Content validation in comment creation
IF input_data.content IS NULL OR length(trim(input_data.content)) = 0 THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'comment',
        NULL,
        'NOOP',
        'noop:invalid_content',
        ARRAY['content'],
        'Content is required',
        NULL,
        NULL,
        jsonb_build_object(
            'trigger', 'api_create',
            'reason', 'empty_content'
        )
    );
END IF;
```

**Format Validation:**
```sql
-- Email format validation
IF input_data.email IS NOT NULL
   AND input_data.email !~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$' THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'user',
        input_pk_user,
        'NOOP',
        'noop:invalid_email_format',
        ARRAY['email'],
        'Invalid email format',
        v_payload_before,
        NULL,
        jsonb_build_object(
            'trigger', 'api_update',
            'reason', 'email_format_invalid'
        )
    );
END IF;
```

### 2. Entity Not Found NOOPs

**Record Missing for Update:**
```sql
-- Post not found during update
SELECT id, slug, title, content INTO v_id, v_old_slug, v_old_title, v_payload_before
FROM tb_post
WHERE pk_post = input_pk_post;

IF NOT FOUND THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'post',
        input_pk_post,
        'NOOP',
        'noop:not_found',
        ARRAY['all'],
        'Post not found',
        NULL,
        NULL,
        jsonb_build_object(
            'trigger', 'api_update',
            'reason', 'post_not_found'
        )
    );
END IF;
```

**Foreign Key Reference Missing:**
```sql
-- User not found during post creation
IF NOT EXISTS (SELECT 1 FROM tb_user WHERE pk_user = input_fk_user) THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'post',
        NULL,
        'NOOP',
        'noop:user_not_found',
        ARRAY['author'],
        'User not found',
        NULL,
        NULL,
        jsonb_build_object(
            'trigger', 'api_create',
            'reason', 'author_not_found'
        )
    );
END IF;
```

### 3. Business Rule Violation NOOPs

**Uniqueness Constraints:**
```sql
-- Duplicate slug prevention
v_slug := lower(regexp_replace(trim(input_data.title), '[^a-zA-Z0-9]+', '-', 'g'));

IF EXISTS (SELECT 1 FROM tb_post WHERE slug = v_slug) THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'post',
        NULL,
        'NOOP',
        'noop:duplicate_slug',
        ARRAY['title', 'slug'],
        'A post with this title already exists',
        NULL,
        NULL,
        jsonb_build_object(
            'trigger', 'api_create',
            'reason', 'slug_conflict',
            'generated_slug', v_slug
        )
    );
END IF;
```

**State Transition Rules:**
```sql
-- Cannot publish post without required fields
IF input_data.is_published = true
   AND (input_data.excerpt IS NULL OR length(trim(input_data.excerpt)) = 0) THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'post',
        input_pk_post,
        'NOOP',
        'noop:cannot_publish_incomplete',
        ARRAY['is_published', 'excerpt'],
        'Cannot publish post without excerpt',
        v_payload_before,
        NULL,
        jsonb_build_object(
            'trigger', 'api_update',
            'reason', 'incomplete_publication_data'
        )
    );
END IF;
```

### 4. Authorization NOOPs

**Ownership Verification:**
```sql
-- Comment ownership check for updates
IF (v_payload_before->>'fk_user')::UUID != input_fk_user THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'comment',
        input_pk_comment,
        'NOOP',
        'noop:not_owner',
        ARRAY['all'],
        'Only the comment author can edit it',
        v_payload_before,
        NULL,
        jsonb_build_object(
            'trigger', 'api_update',
            'reason', 'not_comment_owner',
            'actual_owner', v_payload_before->>'fk_user'
        )
    );
END IF;
```

**Permission Level Checks:**
```sql
-- Admin-only operation
IF NOT has_role(input_fk_user, 'admin') THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'user',
        input_pk_user,
        'NOOP',
        'noop:insufficient_permissions',
        ARRAY['roles'],
        'Admin privileges required for this operation',
        v_payload_before,
        NULL,
        jsonb_build_object(
            'trigger', 'api_update',
            'reason', 'admin_required',
            'user_roles', get_user_roles(input_fk_user)
        )
    );
END IF;
```

### 5. Dependency Violation NOOPs

**Cannot Delete with Dependencies:**
```sql
-- Post deletion with existing comments
IF EXISTS (SELECT 1 FROM tb_comment WHERE fk_post = input_pk_post) THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'post',
        input_pk_post,
        'NOOP',
        'noop:has_comments',
        ARRAY['all'],
        'Cannot delete post with existing comments',
        v_payload_before,
        NULL,
        jsonb_build_object(
            'trigger', 'api_delete',
            'reason', 'has_child_comments',
            'comment_count', (SELECT count(*) FROM tb_comment WHERE fk_post = input_pk_post)
        )
    );
END IF;
```

**Parent-Child Relationship Validation:**
```sql
-- Parent comment validation
IF input_data.fk_parent_comment IS NOT NULL THEN
    IF NOT EXISTS (
        SELECT 1
        FROM tb_comment
        WHERE pk_comment = input_data.fk_parent_comment
          AND fk_post = input_fk_post
    ) THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_fk_user,
            'comment',
            NULL,
            'NOOP',
            'noop:parent_not_found',
            ARRAY['fk_parent_comment'],
            'Parent comment not found or belongs to different post',
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_create',
                'reason', 'invalid_parent_comment',
                'parent_id', input_data.fk_parent_comment,
                'post_id', input_fk_post
            )
        );
    END IF;
END IF;
```

### 6. State Consistency NOOPs

**Immutable Field Changes:**
```sql
-- Cannot change parent comment after creation
IF input_data.fk_parent_comment IS NOT NULL
   AND (v_payload_before->>'fk_parent_comment')::UUID IS DISTINCT FROM input_data.fk_parent_comment THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'comment',
        input_pk_comment,
        'NOOP',
        'noop:cannot_change_parent',
        ARRAY['fk_parent_comment'],
        'Cannot change parent comment',
        v_payload_before,
        NULL,
        jsonb_build_object(
            'trigger', 'api_update',
            'reason', 'immutable_parent_change',
            'current_parent', v_payload_before->>'fk_parent_comment',
            'requested_parent', input_data.fk_parent_comment
        )
    );
END IF;
```

**Logical State Conflicts:**
```sql
-- Cannot unpublish already published content
IF input_data.is_published = false
   AND (v_payload_before->>'is_published')::boolean = true
   AND (v_payload_before->>'published_at') IS NOT NULL THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_fk_user,
        'post',
        input_pk_post,
        'NOOP',
        'noop:cannot_unpublish',
        ARRAY['is_published'],
        'Cannot unpublish already published content',
        v_payload_before,
        NULL,
        jsonb_build_object(
            'trigger', 'api_update',
            'reason', 'published_content_immutable',
            'published_at', v_payload_before->>'published_at'
        )
    );
END IF;
```

## Complete Working Examples

### 1. Create Operation with Idempotent Duplicate Handling

**SQL Function:**
```sql
CREATE OR REPLACE FUNCTION app.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_user_input;
    v_existing_user RECORD;
    v_user_id UUID;
    v_payload_after JSONB;
BEGIN
    -- Parse input
    v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);

    -- Check for existing user by email (primary business key)
    SELECT pk_user, data INTO v_existing_user
    FROM tenant.tb_user
    WHERE pk_organization = input_pk_organization
    AND data->>'email' = v_input.email;

    -- NOOP: User already exists with this email
    IF v_existing_user.pk_user IS NOT NULL THEN
        -- Get complete user data from view
        SELECT data INTO v_payload_after
        FROM public.tv_user
        WHERE id = v_existing_user.pk_user;

        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'user',
            v_existing_user.pk_user,
            'NOOP',
            'noop:already_exists',
            ARRAY[]::TEXT[],  -- No fields changed
            format('User with email %s already exists', v_input.email),
            v_payload_after,  -- Current state
            v_payload_after,  -- Unchanged state
            jsonb_build_object(
                'trigger', 'api_create',
                'reason', 'duplicate_email',
                'existing_email', v_input.email,
                'idempotent_match', true,
                'requested_data', input_payload
            )
        );
    END IF;

    -- Proceed with creation...
    INSERT INTO tenant.tb_user (pk_organization, data, created_by)
    VALUES (
        input_pk_organization,
        jsonb_build_object(
            'email', v_input.email,
            'name', v_input.name,
            'role', COALESCE(v_input.role, 'user')
        ),
        input_created_by
    ) RETURNING pk_user INTO v_user_id;

    -- Get complete user data
    SELECT data INTO v_payload_after
    FROM public.tv_user
    WHERE id = v_user_id;

    -- Return success
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_created_by,
        'user',
        v_user_id,
        'INSERT',
        'new',
        ARRAY['email', 'name', 'role'],
        'User created successfully',
        NULL,
        v_payload_after,
        jsonb_build_object(
            'trigger', 'api_create',
            'created_fields', ARRAY['email', 'name', 'role']
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

### 2. Update Operation with No-Changes NOOP

**SQL Function:**
```sql
CREATE OR REPLACE FUNCTION app.update_post(
    input_pk_organization UUID,
    input_updated_by UUID,
    input_pk_post UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_post_input;
    v_current_data JSONB;
    v_changed_fields TEXT[] := ARRAY[]::TEXT[];
    v_update_data JSONB := '{}'::JSONB;
    v_payload_after JSONB;
BEGIN
    -- Get current post data
    SELECT data INTO v_current_data
    FROM public.tv_post
    WHERE id = input_pk_post AND tenant_id = input_pk_organization;

    -- NOOP: Post not found
    IF v_current_data IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_updated_by,
            'post',
            input_pk_post,
            'NOOP',
            'noop:not_found',
            ARRAY[]::TEXT[],
            'Post not found',
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_update',
                'requested_id', input_pk_post,
                'idempotent_safe', true
            )
        );
    END IF;

    -- Parse input
    v_input := jsonb_populate_record(NULL::app.type_post_input, input_payload);

    -- Check each field for actual changes
    IF v_input.title IS NOT NULL THEN
        IF v_input.title != COALESCE(v_current_data->>'title', '') THEN
            v_update_data := v_update_data || jsonb_build_object('title', v_input.title);
            v_changed_fields := array_append(v_changed_fields, 'title');
        END IF;
    END IF;

    IF v_input.content IS NOT NULL THEN
        IF v_input.content != COALESCE(v_current_data->>'content', '') THEN
            v_update_data := v_update_data || jsonb_build_object('content', v_input.content);
            v_changed_fields := array_append(v_changed_fields, 'content');
        END IF;
    END IF;

    IF v_input.tags IS NOT NULL THEN
        IF v_input.tags::jsonb != COALESCE(v_current_data->'tags', '[]'::jsonb) THEN
            v_update_data := v_update_data || jsonb_build_object('tags', v_input.tags);
            v_changed_fields := array_append(v_changed_fields, 'tags');
        END IF;
    END IF;

    -- NOOP: No actual changes detected
    IF array_length(v_changed_fields, 1) IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_updated_by,
            'post',
            input_pk_post,
            'NOOP',
            'noop:no_changes',
            ARRAY[]::TEXT[],
            'No changes detected - values are identical',
            v_current_data,
            v_current_data,  -- Same before and after
            jsonb_build_object(
                'trigger', 'api_update',
                'requested_changes', input_payload,
                'current_values', v_current_data,
                'identical_values', true,
                'idempotent_safe', true
            )
        );
    END IF;

    -- Proceed with update...
    UPDATE tenant.tb_post
    SET data = data || v_update_data
    WHERE pk_post = input_pk_post;

    -- Get updated data
    SELECT data INTO v_payload_after
    FROM public.tv_post
    WHERE id = input_pk_post;

    -- Return success
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_updated_by,
        'post',
        input_pk_post,
        'UPDATE',
        'updated',
        v_changed_fields,
        'Post updated successfully',
        v_current_data,
        v_payload_after,
        jsonb_build_object(
            'trigger', 'api_update',
            'changed_fields', v_changed_fields,
            'update_data', v_update_data
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

### 3. Delete Operation with Multiple NOOP Scenarios

**SQL Function:**
```sql
CREATE OR REPLACE FUNCTION app.delete_post(
    input_pk_organization UUID,
    input_deleted_by UUID,
    input_pk_post UUID
) RETURNS app.mutation_result AS $$
DECLARE
    v_post_data JSONB;
    v_comment_count INTEGER;
    v_is_published BOOLEAN;
BEGIN
    -- Get current post state
    SELECT data INTO v_post_data
    FROM public.tv_post
    WHERE id = input_pk_post AND tenant_id = input_pk_organization;

    -- NOOP: Post not found (idempotent - desired state achieved)
    IF v_post_data IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_deleted_by,
            'post',
            input_pk_post,
            'NOOP',
            'noop:not_found',
            ARRAY[]::TEXT[],
            'Post not found - may already be deleted',
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_delete',
                'requested_id', input_pk_post,
                'idempotent_safe', true,
                'reason', 'not_found_or_deleted'
            )
        );
    END IF;

    -- NOOP: Already marked as deleted (soft delete scenario)
    IF (v_post_data->>'deleted_at') IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_deleted_by,
            'post',
            input_pk_post,
            'NOOP',
            'noop:already_deleted',
            ARRAY[]::TEXT[],
            'Post is already deleted',
            v_post_data,
            v_post_data,
            jsonb_build_object(
                'trigger', 'api_delete',
                'deleted_at', v_post_data->>'deleted_at',
                'idempotent_safe', true,
                'reason', 'already_soft_deleted'
            )
        );
    END IF;

    -- Check business rules
    v_is_published := (v_post_data->>'is_published')::BOOLEAN;

    -- NOOP: Cannot delete published post (business rule)
    IF v_is_published THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_deleted_by,
            'post',
            input_pk_post,
            'NOOP',
            'noop:cannot_delete_published',
            ARRAY[]::TEXT[],
            'Cannot delete published post - unpublish first',
            v_post_data,
            v_post_data,
            jsonb_build_object(
                'trigger', 'api_delete',
                'business_rule', 'no_delete_published',
                'published_at', v_post_data->>'published_at',
                'suggested_action', 'unpublish_first'
            )
        );
    END IF;

    -- Check for dependent entities
    SELECT COUNT(*) INTO v_comment_count
    FROM tenant.tb_comment
    WHERE data->>'post_id' = input_pk_post::TEXT;

    -- NOOP: Has dependent comments (referential integrity)
    IF v_comment_count > 0 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_deleted_by,
            'post',
            input_pk_post,
            'NOOP',
            'noop:cannot_delete_referenced',
            ARRAY[]::TEXT[],
            format('Cannot delete post with %s comments', v_comment_count),
            v_post_data,
            v_post_data,
            jsonb_build_object(
                'trigger', 'api_delete',
                'referential_constraint', 'has_comments',
                'comment_count', v_comment_count,
                'suggested_action', 'delete_comments_first'
            )
        );
    END IF;

    -- Proceed with soft delete
    UPDATE tenant.tb_post
    SET data = data || jsonb_build_object(
        'deleted_at', NOW(),
        'deleted_by', input_deleted_by
    )
    WHERE pk_post = input_pk_post;

    -- Return success (note: we could return the deleted state)
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_deleted_by,
        'post',
        input_pk_post,
        'DELETE',
        'deleted',
        ARRAY['deleted_at', 'deleted_by'],
        'Post deleted successfully',
        v_post_data,
        NULL,  -- No "after" state for deletions
        jsonb_build_object(
            'trigger', 'api_delete',
            'soft_delete', true
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

### 4. Multi-Field Deduplication with Priority

**SQL Function:**
```sql
CREATE OR REPLACE FUNCTION app.create_product(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_product_input;
    v_existing_product RECORD;
    v_match_type TEXT;
    v_match_value TEXT;
    v_payload_after JSONB;
    v_product_id UUID;
BEGIN
    -- Parse input
    v_input := jsonb_populate_record(NULL::app.type_product_input, input_payload);

    -- Priority 1: Check by SKU (highest priority business key)
    IF v_input.sku IS NOT NULL THEN
        SELECT pk_product, data, 'sku', v_input.sku
        INTO v_existing_product.pk_product, v_payload_after, v_match_type, v_match_value
        FROM tenant.tb_product
        WHERE pk_organization = input_pk_organization
        AND data->>'sku' = v_input.sku;
    END IF;

    -- Priority 2: Check by external_id (if no SKU match)
    IF v_existing_product.pk_product IS NULL AND v_input.external_id IS NOT NULL THEN
        SELECT pk_product, data, 'external_id', v_input.external_id
        INTO v_existing_product.pk_product, v_payload_after, v_match_type, v_match_value
        FROM tenant.tb_product
        WHERE pk_organization = input_pk_organization
        AND data->>'external_id' = v_input.external_id;
    END IF;

    -- Priority 3: Check by name + category (weak match)
    IF v_existing_product.pk_product IS NULL AND v_input.name IS NOT NULL THEN
        SELECT pk_product, data, 'name_category', v_input.name
        INTO v_existing_product.pk_product, v_payload_after, v_match_type, v_match_value
        FROM tenant.tb_product
        WHERE pk_organization = input_pk_organization
        AND data->>'name' = v_input.name
        AND data->>'category' = v_input.category;
    END IF;

    -- NOOP: Duplicate found
    IF v_existing_product.pk_product IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'product',
            v_existing_product.pk_product,
            'NOOP',
            'noop:already_exists',
            ARRAY[]::TEXT[],
            format('Product already exists - matched by %s: %s', v_match_type, v_match_value),
            v_payload_after,
            v_payload_after,
            jsonb_build_object(
                'trigger', 'api_create',
                'match_strategy', v_match_type,
                'matched_value', v_match_value,
                'match_priority', CASE v_match_type
                    WHEN 'sku' THEN 1
                    WHEN 'external_id' THEN 2
                    WHEN 'name_category' THEN 3
                END,
                'idempotent_match', true,
                'existing_data', v_payload_after
            )
        );
    END IF;

    -- Proceed with creation...
    INSERT INTO tenant.tb_product (pk_organization, data, created_by)
    VALUES (
        input_pk_organization,
        jsonb_build_object(
            'sku', v_input.sku,
            'external_id', v_input.external_id,
            'name', v_input.name,
            'category', v_input.category,
            'price', v_input.price
        ),
        input_created_by
    ) RETURNING pk_product INTO v_product_id;

    -- Get complete product data
    SELECT data INTO v_payload_after
    FROM public.tv_product
    WHERE id = v_product_id;

    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_created_by,
        'product',
        v_product_id,
        'INSERT',
        'new',
        ARRAY['sku', 'external_id', 'name', 'category', 'price'],
        'Product created successfully',
        NULL,
        v_payload_after,
        jsonb_build_object(
            'trigger', 'api_create',
            'created_with_keys', jsonb_build_object(
                'sku', v_input.sku,
                'external_id', v_input.external_id,
                'name_category', v_input.name || '/' || v_input.category
            )
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

## GraphQL Integration Examples

### 1. Success vs NOOP Response Types

**Python Types:**
```python
@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"
    was_noop: bool = False

@fraiseql.success
class CreateUserNoop:
    """NOOP - User already exists"""
    existing_user: User
    message: str
    noop_reason: str
    match_details: dict[str, Any] | None = None
    was_noop: bool = True

@fraiseql.failure
class CreateUserError:
    message: str
    error_code: str

# Union type for all possible outcomes
CreateUserResult = CreateUserSuccess | CreateUserNoop | CreateUserError

@fraiseql.mutation
async def create_user(
    info: GraphQLResolveInfo,
    input: CreateUserInput
) -> CreateUserResult:
    """Create user with comprehensive NOOP handling."""

    result = await db.call_function("app.create_user", ...)
    status = result.get("status", "")

    if status == "new":
        return CreateUserSuccess(
            user=User.from_dict(result["object_data"]),
            message=result["message"]
        )
    elif status.startswith("noop:"):
        return CreateUserNoop(
            existing_user=User.from_dict(result["object_data"]),
            message=result["message"],
            noop_reason=status.replace("noop:", ""),
            match_details=result.get("extra_metadata", {})
        )
    else:
        return CreateUserError(
            message=result.get("message", "Operation failed"),
            error_code="UNKNOWN_STATUS"
        )
```

### 2. Client-Side NOOP Handling

**React/TypeScript Example:**
```typescript
interface CreateUserMutationVariables {
  input: CreateUserInput;
}

interface CreateUserMutationData {
  createUser: CreateUserResult;
}

const CREATE_USER_MUTATION = gql`
  mutation CreateUser($input: CreateUserInput!) {
    createUser(input: $input) {
      __typename
      ... on CreateUserSuccess {
        user {
          id
          email
          name
        }
        message
      }
      ... on CreateUserNoop {
        existingUser {
          id
          email
          name
        }
        message
        noopReason
        matchDetails
      }
      ... on CreateUserError {
        message
        errorCode
      }
    }
  }
`;

const CreateUserComponent: React.FC = () => {
  const [createUser] = useMutation<CreateUserMutationData, CreateUserMutationVariables>(
    CREATE_USER_MUTATION
  );

  const handleSubmit = async (userData: CreateUserInput) => {
    try {
      const result = await createUser({ variables: { input: userData } });
      const response = result.data?.createUser;

      switch (response?.__typename) {
        case 'CreateUserSuccess':
          // New user created
          showToast({
            type: 'success',
            title: 'User Created',
            message: response.message
          });
          // Navigate to user profile
          router.push(`/users/${response.user.id}`);
          break;

        case 'CreateUserNoop':
          // User already exists - show existing user
          showToast({
            type: 'info',
            title: 'User Already Exists',
            message: `${response.message} (matched by ${response.noopReason})`
          });

          // Ask user if they want to view existing user
          const viewExisting = await confirmDialog({
            title: 'User Exists',
            message: 'A user with this email already exists. Would you like to view their profile?',
            confirmText: 'View Profile',
            cancelText: 'Stay Here'
          });

          if (viewExisting) {
            router.push(`/users/${response.existingUser.id}`);
          }
          break;

        case 'CreateUserError':
          showToast({
            type: 'error',
            title: 'Error Creating User',
            message: response.message
          });
          break;
      }
    } catch (error) {
      showToast({
        type: 'error',
        title: 'Network Error',
        message: 'Failed to create user. Please try again.'
      });
    }
  };

  // Component JSX...
};
```

### 3. Batch Operations with NOOP Handling

**SQL Function for Batch Import:**
```sql
CREATE OR REPLACE FUNCTION app.bulk_import_products(
    input_pk_organization UUID,
    input_created_by UUID,
    input_products JSONB
) RETURNS JSONB AS $$
DECLARE
    v_product RECORD;
    v_results JSONB[] := ARRAY[]::JSONB[];
    v_result app.mutation_result;
    v_stats RECORD := ROW(0, 0, 0, 0)::RECORD; -- created, updated, nooped, errored
BEGIN
    -- Process each product
    FOR v_product IN
        SELECT * FROM jsonb_array_elements(input_products) AS product
    LOOP
        -- Call individual create function
        SELECT * INTO v_result
        FROM app.create_product(
            input_pk_organization,
            input_created_by,
            v_product.value
        );

        -- Track statistics
        CASE
            WHEN v_result.status = 'new' THEN
                v_stats.f1 := v_stats.f1 + 1; -- created
            WHEN v_result.status = 'updated' THEN
                v_stats.f2 := v_stats.f2 + 1; -- updated
            WHEN v_result.status LIKE 'noop:%' THEN
                v_stats.f3 := v_stats.f3 + 1; -- nooped
            ELSE
                v_stats.f4 := v_stats.f4 + 1; -- errored
        END CASE;

        -- Add to results
        v_results := array_append(v_results, row_to_json(v_result)::JSONB);
    END LOOP;

    -- Return batch summary
    RETURN jsonb_build_object(
        'success', v_stats.f4 = 0, -- No errors
        'results', array_to_json(v_results),
        'summary', jsonb_build_object(
            'total', jsonb_array_length(input_products),
            'created', v_stats.f1,
            'updated', v_stats.f2,
            'duplicates_skipped', v_stats.f3,
            'errors', v_stats.f4
        ),
        'metadata', jsonb_build_object(
            'trigger', 'api_bulk_import',
            'processed_at', NOW(),
            'idempotent_operation', true
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

## Testing NOOP Scenarios

**Python Test Examples:**
```python
import pytest
from fraiseql.testing import FraiseQLTestClient

@pytest.mark.asyncio
async def test_create_user_idempotent(test_client: FraiseQLTestClient):
    """Test that creating duplicate user returns NOOP."""

    user_input = {
        "email": "test@example.com",
        "name": "Test User"
    }

    # First creation - should succeed
    result1 = await test_client.execute(
        """
        mutation CreateUser($input: CreateUserInput!) {
            createUser(input: $input) {
                __typename
                ... on CreateUserSuccess {
                    user { id email }
                    message
                }
                ... on CreateUserNoop {
                    existingUser { id email }
                    message
                    noopReason
                }
            }
        }
        """,
        variables={"input": user_input}
    )

    assert result1["data"]["createUser"]["__typename"] == "CreateUserSuccess"
    user_id = result1["data"]["createUser"]["user"]["id"]

    # Second creation attempt - should return NOOP
    result2 = await test_client.execute(
        """
        mutation CreateUser($input: CreateUserInput!) {
            createUser(input: $input) {
                __typename
                ... on CreateUserNoop {
                    existingUser { id email }
                    message
                    noopReason
                    matchDetails
                }
            }
        }
        """,
        variables={"input": user_input}
    )

    assert result2["data"]["createUser"]["__typename"] == "CreateUserNoop"
    assert result2["data"]["createUser"]["existingUser"]["id"] == user_id
    assert result2["data"]["createUser"]["noopReason"] == "already_exists"
    assert "duplicate_email" in result2["data"]["createUser"]["message"].lower()

@pytest.mark.asyncio
async def test_update_no_changes_noop(test_client: FraiseQLTestClient):
    """Test that updating with identical values returns NOOP."""

    # Create a user first
    user = await create_test_user(test_client)

    # Update with identical values
    result = await test_client.execute(
        """
        mutation UpdateUser($id: UUID!, $input: UpdateUserInput!) {
            updateUser(id: $id, input: $input) {
                __typename
                ... on UpdateUserNoop {
                    message
                    noopReason
                    currentUser { id name }
                }
            }
        }
        """,
        variables={
            "id": user["id"],
            "input": {
                "name": user["name"],  # Same as current
                "bio": user["bio"]     # Same as current
            }
        }
    )

    assert result["data"]["updateUser"]["__typename"] == "UpdateUserNoop"
    assert result["data"]["updateUser"]["noopReason"] == "no_changes"
```
