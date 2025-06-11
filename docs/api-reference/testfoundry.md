# TestFoundry

TestFoundry is FraiseQL's automated test generation framework that creates comprehensive pgTAP tests for your GraphQL mutations and database operations. It analyzes your database schema and FraiseQL types to generate tests that ensure your API works correctly.

## Overview

TestFoundry eliminates the tedious work of writing database tests by automatically generating:

- Happy path tests for CRUD operations
- Constraint violation tests (unique, foreign key, check constraints)
- Soft delete validation tests
- Authorization and role-based access control tests
- Custom scenario tests based on your business logic

## Installation

TestFoundry is included as an extension with FraiseQL. To set it up:

```python
from fraiseql.extensions.testfoundry import setup_testfoundry

# Initialize TestFoundry in your database
setup_testfoundry(connection_string="postgresql://user:pass@localhost/db")
```

This will create the necessary schema, tables, and functions in your PostgreSQL database.

## Basic Usage

### Generating Tests for a Type

```python
from fraiseql.extensions.testfoundry import TestFoundryGenerator

# Define your FraiseQL types
@fraiseql.type
class User:
    id: UUID
    email: str = fraise_field(description="User's email address")
    name: str = fraise_field(description="User's full name")
    created_at: datetime = fraise_field(description="When the user was created")

@fraiseql.input
class CreateUserInput:
    email: str
    name: str
    password: str

# Generate tests
generator = TestFoundryGenerator(connection_string="postgresql://...")
generator.generate_tests_for_type(User, CreateUserInput)
```

### Running Generated Tests

The generated tests are pgTAP functions that can be run directly in PostgreSQL:

```sql
-- Run all generated tests
SELECT * FROM run_tests();

-- Run tests for a specific entity
SELECT * FROM test_create_user_happy_path();
SELECT * FROM test_create_user_duplicate_email();
SELECT * FROM test_create_user_invalid_input();
```

### Integration with pytest

Wrap pgTAP tests in pytest for CI/CD integration:

```python
import pytest
from fraiseql.extensions.testfoundry import run_pgtap_test

@pytest.mark.database
def test_user_creation(db_connection):
    result = run_pgtap_test(db_connection, "test_create_user_happy_path")
    assert result.passed
    assert result.test_count == result.pass_count
```

## Test Types

### Happy Path Tests

Tests the successful execution of operations:

```sql
-- Generated test example
CREATE OR REPLACE FUNCTION test_create_post_happy_path()
RETURNS SETOF text AS $$
BEGIN
    -- Arrange: Create test user
    INSERT INTO users (data) VALUES ('{"email": "test@example.com", "name": "Test User"}');

    -- Act: Create post
    PERFORM create_post('{"title": "Test Post", "content": "Content", "author_id": ...}');

    -- Assert: Post was created
    RETURN NEXT has_rows('SELECT 1 FROM posts WHERE data->>''title'' = ''Test Post''');
END;
$$ LANGUAGE plpgsql;
```

### Constraint Violation Tests

Tests that constraints are properly enforced:

```sql
-- Test unique constraint
CREATE OR REPLACE FUNCTION test_create_user_duplicate_email()
RETURNS SETOF text AS $$
BEGIN
    -- Create first user
    PERFORM create_user('{"email": "duplicate@example.com", "name": "User 1"}');

    -- Try to create second user with same email
    RETURN NEXT throws_ok(
        'SELECT create_user(''{"email": "duplicate@example.com", "name": "User 2"}'')',
        '23505',  -- unique_violation
        'duplicate key value violates unique constraint'
    );
END;
$$ LANGUAGE plpgsql;
```

### Authorization Tests

Tests role-based access control:

```sql
-- Test that only admins can delete users
CREATE OR REPLACE FUNCTION test_delete_user_requires_admin()
RETURNS SETOF text AS $$
BEGIN
    -- Set role to regular user
    SET LOCAL ROLE regular_user;

    -- Try to delete a user
    RETURN NEXT throws_ok(
        'SELECT delete_user(''user-id-123'')',
        '42501',  -- insufficient_privilege
        'permission denied'
    );
END;
$$ LANGUAGE plpgsql;
```

## Metadata Configuration

TestFoundry uses metadata tables to control test generation:

### Field Mapping

Configure how test data is generated for specific fields:

```sql
-- Configure email generation
INSERT INTO testfoundry.tb_field_mapping (entity_name, field_name, generation_type, pattern)
VALUES ('User', 'email', 'pattern', '{firstName}.{lastName}@{domain}');

-- Configure phone number format
INSERT INTO testfoundry.tb_field_mapping (entity_name, field_name, generation_type, validation_regex)
VALUES ('Contact', 'phone', 'regex', '^\+?[1-9]\d{1,14}$');
```

### Group Leaders

Define related fields that must be consistent:

```sql
-- Ensure country, state, and postal code are consistent
INSERT INTO testfoundry.tb_group_leader (entity_name, leader_field, dependent_fields)
VALUES ('Address', 'country', '["state", "postal_code", "city"]');
```

### Custom Scenarios

Define business-specific test scenarios:

```sql
INSERT INTO testfoundry.tb_test_scenario (
    entity_name,
    scenario_name,
    description,
    setup_function,
    assertion_function
) VALUES (
    'Order',
    'test_order_total_calculation',
    'Verify order total is calculated correctly with taxes and discounts',
    'testfoundry.setup_order_with_items',
    'testfoundry.assert_order_total_correct'
);
```

## Advanced Features

### Intelligent Data Generation

TestFoundry generates realistic test data:

```python
# Configures automatic generation of:
# - Valid email addresses
# - Phone numbers matching country codes
# - Postal codes valid for the selected country
# - URLs with proper protocols
# - Dates within reasonable ranges
```

### Dependency Resolution

Automatically handles foreign key relationships:

```python
# When testing post creation, TestFoundry will:
# 1. Create a valid user first
# 2. Use that user's ID as the author_id
# 3. Ensure all foreign key constraints are satisfied
```

### Temporal Testing

Test time-based business logic:

```sql
-- Test that expired subscriptions are handled correctly
CREATE OR REPLACE FUNCTION test_expired_subscription_access()
RETURNS SETOF text AS $$
BEGIN
    -- Create subscription that expired yesterday
    INSERT INTO subscriptions (data)
    VALUES (jsonb_build_object(
        'user_id', create_test_user(),
        'expires_at', CURRENT_DATE - INTERVAL '1 day'
    ));

    -- Test that access is denied
    RETURN NEXT throws_ok(
        'SELECT access_premium_content()',
        'subscription_expired'
    );
END;
$$ LANGUAGE plpgsql;
```

## Best Practices

1. **Run TestFoundry After Schema Changes**: Regenerate tests when you modify your database schema or FraiseQL types.

2. **Customize Test Data**: Use the metadata tables to ensure generated data matches your domain requirements.

3. **Add Custom Scenarios**: Don't rely solely on generated tests - add custom scenarios for complex business logic.

4. **Use in CI/CD**: Integrate TestFoundry tests into your continuous integration pipeline.

5. **Monitor Test Performance**: TestFoundry includes timing information to identify slow tests.

## Configuration Options

```python
# Configure TestFoundry behavior
generator = TestFoundryGenerator(
    connection_string="postgresql://...",
    config={
        "generate_auth_tests": True,
        "test_soft_deletes": True,
        "max_random_attempts": 100,
        "include_performance_tests": False,
        "test_schema": "test_scenarios"
    }
)
```

## Troubleshooting

### Common Issues

1. **Foreign Key Generation Failures**: Ensure referenced tables have test data generation configured.

2. **Regex Validation Failures**: Check that your validation patterns in field mappings are correct.

3. **Test Timeout**: Complex scenarios may need increased timeout values.

### Debug Mode

Enable debug mode to see generated SQL:

```python
generator = TestFoundryGenerator(connection_string="...", debug=True)
```

## API Reference

### TestFoundryGenerator

Main class for generating tests:

```python
class TestFoundryGenerator:
    def __init__(self, connection_string: str, config: dict = None, debug: bool = False)
    def generate_tests_for_type(self, type_class: Type, input_class: Type) -> List[str]
    def generate_all_tests(self) -> List[str]
    def analyze_schema(self) -> SchemaAnalysis
```

### Metadata Tables

- `testfoundry.tb_field_mapping` - Configure field generation
- `testfoundry.tb_entity_dependencies` - Define entity relationships
- `testfoundry.tb_group_leader` - Group related fields
- `testfoundry.tb_test_scenario` - Custom test scenarios

### Utility Functions

```sql
-- Generate random valid data
SELECT testfoundry.random_email();
SELECT testfoundry.random_phone('US');
SELECT testfoundry.random_postal_code('FR');

-- Resolve dependencies
SELECT testfoundry.get_random_fk_value('users', 'id');
```

## Examples

See the [TestFoundry examples](https://github.com/fraiseql/fraiseql/tree/main/src/fraiseql/extensions/testfoundry/examples) for complete working examples including:

- Blog application with posts and comments
- E-commerce with orders and inventory
- Multi-tenant SaaS application
- Time-series data with validations
