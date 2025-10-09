# {Example Name}

{Brief description of what this example demonstrates}

## Features

- **{Feature 1}**: {Description}
- **{Feature 2}**: {Description}
- **{Feature 3}**: {Description}

## Quick Start

### Prerequisites
- Python 3.13+
- PostgreSQL 14+
- Docker & Docker Compose (recommended)

### Using Docker (Recommended)
```bash
cd examples/{example_name}
docker-compose up -d

# Visit http://localhost:8000/graphql for GraphQL playground
# Visit http://localhost:8000/docs for API documentation
```

### Manual Setup
```bash
# Install dependencies
pip install -r requirements.txt

# Set up database
createdb {database_name}
psql -d {database_name} -f db/migrations/001_initial_schema.sql

# Run the application
uvicorn app:app --reload
```

## Architecture

{Brief explanation of the architecture patterns used}

### Patterns Demonstrated
- ✅ **{Pattern 1}**: {Description}
- ✅ **{Pattern 2}**: {Description}
- ✅ **{Pattern 3}**: {Description}

## Key Files

- **app.py** - Main FastAPI application
- **models.py** - Pydantic models and GraphQL types
- **mutations.py** - GraphQL mutations (if applicable)
- **queries.py** - GraphQL queries (if applicable)
- **db/migrations/** - Database schema migrations
- **tests/** - Test suite

## Testing

```bash
# Run tests
pytest tests/ -v

# Run with coverage
pytest tests/ --cov=. --cov-report=html
```

## API Examples

### {Example Query/Mutation Name}

```graphql
{example_graphql_query}
```

{Expected response or explanation}

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection URL | `postgresql://user:pass@localhost/db` |
| `DEBUG` | Enable debug mode | `False` |
| `HOST` | Server host | `0.0.0.0` |
| `PORT` | Server port | `8000` |

## Related Examples

- **{Related Example 1}** - {Why it's related}
- **{Related Example 2}** - {Why it's related}

## Next Steps

- Try {next example or concept}
- Explore {related documentation}
- Read more about {advanced topic}

## Troubleshooting

### Common Issues

**Issue**: {Common problem}
**Solution**: {How to fix it}

**Issue**: {Another common problem}
**Solution**: {How to fix it}

For more help, see the [main documentation](https://fraiseql.dev) or [open an issue](https://github.com/fraiseql/fraiseql/issues).
