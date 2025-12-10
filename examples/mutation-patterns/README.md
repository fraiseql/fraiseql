# FraiseQL Mutation Patterns

Real-world mutation examples you can copy and adapt.

> 📦 **v1.8.1 Release**: 10 core patterns available. Additional examples (relationships, calculated fields, async) coming in future releases.

## Quick Index

| Pattern | File | Use Case |
|---------|------|----------|
| **Basic CRUD** |
| Create | [01-basic-crud/create-user.sql](01-basic-crud/create-user.sql) | Simple INSERT |
| Update | [01-basic-crud/update-user.sql](01-basic-crud/update-user.sql) | Simple UPDATE |
| Delete | [01-basic-crud/delete-user.sql](01-basic-crud/delete-user.sql) | Simple DELETE |
| **Validation** |
| Simple | [02-validation/simple-validation.sql](02-validation/simple-validation.sql) | Single error (Pattern 1) |
| Multiple Fields | [02-validation/multiple-field-validation.sql](02-validation/multiple-field-validation.sql) | Multiple errors (Pattern 2) |
| **Business Logic** |
| Conditional Update | [03-business-logic/conditional-update.sql](03-business-logic/conditional-update.sql) | Optimistic locking |
| State Machine | [03-business-logic/state-machine.sql](03-business-logic/state-machine.sql) | Valid transitions |
| **Error Handling** |
| Not Found | [05-error-handling/not-found.sql](05-error-handling/not-found.sql) | 404 errors |
| Duplicate | [05-error-handling/conflict-duplicate.sql](05-error-handling/conflict-duplicate.sql) | Unique violations |
| **Advanced** |
| Bulk Operations | [06-advanced/bulk-operations.sql](06-advanced/bulk-operations.sql) | Array inputs |

## Coming Soon

Additional patterns planned for future releases:

- **Validation**: Custom business rules
- **Business Logic**: Calculated fields
- **Relationships**: CREATE with children, UPDATE CASCADE, DELETE CASCADE
- **Error Handling**: Permission/authorization patterns
- **Advanced**: Transaction rollback, async job processing

## Setup

```bash
# Create test database
createdb fraiseql_patterns

# Load schema
psql fraiseql_patterns < schema.sql

# Test all examples
./test-all.sh
```

## Usage

Each example is standalone and copy-paste ready:

1. Read the example SQL file
2. Adapt variable names and table names
3. Copy into your project
4. Test with psql

## Contributing

Have a useful pattern? Submit a PR with:
- SQL file with comments
- Test case showing usage
- README section explaining the pattern
