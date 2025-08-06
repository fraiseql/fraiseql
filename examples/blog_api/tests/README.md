# Blog API Test Suite

Comprehensive test suite for the Blog API example, including unit tests, integration tests, and end-to-end GraphQL tests.

## Test Structure

- **test_repository.py**: Unit tests for the BlogRepository CQRS operations
- **test_mutations.py**: Integration tests for GraphQL mutations
- **test_queries.py**: Integration tests for GraphQL queries
- **test_graphql_e2e.py**: End-to-end tests using real GraphQL requests
- **conftest.py**: Shared fixtures and test configuration

## Setup

### 1. Create Test Database

```bash
# Run the setup script
./setup_test_db.sh

# Or manually:
createdb blog_test
psql -d blog_test -f ../db/migrations/001_initial_schema.sql
psql -d blog_test -f ../db/migrations/002_functions.sql
psql -d blog_test -f ../db/migrations/003_views.sql
```

### 2. Install Test Dependencies

```bash
pip install pytest pytest-asyncio httpx
```

### 3. Set Environment Variable

```bash
export TEST_DATABASE_URL=postgresql://localhost/blog_test
```

## Running Tests

### Run All Tests

```bash
# From the blog_api directory
python -m pytest tests/

# With verbose output
python -m pytest tests/ -v

# With coverage
python -m pytest tests/ --cov=. --cov-report=html
```

### Run Specific Test Files

```bash
# Unit tests only
python -m pytest tests/test_repository.py

# Integration tests only
python -m pytest tests/test_mutations.py tests/test_queries.py

# End-to-end tests only
python -m pytest tests/test_graphql_e2e.py
```

### Run Specific Test

```bash
python -m pytest tests/test_repository.py::TestBlogRepository::test_create_user
```

## Test Coverage

The test suite covers:

### Repository Tests
- User CRUD operations
- Post CRUD operations with slug generation
- Comment creation with nesting
- Filtering, ordering, and pagination
- View count incrementation

### Mutation Tests
- User creation with validation
- Authenticated post creation
- Post updates with ownership checks
- Admin permissions
- Comment creation with replies
- Error handling

### Query Tests
- User and post queries
- Authenticated queries (me)
- Complex filtering and pagination
- Field resolution for relationships
- View count tracking

### End-to-End Tests
- Complete GraphQL request/response cycle
- Authentication flow
- Complex nested queries
- Error responses
- Full mutation → query workflows

## Fixtures

Key fixtures provided by conftest.py:

- `clean_db`: Ensures clean database state for each test
- `blog_repo`: BlogRepository instance
- `test_user` / `admin_user`: Pre-created users
- `create_test_post`: Factory for creating posts
- `create_test_comment`: Factory for creating comments
- `async_client`: HTTPX client for GraphQL requests

## Authentication

Tests simulate authentication using mock tokens. In production, these would be real JWT tokens from Auth0 or another provider.

## Tips

1. Tests are isolated - each test runs with a clean database
2. Use factory fixtures to create test data
3. Tests use transactions that are rolled back
4. GraphQL tests use real HTTP requests for true e2e testing
