# FraiseQL CLI Demo

This demonstrates the FraiseQL command-line interface for managing projects.

## Installation

```bash
pip install fraiseql
```

## Commands Overview

### 1. Initialize a New Project

```bash
# Create a basic project
fraiseql init myapp

# Create with a template
fraiseql init blog --template blog

# Specify database URL
fraiseql init myapp --database-url postgresql://user:pass@localhost/mydb
```

### 2. Start Development Server

```bash
# Start with defaults (localhost:8000)
fraiseql dev

# Custom host and port
fraiseql dev --host 0.0.0.0 --port 3000

# Disable auto-reload
fraiseql dev --no-reload
```

### 3. Generate Code

#### Generate GraphQL Schema
```bash
# Output to schema.graphql
fraiseql generate schema

# Custom output file
fraiseql generate schema -o my-schema.graphql
```

#### Generate Database Migration
```bash
# Generate migration for a type
fraiseql generate migration User

# Specify custom table name
fraiseql generate migration User --table users
```

#### Generate CRUD Mutations
```bash
# Generate Create/Update/Delete mutations
fraiseql generate crud User
```

### 4. Type Checking

```bash
# Run type checking
fraiseql check

# Less strict mode
fraiseql check --no-strict

# Show all output
fraiseql check --show-all
```

### 5. TestFoundry Integration

#### Install TestFoundry
```bash
fraiseql testfoundry install
```

#### Generate Tests
```bash
# Generate tests for an entity
fraiseql testfoundry generate User

# Custom output directory
fraiseql testfoundry generate User -o tests/integration
```

#### Analyze Types
```bash
# See how TestFoundry will handle your types
fraiseql testfoundry analyze User CreateUserInput
```

## Complete Example Workflow

```bash
# 1. Create a new blog project
fraiseql init myblog --template blog

# 2. Enter project directory
cd myblog

# 3. Set up virtual environment
python -m venv .venv
source .venv/bin/activate

# 4. Install dependencies
pip install -e ".[dev]"

# 5. Check types are valid
fraiseql check

# 6. Generate database migration
fraiseql generate migration User
fraiseql generate migration Post
fraiseql generate migration Comment

# 7. Run migrations
psql $DATABASE_URL -f migrations/*.sql

# 8. Generate GraphQL schema file
fraiseql generate schema

# 9. Install TestFoundry
fraiseql testfoundry install

# 10. Generate tests
fraiseql testfoundry generate User
fraiseql testfoundry generate Post

# 11. Start development server
fraiseql dev
```

## Environment Variables

The CLI respects these environment variables:

- `DATABASE_URL` - PostgreSQL connection string
- `FRAISEQL_AUTO_CAMEL_CASE` - Auto-convert to camelCase (default: true)
- `FRAISEQL_DEV_PASSWORD` - Development auth password
- `FRAISEQL_PRODUCTION` - Enable production mode

## Tips

1. **Project Structure**: The CLI assumes a standard structure:
   ```
   myproject/
   ├── src/
   │   ├── main.py
   │   ├── types/
   │   ├── mutations/
   │   └── queries/
   ├── tests/
   ├── migrations/
   └── pyproject.toml
   ```

2. **Type Registration**: Make sure your types are imported and registered in `src/main.py`

3. **Database Setup**: Always set up your PostgreSQL database before running migrations

4. **Development Workflow**: Use `fraiseql dev` with auto-reload for rapid development

5. **Testing**: Generate tests with TestFoundry after defining your types
