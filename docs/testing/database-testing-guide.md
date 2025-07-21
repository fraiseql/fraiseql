# Database Testing Guide

FraiseQL uses a **hybrid database testing approach** that automatically adapts to different environments for optimal speed and developer experience.

## 🚀 Quick Start

### Local Development (Recommended)
```bash
# 1. Install PostgreSQL locally
brew install postgresql  # macOS
sudo apt install postgresql  # Ubuntu

# 2. Create test database
createdb fraiseql_test

# 3. Set environment variable
export TEST_DATABASE_URL="postgresql://localhost/fraiseql_test"

# 4. Run tests (super fast!)
pytest tests -m database
```

### Docker Fallback
```bash
# If no local PostgreSQL, testcontainers will auto-start one
pytest tests -m database  # Slower but works everywhere
```

## 🔄 How the Hybrid Approach Works

The testing system automatically chooses the best database option:

```python
# Priority order (tests/database_conftest.py)
1. TEST_DATABASE_URL environment variable (fastest)
2. DATABASE_URL environment variable
3. Testcontainers PostgreSQL (fallback)
```

### Speed Comparison
| Method | Local Dev | CI/CD | Setup Required |
|--------|-----------|-------|----------------|
| Local PostgreSQL | ⚡ ~5s | ❌ N/A | ✅ Manual |
| PostgreSQL Service | ❌ N/A | ⚡ ~30s | ✅ Auto |
| Testcontainers | 🐌 ~60s | 🐌 ~10min | ❌ None |

## 🛠️ Setup Options

### Option 1: Local PostgreSQL (Fastest)

**macOS:**
```bash
# Install PostgreSQL
brew install postgresql
brew services start postgresql

# Create test database
createdb fraiseql_test

# Add to your shell profile (~/.zshrc, ~/.bashrc)
export TEST_DATABASE_URL="postgresql://localhost/fraiseql_test"
```

**Ubuntu/Debian:**
```bash
# Install PostgreSQL
sudo apt update
sudo apt install postgresql postgresql-contrib

# Switch to postgres user and create database
sudo -u postgres createdb fraiseql_test
sudo -u postgres createuser $(whoami) --superuser

# Add to ~/.bashrc
export TEST_DATABASE_URL="postgresql://localhost/fraiseql_test"
```

**Windows:**
```powershell
# Install PostgreSQL from https://www.postgresql.org/download/windows/
# Or use Windows Subsystem for Linux (WSL)

# Create database using pgAdmin or command line
createdb fraiseql_test

# Add environment variable
setx TEST_DATABASE_URL "postgresql://localhost/fraiseql_test"
```

### Option 2: Docker Compose (Alternative)

```yaml
# docker-compose.test.yml
version: '3.8'
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: fraiseql_test
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: fraiseql
    ports:
      - "5432:5432"
    tmpfs:
      - /var/lib/postgresql/data  # In-memory for speed
```

```bash
# Start test database
docker-compose -f docker-compose.test.yml up -d

# Set environment variable
export TEST_DATABASE_URL="postgresql://fraiseql:fraiseql@localhost/fraiseql_test"

# Run tests
pytest tests -m database

# Cleanup
docker-compose -f docker-compose.test.yml down
```

### Option 3: Testcontainers (Zero Setup)

```bash
# Just run tests - testcontainers handles everything
pytest tests -m database

# Slower but requires no setup
```

## 🧪 Test Categories

### Unit Tests (No Database)
```bash
# Fast tests that don't need a database
pytest tests -m "not database"  # ~10 seconds
```

### Database Integration Tests
```bash
# Tests that need real PostgreSQL
pytest tests -m database  # Speed depends on setup

# Specific database test files
pytest tests/test_db_comprehensive.py
pytest tests/test_db_extended.py
pytest tests/test_dual_mode_jsonb_pattern.py
```

### All Tests
```bash
# Run everything
pytest tests  # Combines unit + database tests
```

## 🔧 Configuration

### Environment Variables
```bash
# Primary: Use specific test database
export TEST_DATABASE_URL="postgresql://localhost/fraiseql_test"

# Fallback: Use any PostgreSQL database
export DATABASE_URL="postgresql://user:pass@host/db"

# Skip database tests entirely
pytest tests --no-db
```

### Test Isolation

All database tests use **transaction-based isolation**:

```python
@pytest_asyncio.fixture
async def db_connection(db_pool):
    """Each test runs in its own transaction that gets rolled back."""
    async with db_pool.connection() as conn:
        await conn.execute("BEGIN")  # Start transaction
        yield conn
        await conn.execute("ROLLBACK")  # Always rollback
```

**Benefits:**
- ✅ Each test starts with clean database state
- ✅ Tests can run in parallel
- ✅ No cleanup required
- ✅ Tests are completely isolated

## 🚀 CI/CD Configuration

### GitHub Actions (Automatic)
```yaml
# .github/workflows/ci.yml already configured with:
services:
  postgres:
    image: postgres:16-alpine
    env:
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: fraiseql_test
    options: >-
      --health-cmd pg_isready
      --health-interval 10s
```

The CI automatically sets `TEST_DATABASE_URL` to use the service.

### Other CI Systems

**GitLab CI:**
```yaml
services:
  - postgres:16-alpine

variables:
  POSTGRES_DB: fraiseql_test
  POSTGRES_PASSWORD: postgres
  TEST_DATABASE_URL: postgresql://postgres:postgres@postgres/fraiseql_test
```

**CircleCI:**
```yaml
version: 2.1
jobs:
  test:
    docker:
      - image: python:3.13
      - image: postgres:16-alpine
        environment:
          POSTGRES_DB: fraiseql_test
          POSTGRES_PASSWORD: postgres
    environment:
      TEST_DATABASE_URL: postgresql://postgres:postgres@localhost/fraiseql_test
```

## 🔍 Troubleshooting

### "Docker not available" Error
```bash
# Check Docker installation
docker --version

# Start Docker service
sudo systemctl start docker  # Linux
# or restart Docker Desktop on macOS/Windows
```

### "Connection refused" Error
```bash
# Check if PostgreSQL is running
pg_isready -h localhost

# Start PostgreSQL service
brew services start postgresql  # macOS
sudo systemctl start postgresql  # Linux

# Check connection
psql -h localhost -d fraiseql_test -c "SELECT 1"
```

### Slow Tests
```bash
# Check which method is being used
pytest tests -m database -v | grep "container\|local\|service"

# Force local database
export TEST_DATABASE_URL="postgresql://localhost/fraiseql_test"
pytest tests -m database
```

### Permission Errors
```bash
# Create user if needed
sudo -u postgres createuser $(whoami) --superuser

# Fix database ownership
sudo -u postgres psql -c "ALTER DATABASE fraiseql_test OWNER TO $(whoami)"
```

## 📊 Performance Tips

### 1. Use Local PostgreSQL
- **10-20x faster** than testcontainers
- Set `TEST_DATABASE_URL` environment variable

### 2. Parallel Testing
```bash
# Run tests in parallel (requires local DB)
pytest tests -m database -n auto
```

### 3. Test Subsets
```bash
# Run specific test files
pytest tests/test_db_comprehensive.py

# Skip slow tests during development
pytest tests -m "database and not slow"
```

### 4. In-Memory PostgreSQL (Advanced)
```bash
# Use tmpfs for ultimate speed (Linux)
export TEST_DATABASE_URL="postgresql://localhost/fraiseql_test?options=-c%20shared_buffers=256MB%20-c%20fsync=off"
```

## 🏗️ Architecture Details

### Database Fixtures Hierarchy
```
session: postgres_container (testcontainers)
  └── session: postgres_url (connection string)
    └── session: db_pool (connection pool)
      └── function: db_connection (isolated transaction)
        └── function: db_cursor (simple operations)
```

### Test Patterns
```python
# Standard database test
@pytest.mark.database
async def test_something(db_connection):
    await db_connection.execute("INSERT INTO users ...")
    # Automatic rollback after test

# Test that needs committed data
async def test_complex(db_connection_committed):
    await db_connection.execute("INSERT ...")
    await db_connection.commit()
    # Still cleaned up with test schema
```

## 🔗 See Also

- [CLAUDE.md](../CLAUDE.md) - Project development guide
- [Contributing Guide](../CONTRIBUTING.md) - How to contribute
- [API Documentation](../api/) - FraiseQL API reference
