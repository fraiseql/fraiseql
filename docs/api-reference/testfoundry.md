# TestFoundry API Reference

TestFoundry is a test generation tool for FraiseQL applications that creates comprehensive pgTAP tests for your database operations.

## Overview

TestFoundry automatically generates tests for:
- Valid operations (happy path)
- Constraint violations (unique, foreign key, check)
- Edge cases and boundary conditions
- Authorization rules

## Basic Usage

```python
from fraiseql.extensions.testfoundry import FoundryGenerator

# Generate pgTAP tests for your mutations
generator = FoundryGenerator(repository)
tests = await generator.generate_tests_for_entity(
    entity_name="users",
    table_name="tb_users",
    input_type_name="user_input"
)
```

## CLI Commands

### Generate Tests

```bash
# Generate tests for a specific entity
fraiseql testfoundry generate User

# Generate tests for all entities
fraiseql testfoundry generate --all
```

### Run Tests

```bash
# Run all generated tests
fraiseql testfoundry run

# Run tests for specific entity
fraiseql testfoundry run --entity User
```

## Configuration

TestFoundry can be configured through your `pyproject.toml` file:

```toml
[tool.fraiseql.testfoundry]
output_directory = "tests/generated"
test_framework = "pgtap"
include_performance_tests = true
```

## API Reference

### FoundryGenerator

The main class for generating tests.

#### Methods

- `generate_tests_for_entity(entity_name, table_name, input_type_name)`: Generate tests for a specific entity
- `generate_all_tests()`: Generate tests for all registered entities
- `validate_entity(entity_name)`: Validate entity configuration

### Test Types

TestFoundry generates several types of tests:

1. **CRUD Operation Tests**: Basic create, read, update, delete operations
2. **Constraint Tests**: Database constraint validation
3. **Business Logic Tests**: Custom validation rules
4. **Performance Tests**: Query performance validation
5. **Authorization Tests**: Permission and role-based access tests

## Example Output

Generated tests follow pgTAP conventions:

```sql
-- Test user creation
SELECT plan(3);

-- Test valid user creation
SELECT ok(
    (SELECT create_user('{"email": "test@example.com", "name": "Test User"}'::jsonb)).success,
    'User creation should succeed with valid data'
);

-- Test duplicate email constraint
SELECT throws_ok(
    $$SELECT create_user('{"email": "test@example.com", "name": "Duplicate"}'::jsonb)$$,
    'unique_violation',
    'User creation should fail with duplicate email'
);

SELECT finish();
```

## Best Practices

1. **Run tests regularly**: Include TestFoundry tests in your CI/CD pipeline
2. **Review generated tests**: Always review generated tests before committing
3. **Customize as needed**: Generated tests are starting points - customize for your needs
4. **Keep tests updated**: Regenerate tests when your schema changes

## Troubleshooting

### Common Issues

1. **Missing entity configuration**: Ensure entities are properly registered
2. **Database connection errors**: Verify database connection settings
3. **Permission issues**: Ensure test database has proper permissions

### Debug Mode

Enable debug mode for detailed output:

```bash
fraiseql testfoundry generate --debug User
```

## Integration with CI

Example GitHub Actions workflow:

```yaml
- name: Run TestFoundry tests
  run: |
    fraiseql testfoundry run
    pg_prove tests/generated/*.sql
```