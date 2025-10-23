# Todo API

A minimal FraiseQL GraphQL API project.

## Getting Started

1. Set up a Python virtual environment:
   ```bash
   python -m venv .venv
   source .venv/bin/activate  # On Windows: .venv\\Scripts\\activate
   ```

2. Install dependencies:
   ```bash
   pip install -e ".[dev]"
   ```

3. Set up your PostgreSQL database and update the DATABASE_URL in `.env`

4. Run migrations (if any):
   ```bash
   fraiseql migrate
   ```

5. Start the development server:
   ```bash
   python -m src.main
   ```

Your GraphQL API will be available at http://localhost:8000/graphql

## Project Structure

- `src/` - Application source code
  - `main.py` - GraphQL schema and resolvers
  - `types/` - Type definitions
  - `queries/` - Custom query logic
  - `mutations/` - Mutation handlers
- `tests/` - Test files
- `migrations/` - Database migrations

## Learn More

- [FraiseQL Documentation](https://fraiseql.readthedocs.io)
- [GraphQL](https://graphql.org)
