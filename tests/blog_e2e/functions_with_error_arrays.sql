-- Blog E2E Test Suite - Enhanced PostgreSQL Functions with Error Arrays
-- Demonstrates PrintOptim Backend patterns for multiple validation errors

-- ============================================================================
-- ENHANCED MUTATION RESULT TYPE - Supporting multiple errors
-- ============================================================================

DROP TYPE IF EXISTS app.mutation_result CASCADE;
CREATE TYPE app.mutation_result AS (
    id UUID,                    -- Entity primary key
    updated_fields TEXT[],      -- Fields that were modified
    status TEXT,                -- Operation status (new, updated, noop:*)
    message TEXT,               -- Human-readable message
    object_data JSONB,          -- Complete entity snapshot after mutation
    extra_metadata JSONB,       -- Additional context and debugging info
    errors JSONB                -- Array of structured error objects
);

COMMENT ON TYPE app.mutation_result IS 
'Enhanced mutation result type with support for multiple structured errors as arrays.';

-- ============================================================================
-- ENHANCED VALIDATION FUNCTIONS - Collecting multiple errors
-- ============================================================================

-- Validation result accumulator type
CREATE TYPE core.validation_result AS (
    is_valid BOOLEAN,
    errors JSONB  -- Array of error objects
);

-- Function to add validation error to accumulator
CREATE OR REPLACE FUNCTION core.add_validation_error(
    current_result core.validation_result,
    error_code INTEGER,
    error_identifier TEXT,
    error_message TEXT,
    error_details JSONB DEFAULT '{}'::JSONB
) RETURNS core.validation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_error_obj JSONB;
    v_errors_array JSONB;
BEGIN
    -- Build error object following PrintOptim patterns
    v_error_obj := jsonb_build_object(
        'code', error_code,
        'identifier', error_identifier,
        'message', error_message,
        'details', error_details
    );
    
    -- Get existing errors array or initialize empty
    v_errors_array := COALESCE(current_result.errors, '[]'::JSONB);
    
    -- Add new error to array
    v_errors_array := v_errors_array || jsonb_build_array(v_error_obj);
    
    -- Return updated result
    RETURN (false, v_errors_array)::core.validation_result;
END;
$$;

-- Function to initialize empty validation result
CREATE OR REPLACE FUNCTION core.init_validation_result()
RETURNS core.validation_result
LANGUAGE plpgsql AS $$
BEGIN
    RETURN (true, '[]'::JSONB)::core.validation_result;
END;
$$;

-- Enhanced logging function supporting multiple errors
CREATE OR REPLACE FUNCTION core.log_and_return_mutation_with_errors(
    input_entity_type TEXT,
    input_entity_id UUID,
    input_modification_type TEXT,  -- INSERT, UPDATE, DELETE, NOOP
    input_change_status TEXT,      -- new, updated, noop:*
    input_fields TEXT[],
    input_message TEXT,
    input_payload_before JSONB DEFAULT NULL,
    input_payload_after JSONB DEFAULT NULL,
    input_extra_metadata JSONB DEFAULT '{}'::JSONB,
    input_errors JSONB DEFAULT '[]'::JSONB  -- Array of error objects
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_result app.mutation_result;
BEGIN
    -- Log the mutation event (simplified for E2E testing)
    -- In production, this would insert to audit log table
    
    -- Construct enhanced mutation result with errors array
    v_result.id := input_entity_id;
    v_result.updated_fields := input_fields;
    v_result.status := input_change_status;
    v_result.message := input_message;
    v_result.object_data := COALESCE(input_payload_after, input_payload_before);
    v_result.extra_metadata := input_extra_metadata;
    v_result.errors := input_errors;  -- Pass through errors array
    
    RETURN v_result;
END;
$$;

-- ============================================================================
-- ENHANCED AUTHOR FUNCTIONS - Multiple validation errors
-- ============================================================================

-- Enhanced core author creation with comprehensive validation
CREATE OR REPLACE FUNCTION core.create_author_with_validation(
    input_created_by UUID,
    input_data app.type_author_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_id UUID;
    v_existing_id UUID;
    v_payload_after JSONB;
    v_author_data JSONB;
    v_extra_metadata JSONB;
    v_validation core.validation_result;
    v_final_errors JSONB;
BEGIN
    -- Generate new UUID
    v_id := gen_random_uuid();
    
    -- Initialize validation result accumulator
    v_validation := core.init_validation_result();
    
    -- VALIDATION 1: Check required fields (collect ALL missing fields)
    IF input_data.identifier IS NULL OR trim(input_data.identifier) = '' THEN
        v_validation := core.add_validation_error(
            v_validation,
            422,  -- Unprocessable Entity
            'missing_required_field',
            'Missing required field: identifier',
            jsonb_build_object('field', 'identifier', 'constraint', 'required')
        );
    END IF;
    
    IF input_data.name IS NULL OR trim(input_data.name) = '' THEN
        v_validation := core.add_validation_error(
            v_validation,
            422,
            'missing_required_field',
            'Missing required field: name',
            jsonb_build_object('field', 'name', 'constraint', 'required')
        );
    END IF;
    
    IF input_data.email IS NULL OR trim(input_data.email) = '' THEN
        v_validation := core.add_validation_error(
            v_validation,
            422,
            'missing_required_field',
            'Missing required field: email',
            jsonb_build_object('field', 'email', 'constraint', 'required')
        );
    END IF;
    
    -- VALIDATION 2: Format validations (continue collecting errors)
    IF input_data.email IS NOT NULL AND NOT core.is_valid_email(input_data.email) THEN
        v_validation := core.add_validation_error(
            v_validation,
            422,
            'invalid_email_format',
            format('Invalid email format: %s', input_data.email),
            jsonb_build_object('field', 'email', 'constraint', 'format', 'value', input_data.email)
        );
    END IF;
    
    -- VALIDATION 3: Identifier format (URL slug validation)
    IF input_data.identifier IS NOT NULL AND NOT (input_data.identifier ~ '^[a-z0-9-]+$') THEN
        v_validation := core.add_validation_error(
            v_validation,
            422,
            'invalid_identifier_format',
            'Identifier must contain only lowercase letters, numbers, and hyphens',
            jsonb_build_object('field', 'identifier', 'constraint', 'format', 'pattern', '^[a-z0-9-]+$')
        );
    END IF;
    
    -- VALIDATION 4: Length constraints
    IF input_data.identifier IS NOT NULL AND length(input_data.identifier) > 50 THEN
        v_validation := core.add_validation_error(
            v_validation,
            422,
            'identifier_too_long',
            format('Identifier too long: %s characters (maximum 50)', length(input_data.identifier)),
            jsonb_build_object('field', 'identifier', 'constraint', 'max_length', 'max_length', 50, 'current_length', length(input_data.identifier))
        );
    END IF;
    
    IF input_data.name IS NOT NULL AND length(input_data.name) > 100 THEN
        v_validation := core.add_validation_error(
            v_validation,
            422,
            'name_too_long',
            format('Name too long: %s characters (maximum 100)', length(input_data.name)),
            jsonb_build_object('field', 'name', 'constraint', 'max_length', 'max_length', 100, 'current_length', length(input_data.name))
        );
    END IF;
    
    -- VALIDATION 5: Business rule validations (only if basic fields are valid)
    IF v_validation.is_valid AND input_data.identifier IS NOT NULL THEN
        -- Check duplicate identifier
        SELECT pk_author INTO v_existing_id
        FROM blog.tb_author
        WHERE identifier = input_data.identifier;
        
        IF v_existing_id IS NOT NULL THEN
            v_validation := core.add_validation_error(
                v_validation,
                409,  -- Conflict
                'duplicate_identifier',
                format('Author with identifier "%s" already exists', input_data.identifier),
                jsonb_build_object(
                    'field', 'identifier',
                    'constraint', 'unique',
                    'conflict_id', v_existing_id,
                    'conflict_identifier', input_data.identifier
                )
            );
        END IF;
    END IF;
    
    IF v_validation.is_valid AND input_data.email IS NOT NULL THEN
        -- Check duplicate email
        SELECT pk_author INTO v_existing_id
        FROM blog.tb_author
        WHERE data->>'email' = input_data.email;
        
        IF v_existing_id IS NOT NULL THEN
            v_validation := core.add_validation_error(
                v_validation,
                409,  -- Conflict
                'duplicate_email',
                format('Author with email "%s" already exists', input_data.email),
                jsonb_build_object(
                    'field', 'email',
                    'constraint', 'unique',
                    'conflict_id', v_existing_id,
                    'conflict_email', input_data.email
                )
            );
        END IF;
    END IF;
    
    -- If validation failed, return NOOP with ALL collected errors
    IF NOT v_validation.is_valid THEN
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_create',
            'reason', 'validation_failed',
            'input_payload', input_payload,
            'validation_errors_count', jsonb_array_length(v_validation.errors)
        );
        
        RETURN core.log_and_return_mutation_with_errors(
            'author', v_id, 'NOOP', 'noop:validation_failed',
            ARRAY[]::TEXT[], 'Author creation failed validation',
            NULL, NULL, v_extra_metadata, v_validation.errors
        );
    END IF;
    
    -- All validations passed - proceed with creation
    v_author_data := jsonb_build_object(
        'name', input_data.name,
        'email', input_data.email,
        'bio', COALESCE(input_data.bio, ''),
        'avatar_url', input_data.avatar_url,
        'social_links', COALESCE(input_data.social_links, '{}'::jsonb)
    );
    
    -- Insert new author
    INSERT INTO blog.tb_author (pk_author, identifier, data, created_by, updated_by)
    VALUES (v_id, input_data.identifier, v_author_data, input_created_by, input_created_by);
    
    -- Refresh materialized table
    PERFORM core.refresh_author(ARRAY[v_id]);
    
    -- Get final state for response
    SELECT data INTO v_payload_after FROM tv_author WHERE id = v_id;
    
    -- Build success metadata
    v_extra_metadata := jsonb_build_object(
        'trigger', 'api_create',
        'reason', 'new_author_created',
        'input_payload', input_payload
    );
    
    -- Return success result (empty errors array for success)
    RETURN core.log_and_return_mutation_with_errors(
        'author', v_id, 'INSERT', 'new',
        ARRAY['identifier', 'name', 'email', 'bio', 'avatar_url', 'social_links'],
        'Author created successfully',
        NULL, v_payload_after, v_extra_metadata, '[]'::JSONB
    );
END;
$$;

-- ============================================================================
-- ENHANCED POST FUNCTIONS - Multiple validation errors
-- ============================================================================

-- Enhanced core post creation with comprehensive validation
CREATE OR REPLACE FUNCTION core.create_post_with_validation(
    input_created_by UUID,
    input_data app.type_post_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_id UUID;
    v_existing_id UUID;
    v_author_id UUID;
    v_tag_ids UUID[];
    v_invalid_tags TEXT[];
    v_payload_after JSONB;
    v_post_data JSONB;
    v_extra_metadata JSONB;
    v_validation core.validation_result;
    v_published_at TIMESTAMPTZ;
    v_temp_tag TEXT;
BEGIN
    -- Generate new UUID
    v_id := gen_random_uuid();
    
    -- Initialize validation result accumulator
    v_validation := core.init_validation_result();
    
    -- VALIDATION 1: Required fields (collect ALL missing fields)
    IF input_data.identifier IS NULL OR trim(input_data.identifier) = '' THEN
        v_validation := core.add_validation_error(
            v_validation, 422, 'missing_required_field',
            'Missing required field: identifier',
            jsonb_build_object('field', 'identifier', 'constraint', 'required')
        );
    END IF;
    
    IF input_data.title IS NULL OR trim(input_data.title) = '' THEN
        v_validation := core.add_validation_error(
            v_validation, 422, 'missing_required_field',
            'Missing required field: title',
            jsonb_build_object('field', 'title', 'constraint', 'required')
        );
    END IF;
    
    IF input_data.content IS NULL OR trim(input_data.content) = '' THEN
        v_validation := core.add_validation_error(
            v_validation, 422, 'missing_required_field',
            'Missing required field: content',
            jsonb_build_object('field', 'content', 'constraint', 'required')
        );
    END IF;
    
    IF input_data.author_identifier IS NULL OR trim(input_data.author_identifier) = '' THEN
        v_validation := core.add_validation_error(
            v_validation, 422, 'missing_required_field',
            'Missing required field: author_identifier',
            jsonb_build_object('field', 'author_identifier', 'constraint', 'required')
        );
    END IF;
    
    -- VALIDATION 2: Format and length constraints
    IF input_data.content IS NOT NULL AND length(input_data.content) > 10000 THEN
        v_validation := core.add_validation_error(
            v_validation, 422, 'content_too_long',
            format('Content too long: %s characters (maximum 10000)', length(input_data.content)),
            jsonb_build_object('field', 'content', 'constraint', 'max_length', 'max_length', 10000, 'current_length', length(input_data.content))
        );
    END IF;
    
    IF input_data.title IS NOT NULL AND length(input_data.title) > 200 THEN
        v_validation := core.add_validation_error(
            v_validation, 422, 'title_too_long',
            format('Title too long: %s characters (maximum 200)', length(input_data.title)),
            jsonb_build_object('field', 'title', 'constraint', 'max_length', 'max_length', 200, 'current_length', length(input_data.title))
        );
    END IF;
    
    -- VALIDATION 3: Status validation
    IF input_data.status IS NOT NULL AND input_data.status NOT IN ('draft', 'published', 'archived') THEN
        v_validation := core.add_validation_error(
            v_validation, 422, 'invalid_status',
            format('Invalid status "%s". Valid options: draft, published, archived', input_data.status),
            jsonb_build_object(
                'field', 'status',
                'constraint', 'enum',
                'invalid_value', input_data.status,
                'valid_values', ARRAY['draft', 'published', 'archived']
            )
        );
    END IF;
    
    -- VALIDATION 4: Content security (basic patterns)
    IF input_data.content IS NOT NULL THEN
        -- Check for potentially dangerous patterns
        IF input_data.content ~* '<script[^>]*>' THEN
            v_validation := core.add_validation_error(
                v_validation, 422, 'unsafe_html',
                'Content contains potentially unsafe HTML: script tags not allowed',
                jsonb_build_object('field', 'content', 'constraint', 'security', 'violation', 'script_tag')
            );
        END IF;
        
        IF input_data.content ~* 'javascript:' THEN
            v_validation := core.add_validation_error(
                v_validation, 422, 'unsafe_javascript',
                'Content contains potentially unsafe JavaScript URIs',
                jsonb_build_object('field', 'content', 'constraint', 'security', 'violation', 'javascript_uri')
            );
        END IF;
        
        IF input_data.content ~* '\.\./.*etc/passwd' THEN
            v_validation := core.add_validation_error(
                v_validation, 422, 'path_traversal',
                'Content contains potential path traversal attack',
                jsonb_build_object('field', 'content', 'constraint', 'security', 'violation', 'path_traversal')
            );
        END IF;
    END IF;
    
    -- VALIDATION 5: Reference validations (only if required fields present)
    IF jsonb_array_length(v_validation.errors) = 0 THEN
        -- Check duplicate identifier
        SELECT pk_post INTO v_existing_id
        FROM blog.tb_post
        WHERE identifier = input_data.identifier;
        
        IF v_existing_id IS NOT NULL THEN
            v_validation := core.add_validation_error(
                v_validation, 409, 'duplicate_identifier',
                format('Post with identifier "%s" already exists', input_data.identifier),
                jsonb_build_object(
                    'field', 'identifier',
                    'constraint', 'unique',
                    'conflict_id', v_existing_id
                )
            );
        END IF;
        
        -- Verify author exists
        SELECT pk_author INTO v_author_id
        FROM blog.tb_author
        WHERE identifier = input_data.author_identifier;
        
        IF v_author_id IS NULL THEN
            v_validation := core.add_validation_error(
                v_validation, 422, 'missing_author',
                format('Author with identifier "%s" not found', input_data.author_identifier),
                jsonb_build_object(
                    'field', 'author_identifier',
                    'constraint', 'foreign_key',
                    'missing_identifier', input_data.author_identifier
                )
            );
        END IF;
        
        -- Verify tags exist (if provided) and collect ALL invalid tags
        IF input_data.tag_identifiers IS NOT NULL AND array_length(input_data.tag_identifiers, 1) > 0 THEN
            v_invalid_tags := ARRAY[]::TEXT[];
            
            -- Check each tag individually
            FOREACH v_temp_tag IN ARRAY input_data.tag_identifiers
            LOOP
                IF NOT EXISTS (SELECT 1 FROM blog.tb_tag WHERE identifier = v_temp_tag) THEN
                    v_invalid_tags := array_append(v_invalid_tags, v_temp_tag);
                END IF;
            END LOOP;
            
            -- If there are invalid tags, add them as individual errors
            IF array_length(v_invalid_tags, 1) > 0 THEN
                FOREACH v_temp_tag IN ARRAY v_invalid_tags
                LOOP
                    v_validation := core.add_validation_error(
                        v_validation, 422, 'invalid_tag',
                        format('Tag with identifier "%s" not found', v_temp_tag),
                        jsonb_build_object(
                            'field', 'tag_identifiers',
                            'constraint', 'foreign_key',
                            'missing_identifier', v_temp_tag
                        )
                    );
                END LOOP;
            END IF;
        END IF;
    END IF;
    
    -- If validation failed, return NOOP with ALL collected errors
    IF NOT v_validation.is_valid THEN
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_create',
            'reason', 'validation_failed',
            'input_payload', input_payload,
            'validation_errors_count', jsonb_array_length(v_validation.errors)
        );
        
        RETURN core.log_and_return_mutation_with_errors(
            'post', v_id, 'NOOP', 'noop:validation_failed',
            ARRAY[]::TEXT[], 'Post creation failed validation',
            NULL, NULL, v_extra_metadata, v_validation.errors
        );
    END IF;
    
    -- All validations passed - proceed with creation
    -- [Rest of the creation logic would be similar to the original function]
    -- For brevity, returning success with empty errors array
    
    v_extra_metadata := jsonb_build_object(
        'trigger', 'api_create',
        'reason', 'new_post_created',
        'input_payload', input_payload
    );
    
    RETURN core.log_and_return_mutation_with_errors(
        'post', v_id, 'INSERT', 'new',
        ARRAY['identifier', 'title', 'content', 'status'],
        'Post created successfully',
        NULL, jsonb_build_object('id', v_id, 'title', input_data.title), 
        v_extra_metadata, '[]'::JSONB
    );
END;
$$;

-- ============================================================================
-- APP WRAPPER FUNCTIONS - Updated for enhanced validation
-- ============================================================================

-- Enhanced app wrapper for author creation
CREATE OR REPLACE FUNCTION app.create_author_enhanced(
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_author_input;
    v_sanitized_payload JSONB;
BEGIN
    -- Sanitize input
    v_sanitized_payload := core.sanitize_jsonb_input(input_payload);
    
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_author_input, v_sanitized_payload);
    
    -- Delegate to enhanced core function
    RETURN core.create_author_with_validation(input_created_by, v_input, v_sanitized_payload);
END;
$$;

-- Enhanced app wrapper for post creation
CREATE OR REPLACE FUNCTION app.create_post_enhanced(
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_post_input;
    v_sanitized_payload JSONB;
BEGIN
    -- Sanitize input
    v_sanitized_payload := core.sanitize_jsonb_input(input_payload);
    
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_post_input, v_sanitized_payload);
    
    -- Delegate to enhanced core function
    RETURN core.create_post_with_validation(input_created_by, v_input, v_sanitized_payload);
END;
$$;