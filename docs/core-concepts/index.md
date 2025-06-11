# Core Concepts

Understanding FraiseQL's core concepts will help you build efficient and maintainable GraphQL APIs.

## The FraiseQL Philosophy

FraiseQL takes a fundamentally different approach to GraphQL:

1. **Database-First**: Leverage PostgreSQL's power instead of fighting it
2. **View-Based Resolution**: Each resolver queries exactly one view
3. **JSON All the Way**: Data flows as JSON from database to client
4. **Composition Over Computation**: Complex relationships are pre-computed in views

## Key Concepts

### [Architecture Overview](./architecture.md)
Understand how FraiseQL components work together to serve GraphQL queries efficiently.

### [Type System](./type-system.md)
Learn about FraiseQL's type decorators, field definitions, and how Python types map to GraphQL.

### [Database Views](./database-views.md)
Master the art of creating efficient database views that power your GraphQL resolvers.

### [Query Translation](./query-translation.md)
See how GraphQL queries are translated into SQL that extracts only requested fields.

## Design Principles

### Single Responsibility Views

Each GraphQL type should have one corresponding database view. This view is responsible for:
- Aggregating data from relevant tables
- Composing relationships from other views
- Returning a complete JSON representation

### Efficient by Default

By moving complexity to the database layer:
- No N+1 query problems
- No need for dataloaders
- Predictable performance characteristics
- Database optimizer handles query planning

### Type Safety Throughout

From Python type hints to GraphQL schema to database queries, FraiseQL maintains type safety:
- Python: Full type hints with runtime validation
- GraphQL: Strongly typed schema generation
- PostgreSQL: JSONB with schema validation

## Next Steps

Ready to dive deeper? Start with:
- [Architecture Overview](./architecture.md) for the big picture
- [Type System](./type-system.md) for schema definition
- [Database Views](./database-views.md) for the data layer
