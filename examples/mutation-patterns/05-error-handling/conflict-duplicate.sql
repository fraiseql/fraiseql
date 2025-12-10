-- ============================================================================
-- Pattern: Duplicate Conflict Handling
-- ============================================================================
-- Use Case: Handle unique constraint violations gracefully
-- Benefits: Clear error messages, prevents data corruption
--
-- This example shows:
-- - Checking for duplicates before insert
-- - Proper conflict status codes
-- - Different identifiers for different constraint types
-- ============================================================================

CREATE OR REPLACE FUNCTION register_user(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    user_email text;
    user_name text;
    user_record record;
BEGIN
    -- ========================================================================
    -- Extract Input
    -- ========================================================================

    user_email := input_payload->>'email';
    user_name := input_payload->>'name';

    -- ========================================================================
    -- Basic Validation
    -- ========================================================================

    IF user_email IS NULL OR trim(user_email) = '' THEN
        result.status := 'failed:validation';
        result.message := 'Email is required';
        RETURN result;
    END IF;

    IF user_name IS NULL OR trim(user_name) = '' THEN
        result.status := 'failed:validation';
        result.message := 'Name is required';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Check for Email Duplicate
    -- ========================================================================

    IF EXISTS (SELECT 1 FROM users WHERE email = user_email) THEN
        result.status := 'conflict:duplicate_email';
        result.message := 'An account with this email already exists';
        result.metadata := jsonb_build_object(
            'errors', jsonb_build_array(
                build_error_object(
                    409,
                    'duplicate_email',
                    'This email address is already registered',
                    jsonb_build_object('field', 'email', 'value', user_email)
                )
            )
        );
        RETURN result;
    END IF;

    -- ========================================================================
    -- Create User
    -- ========================================================================

    INSERT INTO users (email, name)
    VALUES (user_email, user_name)
    RETURNING * INTO user_record;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'created';
    result.message := 'User registered successfully';
    result.entity := row_to_json(user_record);
    result.entity_id := user_record.id::text;
    result.entity_type := 'User';

    RETURN result;

EXCEPTION
    -- Handle unique constraint violations (alternative to pre-check)
    WHEN unique_violation THEN
        IF SQLERRM LIKE '%email%' THEN
            result.status := 'conflict:duplicate_email';
            result.message := 'An account with this email already exists';
        ELSE
            result.status := 'conflict:duplicate';
            result.message := 'Duplicate entry: ' || SQLERRM;
        END IF;
        RETURN result;

    WHEN OTHERS THEN
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Alternative: Username Duplicate Check
-- ============================================================================

CREATE OR REPLACE FUNCTION create_tag(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    tag_name text;
    tag_color text;
    tag_record record;
BEGIN
    tag_name := input_payload->>'name';
    tag_color := COALESCE(input_payload->>'color', '#666666');

    -- Validation
    IF tag_name IS NULL OR trim(tag_name) = '' THEN
        result.status := 'failed:validation';
        result.message := 'Tag name is required';
        RETURN result;
    END IF;

    -- Check for duplicate tag name
    IF EXISTS (SELECT 1 FROM tags WHERE name = tag_name) THEN
        result.status := 'conflict:duplicate_tag_name';
        result.message := format('Tag "%s" already exists', tag_name);
        result.metadata := jsonb_build_object(
            'errors', jsonb_build_array(
                build_error_object(
                    409,
                    'duplicate_tag_name',
                    'A tag with this name already exists',
                    jsonb_build_object('field', 'name', 'value', tag_name)
                )
            )
        );
        RETURN result;
    END IF;

    -- Create tag
    INSERT INTO tags (name, color)
    VALUES (tag_name, tag_color)
    RETURNING * INTO tag_record;

    result.status := 'created';
    result.message := 'Tag created successfully';
    result.entity := row_to_json(tag_record);
    result.entity_id := tag_record.id::text;
    result.entity_type := 'Tag';

    RETURN result;

EXCEPTION
    WHEN unique_violation THEN
        result.status := 'conflict:duplicate';
        result.message := 'Tag name must be unique';
        RETURN result;

    WHEN OTHERS THEN
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Usage Examples
-- ============================================================================

-- Try to register with existing email
SELECT * FROM register_user('{
    "email": "john@example.com",
    "name": "John Doe"
}'::jsonb);
-- Returns: status='conflict:duplicate_email', detailed error in metadata

-- Try to create duplicate tag
-- First create a tag
SELECT * FROM create_tag('{"name": "tutorial"}'::jsonb);
-- Returns: status='created'

-- Then try to create the same tag again
SELECT * FROM create_tag('{"name": "tutorial"}'::jsonb);
-- Returns: status='conflict:duplicate_tag_name'

-- Success case - new unique email
SELECT * FROM register_user('{
    "email": "newuser@example.com",
    "name": "New User"
}'::jsonb);
-- Returns: status='created', entity with user data

-- ============================================================================
-- GraphQL Response for Duplicate Errors
-- ============================================================================

/*
mutation RegisterUser($input: RegisterUserInput!) {
  registerUser(input: $input) {
    ... on RegisterUserSuccess {
      user {
        id
        email
        name
      }
    }
    ... on ConflictError {
      code          # 409
      identifier    # "duplicate_email"
      message       # "An account with this email already exists"
      errors {
        code
        identifier
        message
        details {
          field     # "email"
          value     # "john@example.com"
        }
      }
    }
  }
}
*/
