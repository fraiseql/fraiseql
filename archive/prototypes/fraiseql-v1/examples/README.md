# FraiseQL Examples

This directory contains working examples demonstrating FraiseQL patterns and best practices.

## Available Examples

### 1. Quickstart (`quickstart/`)
**Status**: 🚧 Planned

A minimal 50-line example showing:
- Basic @type and @query decorators
- Single table setup
- Hello world GraphQL query

**Use case**: Learning FraiseQL basics in 5 minutes

### 2. Blog (`blog/`)
**Status**: 🚧 Planned

A complete blog implementation demonstrating:
- CQRS pattern (tb_user, tb_post → tv_user, tv_post)
- Explicit sync strategy
- Relationships (User has many Posts)
- Mutations (create, update, delete)
- Pagination

**Use case**: Understanding core CQRS patterns

### 3. E-Commerce (`ecommerce/`)
**Status**: 🚧 Planned

Product catalog with:
- Complex relationships (Products, Categories, Orders)
- Search and filtering
- Performance optimization
- Batch operations
- Transaction handling

**Use case**: Production-grade patterns

## Running Examples

Each example directory contains:
- `README.md` - Setup and explanation
- `schema.sql` - Database schema
- `app.py` - FastAPI application
- `requirements.txt` - Dependencies

To run an example:
```bash
cd examples/blog
pip install -r requirements.txt
python app.py
```

## Example Structure Template

Each example follows this structure:
```
example-name/
├── README.md           # Setup guide & explanation
├── schema.sql          # Database schema (tb_*, tv_*, fn_sync_*)
├── app.py             # FastAPI application
├── types.py           # GraphQL types
├── queries.py         # Query resolvers
├── mutations.py       # Mutation resolvers
├── requirements.txt   # Python dependencies
└── test_queries.graphql  # Example GraphQL queries
```
