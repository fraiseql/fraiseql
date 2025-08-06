# Installation

Get FraiseQL up and running in your development environment.

## Prerequisites

Before installing FraiseQL, make sure you have:

### Python 3.10+
FraiseQL uses modern Python type hints and requires Python 3.10 or later.

```bash
python --version  # Should show 3.10 or higher
```

### PostgreSQL 13+
FraiseQL leverages PostgreSQL's JSONB capabilities and advanced features.

```bash
psql --version  # Should show 13 or higher
```

!!! tip "PostgreSQL Installation"
    - **macOS**: `brew install postgresql@14`
    - **Ubuntu/Debian**: `apt-get install postgresql-14`
    - **Windows**: Download from [postgresql.org](https://www.postgresql.org/download/windows/)

## Install FraiseQL

### Using pip (Recommended)

```bash
pip install fraiseql
```

### Using Poetry

```bash
poetry add fraiseql
```

### Using pipenv

```bash
pipenv install fraiseql
```

### Development Installation

For contributing or testing latest features:

```bash
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql
pip install -e ".[dev]"
```

## Verify Installation

Check that FraiseQL is installed correctly:

```bash
python -c "import fraiseql; print(fraiseql.__version__)"
```

## Database Setup

### 1. Create a Database

```bash
createdb my_app_db
```

Or using psql:

```sql
CREATE DATABASE my_app_db;
```

### 2. Set Database URL

FraiseQL uses standard PostgreSQL connection strings:

```bash
# Environment variable (recommended)
export DATABASE_URL="postgresql://username:password@localhost:5432/my_app_db"

# Or in .env file
DATABASE_URL=postgresql://username:password@localhost:5432/my_app_db
```

### 3. Test Connection

Create a simple test script:

```python
# test_connection.py
from fraiseql import FraiseQL

app = FraiseQL(
    database_url="postgresql://localhost/my_app_db"
)

if __name__ == "__main__":
    print("✅ FraiseQL connected successfully!")
```

Run it:

```bash
python test_connection.py
```

## Optional Dependencies

### For Production Deployment

```bash
pip install fraiseql[production]
# Includes: uvicorn, gunicorn, python-multipart
```

### For Development

```bash
pip install fraiseql[dev]
# Includes: pytest, black, mypy, pre-commit
```

### For FastAPI Integration

```bash
pip install fraiseql[fastapi]
# Includes: fastapi, uvicorn
```

## IDE Setup

### VS Code

Install recommended extensions for the best experience:

```json
{
  "recommendations": [
    "ms-python.python",
    "ms-python.vscode-pylance",
    "GraphQL.vscode-graphql",
    "mtxr.sqltools",
    "mtxr.sqltools-driver-pg"
  ]
}
```

### PyCharm

1. Enable Python type checking
2. Install GraphQL plugin
3. Configure Database tools for PostgreSQL

## Environment Configuration

Create a `.env` file in your project root:

```bash
# Database
DATABASE_URL=postgresql://localhost/my_app_db
DATABASE_POOL_SIZE=20
DATABASE_MAX_OVERFLOW=40

# Environment
ENVIRONMENT=development  # or production

# GraphQL
GRAPHQL_PATH=/graphql
GRAPHQL_PLAYGROUND_ENABLED=true  # Disable in production

# Security (production)
SECRET_KEY=your-secret-key-here
CORS_ORIGINS=https://yourdomain.com
```

## Docker Setup (Optional)

If you prefer using Docker for PostgreSQL:

```yaml
# docker-compose.yml
version: '3.8'

services:
  postgres:
    image: postgres:14-alpine
    environment:
      POSTGRES_DB: my_app_db
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: secret
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data

volumes:
  postgres_data:
```

Start PostgreSQL:

```bash
docker-compose up -d
```

Your connection string would be:
```bash
DATABASE_URL=postgresql://fraiseql:secret@localhost:5432/my_app_db
```

## Troubleshooting

### Common Issues

#### ImportError: No module named 'fraiseql'
- **Solution**: Ensure you're using the correct Python environment
- Check: `which python` and `pip list | grep fraiseql`

#### psycopg2 Installation Fails
- **macOS**: `brew install postgresql` before installing FraiseQL
- **Ubuntu**: `apt-get install libpq-dev python3-dev`
- **Alternative**: Use `psycopg2-binary` for development

#### Connection Refused to PostgreSQL
- **Check if PostgreSQL is running**: `pg_isready`
- **Check connection details**: `psql -U username -d database -h localhost`
- **Check PostgreSQL logs**: `tail -f /var/log/postgresql/*.log`

#### JSONB Functions Not Available
- **Ensure PostgreSQL 13+**: Older versions have limited JSONB support
- **Check extensions**: `CREATE EXTENSION IF NOT EXISTS "uuid-ossp";`

### Getting Help

If you encounter issues:

1. Check the [FAQ section](../faq.md)
2. Search [existing issues](https://github.com/fraiseql/fraiseql/issues)
3. Ask in [discussions](https://github.com/fraiseql/fraiseql/discussions)
4. Report a [new issue](https://github.com/fraiseql/fraiseql/issues/new)

## Next Steps

Now that FraiseQL is installed, you're ready to:

- [**Follow the Quickstart**](quickstart.md) - Build your first API in 5 minutes
- [**Explore the Playground**](graphql-playground.md) - Test queries interactively
- [**Build Your First API**](first-api.md) - Step-by-step guide