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

# Load schema. `-f`, not `<`: schema.sql includes the shared validation helpers
# with `\ir`, which resolves relative to the SCRIPT — and psql only knows which
# script it is reading when it opens the file itself. ON_ERROR_STOP so a missing
# include is a failure rather than a line of output.
psql -v ON_ERROR_STOP=1 -d fraiseql_patterns -f schema.sql

# Test all examples (loads the pattern functions it exercises)
./test-all.sh
```

## What schema.sql provides

Every one of the eighteen pattern files loads against `schema.sql` alone. It
carries two generations of the protocol side by side, which is worth knowing
before copying a pattern out:

| | Used by | Shape |
|---|---|---|
| `mutation_response` (public) | 17 patterns | the 8-column composite |
| `app.mutation_response` | `04-relationships/update-with-cascade` | the 13-column v2 protocol type |

The v2 cascade pattern also needs the `fraiseql.*` builders — what `fraiseql setup`
installs — and read views it can read an entity from. `schema.sql` includes the
shipped helpers directly (`sql/helpers/mutation_response.sql`, `cascade.sql`) so
the example loads without a separate CLI step, and defines `v_category` /
`v_product` `WITH (security_invoker = true)`. That setting is load-bearing:
`fraiseql.cascade_entity` reads each entity through its view, so a default view —
which runs as its owner — would bypass base-table RLS and put rows in the cascade
that the caller cannot see.

Beyond `users`/`posts`/`comments`/`tags`, it creates `jobs` (async-processing),
`accounts` and `transfers` (transaction-rollback), and `categories`/`products`
(the cascade pattern).

> Each pattern file ends with a "Usage" section of live `SELECT`s, so loading a
> file also runs its examples and mutates the fixture. Reload the schema between
> experiments if you want the initial data back.

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
