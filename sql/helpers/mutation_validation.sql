-- ============================================================================
-- FraiseQL Mutation Validation Helpers
-- ============================================================================
-- These functions help catch common mistakes in mutation functions during
-- development, before runtime errors occur.
--
-- Usage:
--   1. Include in your migration: \i sql/helpers/mutation_validation.sql
--   2. Use in your mutation functions to validate responses
--   3. Remove assertions in production (or keep for safety)
-- ============================================================================

-- ----------------------------------------------------------------------------
-- validate_status_format: Check status string follows FraiseQL convention
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION validate_status_format(status text)
RETURNS boolean AS $$
BEGIN
    -- Valid patterns:
    -- - Simple: 'created', 'updated', 'deleted', 'success'
    -- - With identifier: 'failed:validation', 'not_found:user', etc.
    RETURN status ~ '^(success|created|updated|deleted|failed|not_found|conflict|unauthorized|forbidden|timeout|noop)(:.+)?$';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION validate_status_format IS
'Validates that status string follows FraiseQL convention: prefix or prefix:identifier';

-- Usage example:
-- ASSERT validate_status_format(result.status),
--     format('Invalid status format: %s', result.status);

-- ----------------------------------------------------------------------------
-- validate_errors_array: Check metadata.errors structure
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION validate_errors_array(metadata jsonb)
RETURNS boolean AS $$
DECLARE
    errors jsonb;
    error_obj jsonb;
BEGIN
    -- If no metadata, valid (auto-generation will handle it)
    IF metadata IS NULL THEN
        RETURN true;
    END IF;

    -- If no errors in metadata, valid
    errors := metadata->'errors';
    IF errors IS NULL THEN
        RETURN true;
    END IF;

    -- Must be an array
    IF jsonb_typeof(errors) != 'array' THEN
        RAISE NOTICE 'metadata.errors must be a JSONB array, got: %', jsonb_typeof(errors);
        RETURN false;
    END IF;

    -- Each error must have required fields
    FOR error_obj IN SELECT jsonb_array_elements(errors)
    LOOP
        -- Check required fields exist
        IF NOT (error_obj ? 'code' AND error_obj ? 'identifier' AND error_obj ? 'message') THEN
            RAISE NOTICE 'Error object missing required fields (code, identifier, message): %', error_obj;
            RETURN false;
        END IF;

        -- Check types
        IF jsonb_typeof(error_obj->'code') != 'number' THEN
            RAISE NOTICE 'Error code must be number, got: %', jsonb_typeof(error_obj->'code');
            RETURN false;
        END IF;

        IF jsonb_typeof(error_obj->'identifier') != 'string' THEN
            RAISE NOTICE 'Error identifier must be string, got: %', jsonb_typeof(error_obj->'identifier');
            RETURN false;
        END IF;

        IF jsonb_typeof(error_obj->'message') != 'string' THEN
            RAISE NOTICE 'Error message must be string, got: %', jsonb_typeof(error_obj->'message');
            RETURN false;
        END IF;
    END LOOP;

    RETURN true;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION validate_errors_array IS
'Validates that metadata.errors is properly structured for FraiseQL';

-- Usage example:
-- ASSERT validate_errors_array(result.metadata),
--     'Invalid metadata.errors structure';

-- ----------------------------------------------------------------------------
-- validate_mutation_response: Comprehensive validation
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION validate_mutation_response(result mutation_response)
RETURNS boolean AS $$
BEGIN
    -- Status format
    IF NOT validate_status_format(result.status) THEN
        RAISE NOTICE 'Invalid status format: %', result.status;
        RETURN false;
    END IF;

    -- Message required
    IF result.message IS NULL OR trim(result.message) = '' THEN
        RAISE NOTICE 'Message is required';
        RETURN false;
    END IF;

    -- Errors array structure
    IF NOT validate_errors_array(result.metadata) THEN
        RETURN false;
    END IF;

    -- Success cases should have entity (unless DELETE)
    IF result.status IN ('created', 'updated', 'success') THEN
        IF result.entity IS NULL THEN
            RAISE NOTICE 'Success status "%" should have entity data', result.status;
            RETURN false;
        END IF;
    END IF;

    RETURN true;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION validate_mutation_response IS
'Comprehensive validation of mutation_response before returning';

-- Usage example:
-- ASSERT validate_mutation_response(result),
--     'Mutation response validation failed';

-- ----------------------------------------------------------------------------
-- get_expected_code: Get HTTP code for status string
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION get_expected_code(status text)
RETURNS integer AS $$
BEGIN
    CASE
        WHEN status LIKE 'created%' THEN RETURN 201;
        WHEN status LIKE 'success%' THEN RETURN 200;
        WHEN status LIKE 'updated%' THEN RETURN 200;
        WHEN status LIKE 'deleted%' THEN RETURN 200;
        WHEN status LIKE 'failed:%' THEN RETURN 422;
        WHEN status LIKE 'not_found:%' THEN RETURN 404;
        WHEN status LIKE 'conflict:%' THEN RETURN 409;
        WHEN status LIKE 'unauthorized:%' THEN RETURN 401;
        WHEN status LIKE 'forbidden:%' THEN RETURN 403;
        WHEN status LIKE 'timeout:%' THEN RETURN 408;
        WHEN status LIKE 'noop:%' THEN RETURN 422;
        ELSE RETURN 500;
    END CASE;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION get_expected_code IS
'Returns the HTTP code that FraiseQL will generate for a given status string';

-- Usage example:
-- SELECT get_expected_code('failed:validation');  -- Returns 422

-- ----------------------------------------------------------------------------
-- extract_identifier: Extract identifier from status string
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION extract_identifier(status text)
RETURNS text AS $$
BEGIN
    -- Split on colon and return second part
    IF position(':' in status) > 0 THEN
        RETURN split_part(status, ':', 2);
    ELSE
        RETURN 'general_error';
    END IF;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION extract_identifier IS
'Extracts the identifier part from a status string (part after colon)';

-- Usage example:
-- SELECT extract_identifier('failed:validation');  -- Returns 'validation'

-- ----------------------------------------------------------------------------
-- build_error_object: Helper to build properly formatted error object
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION build_error_object(
    p_code integer,
    p_identifier text,
    p_message text,
    p_details jsonb DEFAULT NULL
)
RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'code', p_code,
        'identifier', p_identifier,
        'message', p_message,
        'details', p_details
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION build_error_object IS
'Builds a properly formatted error object for metadata.errors array';

-- Usage example:
-- SELECT build_error_object(422, 'invalid_email', 'Email format invalid', '{"field": "email"}'::jsonb);

-- ----------------------------------------------------------------------------
-- mutation_assert: Conditional assertion for development
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION mutation_assert(
    condition boolean,
    error_message text
)
RETURNS void AS $$
BEGIN
    -- Only assert in development (when debug is enabled)
    -- Set: ALTER DATABASE mydb SET fraiseql.debug = 'on';
    IF current_setting('fraiseql.debug', true) = 'on' THEN
        IF NOT condition THEN
            RAISE EXCEPTION '%', error_message;
        END IF;
    ELSIF NOT condition THEN
        -- In production, log warning but don't fail
        RAISE WARNING '%', error_message;
    END IF;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION mutation_assert IS
'Conditional assertion that throws in debug mode, warns in production';

-- Usage example:
-- PERFORM mutation_assert(
--     validate_mutation_response(result),
--     format('Validation failed: %s', result.status)
-- );

-- ============================================================================
-- Example Usage in Mutation Function
-- ============================================================================

/*
CREATE OR REPLACE FUNCTION create_user(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    user_email text;
BEGIN
    -- Extract input
    user_email := input_payload->>'email';

    -- Validation
    IF user_email IS NULL THEN
        result.status := 'failed:validation';
        result.message := 'Email is required';

        -- Optional: Add explicit errors
        result.metadata := jsonb_build_object(
            'errors', jsonb_build_array(
                build_error_object(422, 'email_required', 'Email is required',
                    jsonb_build_object('field', 'email'))
            )
        );

        -- Validate before returning
        PERFORM mutation_assert(
            validate_mutation_response(result),
            'Mutation response validation failed'
        );

        RETURN result;
    END IF;

    -- ... rest of function

    -- Success
    result.status := 'created';
    result.message := 'User created';
    result.entity := row_to_json(NEW);

    -- Validate before returning
    PERFORM mutation_assert(
        validate_mutation_response(result),
        'Mutation response validation failed'
    );

    RETURN result;
END;
$$ LANGUAGE plpgsql;
*/

-- ============================================================================
-- Tests
-- ============================================================================

DO $$
BEGIN
    -- Test validate_status_format
    ASSERT validate_status_format('created') = true, 'created should be valid';
    ASSERT validate_status_format('failed:validation') = true, 'failed:validation should be valid';
    ASSERT validate_status_format('not_found:user') = true, 'not_found:user should be valid';
    ASSERT validate_status_format('invalid_format') = false, 'invalid_format should be invalid';
    ASSERT validate_status_format('failed-validation') = false, 'failed-validation (dash) should be invalid';

    -- Test extract_identifier
    ASSERT extract_identifier('failed:validation') = 'validation', 'Should extract validation';
    ASSERT extract_identifier('not_found:user') = 'user', 'Should extract user';
    ASSERT extract_identifier('created') = 'general_error', 'Should return general_error for no colon';

    -- Test get_expected_code
    ASSERT get_expected_code('created') = 201, 'created should map to 201';
    ASSERT get_expected_code('failed:validation') = 422, 'failed:* should map to 422';
    ASSERT get_expected_code('not_found:user') = 404, 'not_found:* should map to 404';
    ASSERT get_expected_code('conflict:duplicate') = 409, 'conflict:* should map to 409';

    RAISE NOTICE 'All validation helper tests passed!';
END;
$$;
