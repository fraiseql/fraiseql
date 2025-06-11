# TestFoundry Overview

## What is TestFoundry?

TestFoundry is a metadata-driven test generation framework for PostgreSQL databases. It automatically generates comprehensive pgTAP tests for:

- CRUD operations (Create, Read, Update, Delete)
- Constraint violations (unique, foreign key, check constraints)
- Soft delete operations
- Authorization and access control
- Custom business logic scenarios

## Why TestFoundry?

### The Problem

Writing comprehensive database tests is:
- **Time-consuming**: Each entity needs multiple test scenarios
- **Error-prone**: Easy to miss edge cases or constraints
- **Repetitive**: Most CRUD tests follow similar patterns
- **Inconsistent**: Different developers write tests differently

### The Solution

TestFoundry solves these problems by:
- **Automating test generation** based on your database schema
- **Ensuring complete coverage** of all constraints and relationships
- **Maintaining consistency** across all generated tests
- **Respecting data integrity** through intelligent random data generation

## Key Concepts

### 1. Metadata-Driven Approach

TestFoundry uses metadata tables to understand your database:

```sql
-- Describe how to generate input fields
testfoundry_tb_input_field_mapping

-- Describe how to resolve foreign keys
testfoundry_tb_fk_mapping

-- Define entity relationships
testfoundry_tb_entity_dependents
```

### 2. Intelligent Random Data Generation

TestFoundry generates realistic test data that:
- Respects foreign key constraints
- Maintains referential integrity
- Follows business rules
- Avoids impossible combinations

### 3. Group Leader Pattern

The revolutionary "Group Leader" concept ensures related fields are generated together:

```sql
-- Country is the group leader for postal_code and city_code
-- All three fields will be generated from the same source record
('public_address', 'country', 'resolve_fk', 'country', NULL, TRUE, 'postal_city_country', ARRAY['postal_code', 'city_code'])
```

This prevents invalid combinations like a French postal code with a German city.

### 4. Test Scenario Types

TestFoundry generates multiple test scenarios for each entity:

| Scenario | Purpose |
|----------|---------|
| Happy Create | Valid data that should succeed |
| Duplicate Create | Test unique constraint violations |
| FK Violation | Test foreign key constraint violations |
| Constraint Violation | Test check constraints |
| Soft Delete | Test soft delete functionality |
| Blocked Delete | Test delete restrictions |
| Authorization | Test access control |

## How It Works

1. **Analyze Schema**: TestFoundry examines your database schema
2. **Read Metadata**: Consults metadata tables for generation rules
3. **Generate Data**: Creates realistic test data respecting all constraints
4. **Build Tests**: Generates pgTAP test functions
5. **Execute Tests**: Run tests to validate your database operations

## Integration with FraiseQL

TestFoundry integrates seamlessly with FraiseQL by:
- Understanding FraiseQL's JSONB-based data model
- Testing PostgreSQL functions that back GraphQL mutations
- Validating the entire mutation pipeline
- Ensuring GraphQL schema and database constraints align

## Benefits

### For Developers
- **Save Time**: Generate tests in seconds, not hours
- **Increase Coverage**: Never miss an edge case
- **Maintain Consistency**: All tests follow the same patterns
- **Focus on Logic**: Spend time on business logic, not test scaffolding

### For Teams
- **Standardization**: Everyone's tests follow the same structure
- **Documentation**: Tests serve as executable documentation
- **Regression Prevention**: Catch breaking changes automatically
- **Onboarding**: New developers understand the data model quickly

### For Projects
- **Quality Assurance**: Comprehensive test coverage
- **Maintainability**: Easy to update tests when schema changes
- **Confidence**: Know your database operations work correctly
- **Performance**: Identify bottlenecks with generated load tests

## Example

Here's a simple example of TestFoundry in action:

```sql
-- 1. Define input field mappings
INSERT INTO testfoundry_tb_input_field_mapping
(input_type, field_name, generator_type, random_function, required)
VALUES
('user_input', 'email', 'random', 'testfoundry_random_email', TRUE),
('user_input', 'name', 'random', NULL, TRUE);

-- 2. Generate a happy path test
SELECT testfoundry_generate_happy_create('users');

-- Result: A complete pgTAP test function that:
-- - Generates valid random user data
-- - Calls your create_user mutation
-- - Validates the result
-- - Checks all constraints
```

## Next Steps

- Read the [Architecture](./architecture.md) guide to understand the system design
- Follow the [Installation Guide](./installation.md) to set up TestFoundry
- Check the [User Guide](./user-guide.md) for detailed usage instructions
- Explore [Examples](./examples.md) for real-world scenarios
