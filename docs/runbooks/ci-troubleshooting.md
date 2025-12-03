# CI/CD Troubleshooting Runbook

This runbook provides step-by-step troubleshooting procedures for common CI/CD issues in FraiseQL.

## Quick Reference

| Issue | Section | Urgency |
|-------|---------|---------|
| Quality Gate blocked | [Quality Gate Issues](#quality-gate-issues) | 🔴 High |
| PostgreSQL tests failing | [PostgreSQL Issues](#postgresql-connection-issues) | 🔴 High |
| Vault timeout in CI | [Vault Issues](#vault-kms-issues) | 🟡 Medium |
| Enterprise tests failing | [Enterprise CI Issues](#enterprise-ci-issues) | 🟢 Low |
| Performance regression | [Performance Issues](#performance-regression-detected) | 🟡 Medium |
| Test collection slow | [Test Collection Issues](#test-collection-slow) | 🟢 Low |

---

## Main CI Pipeline Issues

### Quality Gate Blocked

**Symptom**: Pull request shows "Quality Gate" check failing

**Diagnostic Steps**:
```bash
# 1. Check which job failed
gh pr checks <PR_NUMBER>

# 2. View specific job logs
gh run view <RUN_ID> --log-failed
```

**Common Causes & Solutions**:

#### Unit Tests Failed
```bash
# Check test output
gh run view <RUN_ID> --job=<JOB_ID>

# Common fixes:
# - Rebase on latest dev branch
# - Fix type errors shown in logs
# - Update test fixtures if API changed
```

#### Lint Errors
```bash
# Run locally to reproduce
uv run ruff check src/ tests/
uv run mypy src/

# Auto-fix most issues
uv run ruff check --fix src/ tests/
```

#### Security Scan Failed
```bash
# Check security scan results
gh run view <RUN_ID> --log | grep -A 10 "Security"

# Common issues:
# - Outdated dependencies with CVEs
# - Hardcoded secrets detected
# - Insecure code patterns

# Fix:
uv pip install --upgrade <vulnerable-package>
# OR
# Add exception in pyproject.toml if false positive
```

#### Integration Tests Failed
```bash
# Check integration test logs
gh run view <RUN_ID> --log | grep -A 50 "integration-postgres"

# Common causes:
# - Database connection issues
# - Schema isolation problems
# - Fixture deadlocks
# - Test timeout

# Reproduce locally:
pytest tests/ -m 'requires_postgres' -v --tb=short
```

---

## PostgreSQL Connection Issues

### Tests Can't Connect to Database

**Symptom**: `psycopg.OperationalError: could not connect to server`

**Diagnostic Steps**:
```bash
# 1. Check if PostgreSQL service started
# In CI logs, look for:
grep "PostgreSQL" <ci-logs> | grep -i "ready\|health"

# 2. Check connection parameters
grep "DATABASE_URL\|DB_HOST\|DB_PORT" <ci-logs>
```

**Solutions**:

#### CI Environment
```yaml
# Verify PostgreSQL service in workflow YAML
services:
  postgres:
    image: pgvector/pgvector:pg16
    env:
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: fraiseql
      POSTGRES_DB: fraiseql_test
    options: >-
      --health-cmd "pg_isready -U fraiseql"
      --health-interval 10s
      --health-timeout 5s
      --health-retries 5
    ports:
      - 5432:5432
```

#### Local Development
```bash
# Check PostgreSQL status
pg_isready -h localhost -p 5432 -U fraiseql

# If not running, start it
./scripts/development/start-postgres-daemon.sh

# Reset test database
dropdb fraiseql_test 2>/dev/null || true
createdb fraiseql_test
```

### Schema Isolation Issues

**Symptom**: Tests pass individually but fail when run together

**Cause**: Schema leakage between test classes

**Diagnostic Steps**:
```bash
# Run tests with verbose schema info
pytest tests/ -v --log-cli-level=DEBUG | grep "schema\|CREATE SCHEMA"

# Check for schema prefixes in SQL
grep -r "public\." tests/
```

**Solution**:
```python
# Ensure tests use search_path, not hardcoded schemas
# ❌ WRONG
await conn.execute("CREATE TABLE public.users (...)")

# ✅ CORRECT
await conn.execute(f"SET search_path TO {test_schema}, public")
await conn.execute("CREATE TABLE users (...)")
```

---

## Enterprise CI Issues

### Vault KMS Issues

#### Vault Not Starting

**Symptom**: Enterprise tests fail with "Vault not ready after 10 attempts"

**Diagnostic Steps**:
```bash
# 1. Check Vault container logs
gh run view <RUN_ID> --log | grep -A 20 "Vault"

# 2. Look for exponential backoff attempts
grep "Attempt [0-9]" <logs>

# 3. Check if 10 retries were attempted
grep "waiting.*s before retry" <logs>
```

**Common Causes**:

1. **Insufficient Resources**
   - GitHub Actions runner out of memory/CPU
   - Multiple containers competing for resources
   - Solution: Increase runner size or reduce parallel jobs

2. **Docker Network Issues**
   - Container networking not ready
   - Port conflicts
   - Solution: Check port mappings in workflow YAML

3. **Vault Configuration Issues**
   - Wrong environment variables
   - Missing VAULT_DEV_ROOT_TOKEN_ID
   - Solution: Verify Vault service config in workflow

**Manual Verification**:
```bash
# Test Vault startup locally
docker run -d --name vault -p 8200:8200 \
  -e VAULT_DEV_ROOT_TOKEN_ID=root \
  hashicorp/vault:latest

# Wait and check health
for i in {1..10}; do
  curl -sf http://localhost:8200/v1/sys/health && echo "✅ Ready" && break
  echo "Attempt $i failed, waiting..."
  sleep 2
done
```

#### Vault Tests Failing

**Symptom**: Vault starts but tests fail

**Diagnostic Steps**:
```bash
# Check Vault authentication
grep "VAULT_TOKEN\|VAULT_ADDR" <ci-logs>

# Check KMS operations
grep -A 10 "kms\|encrypt\|decrypt" <test-logs>
```

**Common Causes**:
1. **Wrong Token**: Mismatch between `VAULT_DEV_ROOT_TOKEN_ID` and `VAULT_TOKEN`
2. **Wrong URL**: `VAULT_ADDR` not pointing to correct host/port
3. **KMS Not Enabled**: Vault transit engine not enabled

**Solution**:
```bash
# Verify Vault configuration in workflow
env:
  VAULT_ADDR: http://localhost:8200
  VAULT_TOKEN: root  # Must match VAULT_DEV_ROOT_TOKEN_ID

# Check transit engine enabled in test setup
vault secrets enable transit
vault write -f transit/keys/fraiseql
```

---

### Auth0 Tests Failing

**Symptom**: Auth0 integration tests fail in enterprise workflow

**Diagnostic Steps**:
```bash
# Check test output
pytest -m 'requires_auth0' -v --tb=short

# Look for JWT/token errors
grep -i "jwt\|token\|auth" <test-logs>
```

**Common Causes**:
1. **Mock Configuration**: Auth0 mocks not properly set up
2. **Token Validation**: JWT validation logic incorrect
3. **Network Issues**: Timeout connecting to Auth0 (if using real Auth0)

**Solution**:
```python
# Ensure tests use mocks
@pytest.fixture
def auth0_mock():
    """Mock Auth0 authentication."""
    # Mock implementation
    pass

# For real Auth0 tests, check credentials
# AUTH0_DOMAIN, AUTH0_CLIENT_ID, AUTH0_CLIENT_SECRET must be set
```

---

## Performance Issues

### Performance Regression Detected

**Symptom**: Quality gate fails with "Performance regression detected"

**Diagnostic Steps**:
```bash
# 1. Check performance test results
gh run view <RUN_ID> --log | grep -A 20 "performance"

# 2. Compare with baseline
# Look for "current" vs "baseline" metrics in logs

# 3. Identify slow tests
pytest tests/ --durations=10
```

**Common Causes**:

1. **New Slow Tests**
   - Recently added tests with inefficient queries
   - Missing indexes on test tables
   - Solution: Optimize queries or add indexes

2. **Database Query Changes**
   - ORM changes generating inefficient SQL
   - N+1 query problems
   - Solution: Use EXPLAIN ANALYZE on slow queries

3. **External Service Delays**
   - Timeout issues with services
   - Network latency
   - Solution: Add timeouts or use mocks

**Remediation**:
```bash
# Identify slow tests
pytest tests/ --durations=20 -v

# Profile specific test
pytest tests/path/to/slow_test.py --profile

# Check database queries
FRAISEQL_LOG_LEVEL=DEBUG pytest tests/integration/...
```

### Test Collection Slow

**Symptom**: CI spends 30+ seconds collecting tests

**Diagnostic Steps**:
```bash
# Time test collection
time pytest --collect-only tests/ -q
```

**Solution**:
```toml
# In pyproject.toml, ensure norecursedirs is set
[tool.pytest.ini_options]
norecursedirs = [
    ".git",
    ".tox",
    "dist",
    "build",
    "*.egg",
    ".eggs",
    "node_modules",
    ".venv",
    "venv"
]
```

---

## Local Development Issues

### Tests Can't Connect to Database

**Quick Fix**:
```bash
# Check PostgreSQL status
pg_isready -h localhost -p 5432

# Start PostgreSQL if not running
brew services start postgresql@16  # macOS
sudo systemctl start postgresql    # Linux

# Create test database
createdb fraiseql_test

# Run database migrations
./scripts/development/test-db-setup.sh
```

### Environment Variables Not Set

**Symptom**: Tests fail with "FRAISEQL_ENVIRONMENT not set"

**Solution**:
```bash
# Create .env file
cat > .env <<EOF
DATABASE_URL=postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test
TEST_DATABASE_URL=postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test
FRAISEQL_ENVIRONMENT=testing
FRAISEQL_AUTO_INSTALL=false
FRAISEQL_LOG_LEVEL=INFO
EOF

# Load environment
export $(cat .env | xargs)
```

### Markers Not Working

**Symptom**: `pytest -m 'requires_postgres'` doesn't filter correctly

**Diagnostic Steps**:
```bash
# List all available markers
pytest --markers

# Check marker application
pytest --collect-only -m 'requires_postgres' tests/ | head -30

# Verify marker definition
grep "markers" pyproject.toml
```

**Solution**:
```toml
# Ensure markers defined in pyproject.toml
[tool.pytest.ini_options]
markers = [
    "requires_postgres: Tests requiring PostgreSQL database",
    "requires_vault: Tests requiring HashiCorp Vault",
    "requires_auth0: Tests requiring Auth0 authentication",
]
```

### Fixture Errors

**Symptom**: `fixture 'db_connection' not found` or similar

**Diagnostic Steps**:
```bash
# List available fixtures
pytest --fixtures tests/

# Check fixture imports
grep -r "db_connection" tests/fixtures/
```

**Common Solutions**:
```python
# Ensure conftest.py is in correct location
tests/
├── conftest.py              # Session-level fixtures
├── fixtures/
│   ├── conftest.py          # Fixture definitions
│   └── database/
│       └── conftest.py      # Database fixtures
```

---

## Emergency Procedures

### CI Completely Broken

**Immediate Actions**:
```bash
# 1. Check GitHub Actions status
curl https://www.githubstatus.com/api/v2/status.json

# 2. Rerun failed jobs
gh run rerun <RUN_ID>

# 3. If persistent, skip CI temporarily (use with caution)
git commit --no-verify -m "Emergency fix"
```

### Vault Hanging All Enterprise Tests

**Quick Fix**:
```bash
# Disable enterprise workflow temporarily
# Edit .github/workflows/enterprise-tests.yml
# Change schedule to run far in future
on:
  schedule:
    - cron: '0 6 1 1 2099'  # Effectively disabled
```

### Database Connection Pool Exhausted

**Symptom**: Tests hang waiting for connections

**Quick Fix**:
```python
# Increase pool size in test configuration
pool = psycopg_pool.AsyncConnectionPool(
    postgres_url,
    min_size=2,
    max_size=10,  # Increase from 5
    open=False,
)
```

---

## Preventive Measures

### Before Pushing Code

```bash
# 1. Run linters
uv run ruff check src/ tests/
uv run mypy src/

# 2. Run unit tests
pytest tests/unit/ -v

# 3. Run PostgreSQL integration tests
pytest -m 'requires_postgres' -v

# 4. Check test collection time
time pytest --collect-only -q
```

### Monitoring CI Health

```bash
# Check recent CI run durations
gh run list --workflow=quality-gate.yml --limit=10 | awk '{print $2, $7}'

# Check failure rate
gh run list --workflow=quality-gate.yml --limit=50 | \
  grep -c "failure" | \
  awk '{print "Failure rate:", $1/50*100"%"}'
```

---

## Getting Help

### Resources

- **CI Architecture Docs**: `docs/testing/ci-architecture.md`
- **Contributing Guide**: `CONTRIBUTING.md`
- **GitHub Actions Logs**: `gh run view <RUN_ID> --log`
- **Test Fixtures Docs**: `docs/testing/config-fixtures.md`

### Escalation

1. **Check existing issues**: Search GitHub issues for similar problems
2. **Review recent changes**: `git log --oneline --since="1 week ago" -- .github/workflows/`
3. **Ask in team chat**: Provide run ID and error message
4. **Create GitHub issue**: Include logs and steps to reproduce

---

## Appendix: Common Error Messages

| Error Message | Likely Cause | Quick Fix |
|---------------|--------------|-----------|
| `psycopg.OperationalError: could not connect` | PostgreSQL not running | Start PostgreSQL service |
| `Vault not ready after 10 attempts` | Vault startup timeout | Check Docker resources, rerun workflow |
| `fixture 'db_connection' not found` | Missing conftest.py | Check fixture imports |
| `ValidationError: FRAISEQL_ENVIRONMENT` | Missing env var | Set `FRAISEQL_ENVIRONMENT=testing` |
| `Schema 'test_X' does not exist` | Schema cleanup issue | Use `test_schema` fixture correctly |
| `Quality gate blocked` | Failed job in pipeline | Check individual job logs |
| `Collection took 45.2s` | Slow test discovery | Add directories to `norecursedirs` |
| `YAML syntax error` | Workflow file invalid | Validate with `python -c "import yaml; yaml.safe_load(open('...'))"` |

---

**Last Updated**: 2025-12-03
**Maintained by**: FraiseQL DevOps Team
