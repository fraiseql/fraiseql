-- ============================================================================
-- Pattern: Multiple Field Validation (Pattern 2)
-- ============================================================================
-- Use Case: Validate multiple fields and return all errors at once
-- Benefits: Better UX (show all errors), easier form field mapping
--
-- This example shows:
-- - Collecting multiple validation errors
-- - Using build_error_object() helper
-- - Returning explicit errors in metadata.errors
-- - Proper error structure for frontend consumption
-- ============================================================================

CREATE OR REPLACE FUNCTION create_user_with_validation(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    validation_errors jsonb := '[]'::jsonb;

    -- Input variables
    user_email text := input_payload->>'email';
    user_name text := input_payload->>'name';
    user_age int := (input_payload->>'age')::int;
    user_password text := input_payload->>'password';
BEGIN
    -- ========================================================================
    -- Collect All Validation Errors
    -- ========================================================================

    -- Email validation
    IF user_email IS NULL OR trim(user_email) = '' THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'email_required',
                'Email address is required',
                jsonb_build_object('field', 'email', 'constraint', 'required')
            );
    ELSIF user_email !~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$' THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'email_invalid_format',
                'Email format is invalid',
                jsonb_build_object(
                    'field', 'email',
                    'constraint', 'format',
                    'example', 'user@example.com'
                )
            );
    END IF;

    -- Name validation
    IF user_name IS NULL OR trim(user_name) = '' THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'name_required',
                'Full name is required',
                jsonb_build_object('field', 'name', 'constraint', 'required')
            );
    ELSIF length(user_name) < 2 THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'name_too_short',
                'Name must be at least 2 characters',
                jsonb_build_object(
                    'field', 'name',
                    'constraint', 'minLength',
                    'minLength', 2,
                    'actualLength', length(user_name)
                )
            );
    END IF;

    -- Age validation
    IF user_age IS NULL THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'age_required',
                'Age is required',
                jsonb_build_object('field', 'age', 'constraint', 'required')
            );
    ELSIF user_age < 13 THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'age_too_young',
                'Must be at least 13 years old',
                jsonb_build_object(
                    'field', 'age',
                    'constraint', 'minimum',
                    'minimum', 13,
                    'actual', user_age
                )
            );
    ELSIF user_age > 150 THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'age_unrealistic',
                'Age seems unrealistic',
                jsonb_build_object(
                    'field', 'age',
                    'constraint', 'maximum',
                    'maximum', 150,
                    'actual', user_age
                )
            );
    END IF;

    -- Password validation
    IF user_password IS NULL OR trim(user_password) = '' THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'password_required',
                'Password is required',
                jsonb_build_object('field', 'password', 'constraint', 'required')
            );
    ELSIF length(user_password) < 8 THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'password_too_short',
                'Password must be at least 8 characters',
                jsonb_build_object(
                    'field', 'password',
                    'constraint', 'minLength',
                    'minLength', 8,
                    'actualLength', length(user_password)
                )
            );
    END IF;

    -- ========================================================================
    -- Return Validation Errors if Any
    -- ========================================================================

    IF jsonb_array_length(validation_errors) > 0 THEN
        result.status := 'failed:validation';
        result.message := format('%s validation error(s)', jsonb_array_length(validation_errors));
        result.metadata := jsonb_build_object('errors', validation_errors);

        -- Optional: Validate response structure in debug mode
        PERFORM mutation_assert(
            validate_mutation_response(result),
            'Validation response structure invalid'
        );

        RETURN result;
    END IF;

    -- ========================================================================
    -- Check Business Rules
    -- ========================================================================

    -- Duplicate email check
    IF EXISTS (SELECT 1 FROM users WHERE email = user_email) THEN
        result.status := 'conflict:duplicate_email';
        result.message := 'Email address already registered';
        result.metadata := jsonb_build_object(
            'errors', jsonb_build_array(
                build_error_object(
                    409,
                    'duplicate_email',
                    'This email is already registered',
                    jsonb_build_object('field', 'email', 'value', user_email)
                )
            )
        );
        RETURN result;
    END IF;

    -- ========================================================================
    -- Create User
    -- ========================================================================

    INSERT INTO users (email, name, age, password_hash)
    VALUES (
        user_email,
        user_name,
        user_age,
        crypt(user_password, gen_salt('bf'))  -- bcrypt hash
    )
    RETURNING * INTO user_record;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'created';
    result.message := 'User created successfully';
    result.entity := row_to_json(user_record);
    result.entity_id := user_record.id::text;
    result.entity_type := 'User';

    -- Validate success response
    PERFORM mutation_assert(
        validate_mutation_response(result),
        'Success response structure invalid'
    );

    RETURN result;

EXCEPTION
    WHEN OTHERS THEN
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Usage Examples
-- ============================================================================

-- Valid input - Success
SELECT * FROM create_user_with_validation('{
  "email": "john@example.com",
  "name": "John Doe",
  "age": 25,
  "password": "secure123"
}'::jsonb);
-- Returns: status='created', entity with user data

-- Invalid input - Multiple errors
SELECT * FROM create_user_with_validation('{
  "email": "invalid-email",
  "name": "J",
  "age": 10,
  "password": "short"
}'::jsonb);
-- Returns: status='failed:validation', metadata.errors with 4 errors

-- Duplicate email
SELECT * FROM create_user_with_validation('{
  "email": "john@example.com",
  "name": "Jane Doe",
  "age": 30,
  "password": "secure456"
}'::jsonb);
-- Returns: status='conflict:duplicate_email'

-- ============================================================================
-- GraphQL Response Example
-- ============================================================================

/*
{
  "data": {
    "createUser": {
      "__typename": "CreateUserError",
      "code": 422,
      "status": "failed:validation",
      "message": "4 validation error(s)",
      "errors": [
        {
          "code": 422,
          "identifier": "email_invalid_format",
          "message": "Email format is invalid",
          "details": {
            "field": "email",
            "constraint": "format",
            "example": "user@example.com"
          }
        },
        {
          "code": 422,
          "identifier": "name_too_short",
          "message": "Name must be at least 2 characters",
          "details": {
            "field": "name",
            "constraint": "minLength",
            "minLength": 2,
            "actualLength": 1
          }
        },
        {
          "code": 422,
          "identifier": "age_too_young",
          "message": "Must be at least 13 years old",
          "details": {
            "field": "age",
            "constraint": "minimum",
            "minimum": 13,
            "actual": 10
          }
        },
        {
          "code": 422,
          "identifier": "password_too_short",
          "message": "Password must be at least 8 characters",
          "details": {
            "field": "password",
            "constraint": "minLength",
            "minLength": 8,
            "actualLength": 5
          }
        }
      ]
    }
  }
}
*/

-- ============================================================================
-- Frontend Usage (TypeScript/React)
-- ============================================================================

/*
// Map errors to form fields
const fieldErrors = response.errors.reduce((acc, err) => {
  if (err.details?.field) {
    acc[err.details.field] = err.message;
  }
  return acc;
}, {});

// Result: { email: "Email format is invalid", name: "Name must be at least 2 characters", ... }
*/
