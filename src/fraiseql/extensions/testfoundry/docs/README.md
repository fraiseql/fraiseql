# TestFoundry Documentation

TestFoundry is a metadata-driven test generation framework for PostgreSQL databases that automatically generates comprehensive pgTAP tests for CRUD operations, constraint validations, and complex business logic testing.

## Table of Contents

1. [Overview](./overview.md) - Introduction and key concepts
2. [Architecture](./architecture.md) - System design and components
3. [Installation Guide](./installation.md) - Setup instructions
4. [User Guide](./user-guide.md) - How to use TestFoundry
5. [API Reference](./api-reference.md) - Detailed API documentation
6. [Examples](./examples.md) - Complete working examples
7. [Troubleshooting](./troubleshooting.md) - Common issues and solutions

## Quick Start

```sql
-- 1. Install TestFoundry schema
SELECT testfoundry.install();

-- 2. Define your input type mappings
INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
(input_type, field_name, generator_type, random_function, required)
VALUES
('user_input', 'email', 'random', 'testfoundry_random_email', TRUE),
('user_input', 'name', 'random', NULL, TRUE);

-- 3. Generate tests
SELECT testfoundry_generate_happy_create('users');
```

## Key Features

- **Automatic Test Generation**: Generate comprehensive test suites without manual coding
- **Intelligent Data Generation**: Respects foreign keys, constraints, and business rules
- **Group Leader Pattern**: Ensures related fields maintain consistency
- **Extensible Architecture**: Add custom generators and scenarios
- **pgTAP Integration**: Generates standard pgTAP tests
- **FraiseQL Compatible**: Works seamlessly with FraiseQL's GraphQL-to-PostgreSQL architecture

## Getting Help

- Read the [User Guide](./user-guide.md) for detailed instructions
- Check [Examples](./examples.md) for common use cases
- See [Troubleshooting](./troubleshooting.md) for solutions to common problems
- Review the [Architecture](./architecture.md) to understand the system design
