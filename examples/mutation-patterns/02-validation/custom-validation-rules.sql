-- ============================================================================
-- Pattern: Custom Validation Rules
-- ============================================================================
-- Use Case: Implement complex business validation logic
-- Benefits: Centralized business rules, consistent validation
--
-- This example shows:
-- - Custom business rules (email domain whitelist)
-- - Cross-field validation (age + consent)
-- - External service validation simulation
-- - Reusable validation functions
-- ============================================================================

CREATE OR REPLACE FUNCTION validate_email_domain(email text)
RETURNS boolean AS $$
BEGIN
    -- Business rule: Only allow company and partner domains
    RETURN email ~ '@(example\.com|partner\.com|acme\.corp)$';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION validate_age_consent(age int, has_consent boolean)
RETURNS boolean AS $$
BEGIN
    -- Business rule: Under 18 requires parental consent
    IF age < 18 THEN
        RETURN has_consent IS TRUE;
    END IF;
    RETURN true;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION create_user_with_rules(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    validation_errors jsonb := '[]'::jsonb;

    user_email text := input_payload->>'email';
    user_name text := input_payload->>'name';
    user_age int := (input_payload->>'age')::int;
    has_parental_consent boolean := (input_payload->>'parental_consent')::boolean;
    user_role text := input_payload->>'role';
    user_record record;
BEGIN
    -- ========================================================================
    -- Custom Validation Rules
    -- ========================================================================

    -- Rule 1: Email domain validation
    IF user_email IS NOT NULL AND NOT validate_email_domain(user_email) THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'email_domain_not_allowed',
                'Email domain not in whitelist. Use @example.com, @partner.com, or @acme.corp',
                jsonb_build_object(
                    'field', 'email',
                    'value', user_email,
                    'allowed_domains', '["example.com", "partner.com", "acme.corp"]'
                )
            );
    END IF;

    -- Rule 2: Age and consent validation
    IF user_age IS NOT NULL AND NOT validate_age_consent(user_age, has_parental_consent) THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'parental_consent_required',
                'Users under 18 require parental consent',
                jsonb_build_object(
                    'field', 'parental_consent',
                    'age', user_age,
                    'has_consent', has_parental_consent
                )
            );
    END IF;

    -- Rule 3: Name profanity check (simple example)
    IF user_name IS NOT NULL AND user_name ~* '(badword1|badword2|spam)' THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'name_contains_inappropriate_content',
                'Name contains inappropriate content',
                jsonb_build_object('field', 'name')
            );
    END IF;

    -- Rule 4: Role validation
    IF user_role IS NOT NULL AND user_role NOT IN ('user', 'moderator', 'admin') THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'invalid_role',
                'Role must be: user, moderator, or admin',
                jsonb_build_object(
                    'field', 'role',
                    'value', user_role,
                    'allowed_values', '["user", "moderator", "admin"]'
                )
            );
    END IF;

    -- Rule 5: Admin role requires age 21+
    IF user_role = 'admin' AND user_age < 21 THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'admin_age_requirement',
                'Admin role requires age 21 or older',
                jsonb_build_object(
                    'field', 'role',
                    'minimum_age', 21,
                    'actual_age', user_age
                )
            );
    END IF;

    -- Return validation errors if any
    IF jsonb_array_length(validation_errors) > 0 THEN
        result.status := 'failed:validation';
        result.message := format('%s validation error(s)', jsonb_array_length(validation_errors));
        result.metadata := jsonb_build_object('errors', validation_errors);
        RETURN result;
    END IF;

    -- ========================================================================
    -- Create User
    -- ========================================================================

    INSERT INTO users (email, name, age, role)
    VALUES (
        user_email,
        user_name,
        user_age,
        COALESCE(user_role, 'user')
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

-- Valid user
SELECT * FROM create_user_with_rules('{
    "email": "john@example.com",
    "name": "John Doe",
    "age": 25,
    "role": "user"
}'::jsonb);
-- Returns: status='created'

-- Invalid email domain
SELECT * FROM create_user_with_rules('{
    "email": "john@invalid.com",
    "name": "John Doe",
    "age": 25
}'::jsonb);
-- Returns: status='failed:validation', error='email_domain_not_allowed'

-- Minor without consent
SELECT * FROM create_user_with_rules('{
    "email": "teen@example.com",
    "name": "Teen User",
    "age": 16,
    "parental_consent": false
}'::jsonb);
-- Returns: status='failed:validation', error='parental_consent_required'

-- Admin role requires age 21+
SELECT * FROM create_user_with_rules('{
    "email": "young@example.com",
    "name": "Young Admin",
    "age": 19,
    "role": "admin"
}'::jsonb);
-- Returns: status='failed:validation', error='admin_age_requirement'
