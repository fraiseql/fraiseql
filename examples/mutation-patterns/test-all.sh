#!/bin/bash

# ============================================================================
# Test Script for FraiseQL Mutation Patterns
# ============================================================================
# This script tests all mutation pattern examples to ensure they work correctly.
# Run this after loading the schema.
# ============================================================================

set -e  # Exit on any error

DB_NAME="fraiseql_patterns"
PSQL="psql -d $DB_NAME -q"

echo "🧪 Testing FraiseQL Mutation Patterns"
echo "====================================="

# Check if database exists
if ! psql -lqt | cut -d \| -f 1 | grep -qw "$DB_NAME"; then
    echo "❌ Database '$DB_NAME' does not exist. Run: createdb $DB_NAME"
    exit 1
fi

echo "✅ Database '$DB_NAME' exists"

# Load schema if not already loaded
echo "📦 Ensuring schema is loaded..."
$PSQL -c "SELECT 1 FROM users LIMIT 1;" 2>/dev/null || {
    echo "Loading schema..."
    $PSQL -f schema.sql
    echo "✅ Schema loaded"
}

# Test basic create function
echo ""
echo "🧪 Testing basic create function..."
$PSQL -c "
DO \$\$
DECLARE
    result mutation_response;
BEGIN
    -- Test success case
    result := create_user('{\"email\": \"test@example.com\", \"name\": \"Test User\"}'::jsonb);
    ASSERT result.status = 'created', 'Expected created, got ' || result.status;

    -- Test validation error
    result := create_user('{\"email\": \"\", \"name\": \"Test User\"}'::jsonb);
    ASSERT result.status = 'failed:validation', 'Expected failed:validation, got ' || result.status;

    RAISE NOTICE '✅ Basic create function tests passed';
END;
\$\$;"

# Test validation helpers
echo ""
echo "🧪 Testing validation helpers..."
$PSQL -c "
DO \$\$
BEGIN
    -- Test status format validation
    ASSERT validate_status_format('created') = true;
    ASSERT validate_status_format('failed:validation') = true;
    ASSERT validate_status_format('invalid') = false;

    -- Test error extraction
    ASSERT extract_identifier('failed:validation') = 'validation';
    ASSERT extract_identifier('created') = 'general_error';

    -- Test code mapping
    ASSERT get_expected_code('created') = 201;
    ASSERT get_expected_code('failed:validation') = 422;
    ASSERT get_expected_code('not_found:user') = 404;

    RAISE NOTICE '✅ Validation helper tests passed';
END;
\$\$;"

# Test multiple field validation
echo ""
echo "🧪 Testing multiple field validation..."
$PSQL -c "
DO \$\$
DECLARE
    result mutation_response;
BEGIN
    -- Test success case
    result := create_user_with_validation('{
        \"email\": \"valid@example.com\",
        \"name\": \"Valid User\",
        \"age\": 25,
        \"password\": \"securepassword\"
    }'::jsonb);
    ASSERT result.status = 'created', 'Expected created, got ' || result.status;

    -- Test multiple validation errors
    result := create_user_with_validation('{
        \"email\": \"invalid-email\",
        \"name\": \"X\",
        \"age\": 5,
        \"password\": \"short\"
    }'::jsonb);
    ASSERT result.status = 'failed:validation', 'Expected failed:validation, got ' || result.status;

    -- Check that errors array exists and has multiple errors
    ASSERT result.metadata->'errors' IS NOT NULL, 'Expected errors array';
    ASSERT jsonb_array_length(result.metadata->'errors') = 4, 'Expected 4 validation errors';

    RAISE NOTICE '✅ Multiple field validation tests passed';
END;
\$\$;"

echo ""
echo "🎉 All tests passed! Mutation patterns are working correctly."
echo ""
echo "📚 Next steps:"
echo "   - Explore individual examples in each subdirectory"
echo "   - Adapt patterns to your specific use cases"
echo "   - Check the README.md in each folder for more details"
