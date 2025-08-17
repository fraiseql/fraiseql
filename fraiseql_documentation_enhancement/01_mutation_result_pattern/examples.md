# Mutation Result Pattern - Code Examples

## Complete Working Examples

### 1. Basic Create Mutation

**SQL Function:**
```sql
-- User creation with mutation result
CREATE OR REPLACE FUNCTION app.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_user_input;
    v_user_id UUID;
    v_payload_after JSONB;
BEGIN
    -- Parse input
    v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);

    -- Validation
    IF v_input.email IS NULL OR v_input.name IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'user',
            NULL,
            'NOOP',
            'noop:invalid_input',
            ARRAY[]::TEXT[],
            'Name and email are required',
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_create',
                'validation_errors', ARRAY['name', 'email']
            )
        );
    END IF;

    -- Check for duplicate
    IF EXISTS (SELECT 1 FROM tenant.tb_user WHERE data->>'email' = v_input.email) THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'user',
            (SELECT pk_user FROM tenant.tb_user WHERE data->>'email' = v_input.email),
            'NOOP',
            'noop:already_exists',
            ARRAY[]::TEXT[],
            'User with this email already exists',
            (SELECT data FROM public.tv_user WHERE email = v_input.email),
            (SELECT data FROM public.tv_user WHERE email = v_input.email),
            jsonb_build_object(
                'trigger', 'api_create',
                'reason', 'duplicate_email',
                'existing_email', v_input.email
            )
        );
    END IF;

    -- Create user
    INSERT INTO tenant.tb_user (pk_organization, data, created_by)
    VALUES (
        input_pk_organization,
        jsonb_build_object(
            'name', v_input.name,
            'email', v_input.email,
            'bio', v_input.bio
        ),
        input_created_by
    ) RETURNING pk_user INTO v_user_id;

    -- Get complete user data from view
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
        ARRAY['name', 'email', 'bio'],
        'User created successfully',
        NULL,
        v_payload_after,
        jsonb_build_object(
            'trigger', 'api_create',
            'created_fields', ARRAY['name', 'email', 'bio']
        )
    );

EXCEPTION
    WHEN OTHERS THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'user',
            NULL,
            'ERROR',
            'noop:internal_error',
            ARRAY[]::TEXT[],
            'Failed to create user: ' || SQLERRM,
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_create',
                'error_detail', SQLERRM,
                'sqlstate', SQLSTATE
            )
        );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

**Python Resolver:**
```python
@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str
    metadata: dict[str, Any] | None = None

@fraiseql.failure
class CreateUserError:
    message: str
    error_code: str
    existing_user: User | None = None
    metadata: dict[str, Any] | None = None

@fraiseql.mutation
async def create_user(
    info: GraphQLResolveInfo,
    input: CreateUserInput
) -> CreateUserSuccess | CreateUserError:
    """Create a new user with mutation result pattern."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]
    user_id = info.context["user_id"]

    # Call PostgreSQL function
    result = await db.call_function(
        "app.create_user",
        input_pk_organization=tenant_id,
        input_created_by=user_id,
        input_payload=input.to_dict()
    )

    # Parse mutation result
    status = result.get("status", "")

    if status.startswith("noop:"):
        error_code = status.replace("noop:", "").upper()
        existing_user = None

        if result.get("object_data"):
            existing_user = User.from_dict(result["object_data"])

        return CreateUserError(
            message=result.get("message", "Operation failed"),
            error_code=error_code,
            existing_user=existing_user,
            metadata=result.get("extra_metadata")
        )

    elif status == "new":
        return CreateUserSuccess(
            user=User.from_dict(result["object_data"]),
            message=result.get("message", "User created successfully"),
            metadata=result.get("extra_metadata")
        )

    else:
        return CreateUserError(
            message="Unknown response status",
            error_code="UNKNOWN_STATUS",
            metadata=result.get("extra_metadata")
        )
```

### 2. Update Mutation with Change Tracking

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
    v_payload_before JSONB;
    v_payload_after JSONB;
    v_changed_fields TEXT[] := ARRAY[]::TEXT[];
    v_update_data JSONB := '{}'::JSONB;
BEGIN
    -- Get current state
    SELECT data INTO v_payload_before
    FROM public.tv_post
    WHERE id = input_pk_post AND tenant_id = input_pk_organization;

    IF v_payload_before IS NULL THEN
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
                'requested_id', input_pk_post
            )
        );
    END IF;

    -- Parse input
    v_input := jsonb_populate_record(NULL::app.type_post_input, input_payload);

    -- Build update data and track changes
    IF v_input.title IS NOT NULL AND v_input.title != v_payload_before->>'title' THEN
        v_update_data := v_update_data || jsonb_build_object('title', v_input.title);
        v_changed_fields := array_append(v_changed_fields, 'title');
    END IF;

    IF v_input.content IS NOT NULL AND v_input.content != v_payload_before->>'content' THEN
        v_update_data := v_update_data || jsonb_build_object('content', v_input.content);
        v_changed_fields := array_append(v_changed_fields, 'content');
    END IF;

    IF v_input.tags IS NOT NULL AND v_input.tags::jsonb != v_payload_before->'tags' THEN
        v_update_data := v_update_data || jsonb_build_object('tags', v_input.tags);
        v_changed_fields := array_append(v_changed_fields, 'tags');
    END IF;

    -- Check if any changes
    IF array_length(v_changed_fields, 1) IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_updated_by,
            'post',
            input_pk_post,
            'NOOP',
            'noop:no_changes',
            ARRAY[]::TEXT[],
            'No changes detected',
            v_payload_before,
            v_payload_before,
            jsonb_build_object(
                'trigger', 'api_update',
                'requested_changes', input_payload
            )
        );
    END IF;

    -- Perform update
    UPDATE tenant.tb_post
    SET
        data = data || v_update_data,
        updated_by = input_updated_by,
        updated_at = NOW()
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
        v_payload_before,
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

### 3. Complex Business Logic with Multiple Operations

**SQL Function:**
```sql
CREATE OR REPLACE FUNCTION app.publish_post(
    input_pk_organization UUID,
    input_published_by UUID,
    input_pk_post UUID
) RETURNS app.mutation_result AS $$
DECLARE
    v_post_data JSONB;
    v_author_id UUID;
    v_payload_before JSONB;
    v_payload_after JSONB;
BEGIN
    -- Get current post state
    SELECT data, (data->>'author_id')::UUID
    INTO v_payload_before, v_author_id
    FROM public.tv_post
    WHERE id = input_pk_post AND tenant_id = input_pk_organization;

    -- Check if post exists
    IF v_payload_before IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_published_by,
            'post',
            input_pk_post,
            'NOOP',
            'noop:not_found',
            ARRAY[]::TEXT[],
            'Post not found',
            NULL,
            NULL,
            jsonb_build_object('trigger', 'api_publish')
        );
    END IF;

    -- Check if already published
    IF (v_payload_before->>'is_published')::BOOLEAN = true THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_published_by,
            'post',
            input_pk_post,
            'NOOP',
            'noop:already_published',
            ARRAY[]::TEXT[],
            'Post is already published',
            v_payload_before,
            v_payload_before,
            jsonb_build_object(
                'trigger', 'api_publish',
                'published_at', v_payload_before->>'published_at'
            )
        );
    END IF;

    -- Update post status
    UPDATE tenant.tb_post
    SET data = data || jsonb_build_object(
        'is_published', true,
        'published_at', NOW()
    )
    WHERE pk_post = input_pk_post;

    -- Update author stats
    PERFORM app.increment_user_published_count(v_author_id);

    -- Create notification
    INSERT INTO tenant.tb_notification (
        pk_organization,
        fk_user,
        notification_type,
        data,
        created_by
    ) VALUES (
        input_pk_organization,
        v_author_id,
        'post_published',
        jsonb_build_object(
            'post_id', input_pk_post,
            'post_title', v_payload_before->>'title'
        ),
        input_published_by
    );

    -- Get updated post data
    SELECT data INTO v_payload_after
    FROM public.tv_post
    WHERE id = input_pk_post;

    -- Return success
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_published_by,
        'post',
        input_pk_post,
        'UPDATE',
        'published',
        ARRAY['is_published', 'published_at'],
        'Post published successfully',
        v_payload_before,
        v_payload_after,
        jsonb_build_object(
            'trigger', 'api_publish',
            'author_id', v_author_id,
            'notification_sent', true,
            'stats_updated', true
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

### 4. Batch Operation Example

**SQL Function:**
```sql
CREATE OR REPLACE FUNCTION app.bulk_update_post_status(
    input_pk_organization UUID,
    input_updated_by UUID,
    input_operations JSONB
) RETURNS JSONB AS $$
DECLARE
    v_operation RECORD;
    v_results JSONB[] := ARRAY[]::JSONB[];
    v_result app.mutation_result;
    v_success_count INTEGER := 0;
    v_error_count INTEGER := 0;
BEGIN
    -- Process each operation
    FOR v_operation IN
        SELECT * FROM jsonb_array_elements(input_operations) AS op
    LOOP
        -- Call individual update function
        SELECT * INTO v_result
        FROM app.update_post_status(
            input_pk_organization,
            input_updated_by,
            (v_operation.value->>'post_id')::UUID,
            v_operation.value->'status'
        );

        -- Add to results
        v_results := array_append(v_results, row_to_json(v_result)::JSONB);

        -- Track counts
        IF v_result.status NOT LIKE 'noop:%' THEN
            v_success_count := v_success_count + 1;
        ELSE
            v_error_count := v_error_count + 1;
        END IF;
    END LOOP;

    -- Return batch summary
    RETURN jsonb_build_object(
        'success', v_error_count = 0,
        'results', array_to_json(v_results),
        'summary', jsonb_build_object(
            'total_operations', jsonb_array_length(input_operations),
            'successful', v_success_count,
            'failed', v_error_count
        ),
        'metadata', jsonb_build_object(
            'trigger', 'api_bulk_update',
            'processed_at', NOW()
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

## GraphQL Schema Examples

**Type Definitions:**
```python
@fraiseql.success
class UpdatePostSuccess:
    post: Post
    updated_fields: list[str]
    message: str = "Post updated successfully"
    metadata: dict[str, Any] | None = None

@fraiseql.failure
class PostNotFoundError:
    message: str = "Post not found"
    error_code: str = "NOT_FOUND"
    requested_id: str
    metadata: dict[str, Any] | None = None

@fraiseql.failure
class PostAlreadyPublishedError:
    message: str = "Post is already published"
    error_code: str = "ALREADY_PUBLISHED"
    post: Post
    published_at: datetime
    metadata: dict[str, Any] | None = None

@fraiseql.failure
class NoChangesError:
    message: str = "No changes detected"
    error_code: str = "NO_CHANGES"
    current_post: Post
    metadata: dict[str, Any] | None = None
```

## Integration with FraiseQL Features

**With Authentication:**
```python
@fraiseql.mutation
@requires_auth
async def update_my_post(
    info: GraphQLResolveInfo,
    post_id: UUID,
    input: UpdatePostInput
) -> UpdatePostResult:
    """Update user's own post with ownership check."""
    db = info.context["db"]
    user = info.context["user"]

    # Call function with user context
    result = await db.call_function(
        "app.update_user_post",  # Function checks ownership
        input_pk_organization=user.tenant_id,
        input_updated_by=user.user_id,
        input_pk_post=post_id,
        input_payload=input.to_dict()
    )

    # Handle result (same pattern as above)
    return parse_mutation_result(result, UpdatePostSuccess, UpdatePostError)
```

**With Caching Integration:**
```python
@fraiseql.mutation
async def create_post(info, input: CreatePostInput) -> CreatePostResult:
    """Create post with automatic cache invalidation."""
    result = await db.call_function("app.create_post", ...)

    if result["status"] == "new":
        # Mutation result includes cache invalidation metadata
        cache_keys = result["extra_metadata"].get("invalidated_cache_keys", [])

        # Optionally trigger additional cache operations
        await info.context["cache"].invalidate_keys(cache_keys)

    return parse_mutation_result(result, CreatePostSuccess, CreatePostError)
```
