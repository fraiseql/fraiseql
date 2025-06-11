# TestFoundry Integration for FraiseQL

TestFoundry is a sophisticated automated test generation framework for PostgreSQL databases that generates comprehensive pgTAP tests for GraphQL mutations and database operations.

## Overview

TestFoundry analyzes your database DDL and generates:
- Happy path CRUD operation tests
- Constraint violation tests (unique, foreign key, check constraints)
- Soft delete validation tests
- Authorization tests
- Custom scenario tests

The key innovation is the use of metadata tables (`testfoundry_tb_input_field_mapping` and `testfoundry_tb_fk_mapping`) to describe how to generate test data that respects your database constraints and relationships.

## Setup

### 1. Install TestFoundry Schema

```python
from fraiseql.db import FraiseQLRepository
from fraiseql.extensions.testfoundry import FoundrySetup

# Create your repository
repository = FraiseQLRepository(pool=your_pool)

# Install TestFoundry
setup = FoundrySetup(repository)
await setup.install()
```

### 2. Populate Metadata

TestFoundry needs to know about your input types and relationships. You populate this via SQL:

```sql
-- Define how to generate fields for your input types
INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
(input_type, field_name, generator_type, fk_mapping_key, random_function, required)
VALUES
('user_input', 'email', 'random', NULL, 'testfoundry_random_email', TRUE),
('user_input', 'name', 'random', NULL, NULL, TRUE);

-- Define how to resolve foreign keys
INSERT INTO testfoundry.testfoundry_tb_fk_mapping
(input_type, from_expression, select_field, random_pk_field, random_value_field, random_select_where)
VALUES
('user_id', 'tb_users', 'id', 'id', 'email', 'deleted_at IS NULL');
```

### 3. Generate Tests

```python
from fraiseql.extensions.testfoundry import FoundryGenerator, FoundryConfig

config = FoundryConfig(
    test_output_dir=Path("tests/generated"),
    generate_pytest=True
)

generator = FoundryGenerator(repository, config)

# Generate tests for an entity
tests = await generator.generate_tests_for_entity(
    entity_name="users",
    table_name="tb_users",
    input_type_name="user_input"
)

# Write to files
await generator.write_tests_to_files(tests, "users")
```

## Key Concepts

### Field Mappings

The `testfoundry_tb_input_field_mapping` table describes how to generate each field:

| Column | Purpose |
|--------|---------|
| `input_type` | Name of the composite input type (e.g., 'user_input') |
| `field_name` | Field name within the type |
| `generator_type` | How to generate: 'random', 'resolve_fk', or 'nested' |
| `fk_mapping_key` | References `testfoundry_tb_fk_mapping` for FK resolution |
| `random_function` | Custom function for random generation |

### FK Mappings

The `testfoundry_tb_fk_mapping` table describes how to resolve foreign keys:

| Column | Purpose |
|--------|---------|
| `input_type` | Logical key (e.g., 'user_id') |
| `from_expression` | SQL FROM clause with joins |
| `select_field` | Field to select |
| `random_select_where` | WHERE conditions |
| `dependency_fields` | Fields this FK depends on |

### Group Leaders

Group leaders ensure related fields are generated together for consistency. For example, country, postal_code, and city_code should all match:

```sql
-- Country is the group leader
('public_address', 'country', 'resolve_fk', 'country', NULL, TRUE, 'postal_city_country', ARRAY['city_code', 'country']),
('public_address', 'postal_code', 'resolve_fk', 'postal_code', NULL, FALSE, 'postal_city_country', NULL),
('public_address', 'city_code', 'resolve_fk', 'city_code', NULL, FALSE, 'postal_city_country', NULL),
```

## Example: Blog Application

See `example_setup.py` for a complete example setting up TestFoundry for a blog with users, posts, and comments.

## Generated Test Types

1. **Happy Create**: Valid data that should succeed
2. **Duplicate Create**: Tests unique constraint violations
3. **FK Violation Create**: Tests foreign key constraint violations
4. **Soft Delete**: Tests soft delete functionality
5. **Blocked Delete**: Tests delete restrictions

## Integration with FraiseQL

TestFoundry integrates with FraiseQL's mutation system by:
1. Analyzing FraiseQL input types
2. Generating appropriate test data
3. Testing PostgreSQL functions that back GraphQL mutations
4. Validating both success and error paths

## Advanced Features

- **Manual Scenarios**: Define custom test scenarios in `test_manual_scenarios`
- **Period Fields**: Handle date/time range validations
- **Entity Dependencies**: Define parent-child relationships for cascade testing
- **Authorization Variables**: Test role-based access control

## Troubleshooting

1. **Missing Functions**: Ensure all SQL files are properly installed
2. **Schema Issues**: Check that search_path includes the testfoundry schema
3. **FK Violations**: Verify FK mappings have correct dependency_fields
4. **Random Functions**: Implement any custom random functions needed

## Next Steps

1. Analyze your FraiseQL types
2. Create appropriate metadata records
3. Generate comprehensive test suites
4. Integrate with CI/CD pipeline
