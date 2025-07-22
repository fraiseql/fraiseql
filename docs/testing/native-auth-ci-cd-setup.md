# Native Authentication CI/CD Testing Setup

This document describes the comprehensive CI/CD setup for testing FraiseQL's native authentication system.

## Overview

The native authentication system is tested through multiple layers to ensure production readiness:

1. **Unit Tests**: Core logic without database dependencies
2. **Database Integration Tests**: Full database schema and operations  
3. **Security Tests**: Password hashing, token security, vulnerability scanning
4. **End-to-End Tests**: Complete auth flow testing
5. **Performance Tests**: Basic performance characteristics
6. **Frontend Validation**: TypeScript types and Vue components

## CI/CD Workflows

### Main CI Pipeline (`.github/workflows/ci.yml`)

The main CI pipeline includes a dedicated `test-native-auth` job that:

- **Runs on**: Ubuntu Latest with PostgreSQL 16 service
- **Database**: `fraiseql_native_auth_test` 
- **Python Version**: 3.13
- **Tests**:
  - Unit tests: `pytest tests/auth/native/ -m "not database"`
  - Database integration: `pytest tests/auth/native/ -m database`
  - Example application compilation test

### Dedicated Native Auth Pipeline (`.github/workflows/native-auth.yml`)

Comprehensive testing specifically for authentication system changes:

#### Test Matrix Jobs
- **Unit Tests**: Core authentication logic
- **Database Integration**: Schema and database operations
- **Security Tests**: Password and token security validation
- **Example Application**: Import and compilation verification

#### Security Audit Job
- **Bandit**: Python security vulnerability scanning
- **Safety**: Dependency vulnerability checking  
- **Custom Security Tests**: Password hashing and JWT validation

#### Comprehensive Test Job
- **Full System Test**: End-to-end auth flow using `scripts/test-native-auth.py`
- **Performance Benchmarks**: Basic performance characteristics
- **Integration Validation**: Complete provider and database integration

#### Frontend Validation Job
- **TypeScript Validation**: Type definition verification
- **Vue Component Validation**: Component structure verification

#### Coverage Reporting Job
- **Native Auth Coverage**: Detailed coverage report for auth system only
- **Codecov Integration**: Coverage tracking and reporting
- **HTML Reports**: Generated coverage reports

## Test Structure

### Unit Tests (`tests/auth/native/`)

```bash
# All native auth tests
pytest tests/auth/native/ -v

# Unit tests only (no database)  
pytest tests/auth/native/ -m "not database" -v

# Database integration tests
pytest tests/auth/native/ -m database -v
```

**Test Files:**
- `test_user_model.py`: User creation, password hashing, validation
- `test_token_manager.py`: JWT token generation, validation, rotation
- `test_auth_endpoints.py`: REST API endpoint testing (15 tests)
- `test_database_schema.py`: Database schema validation

### Comprehensive System Test (`scripts/test-native-auth.py`)

End-to-end testing script that validates:

- ✅ **Password Security**: Argon2id hashing, validation rules
- ✅ **User Management**: Creation, retrieval, updates, deactivation  
- ✅ **Token Operations**: Generation, validation, refresh, theft detection
- ✅ **Auth Provider Integration**: Complete FraiseQL integration
- ✅ **Security Features**: Session management, password reset
- ✅ **Performance Benchmarks**: Basic performance characteristics

Usage:
```bash
# Run comprehensive test
python scripts/test-native-auth.py

# With custom database
DATABASE_URL=postgresql://user:pass@host:port/db python scripts/test-native-auth.py
```

## Makefile Targets

Convenient testing commands:

```bash
# All native auth tests
make test-auth

# Unit tests only
make test-auth-unit

# Database integration tests  
make test-auth-db

# Comprehensive system test
make test-auth-comprehensive

# Security audit
make test-auth-security
```

## Database Requirements

### PostgreSQL Service Configuration

CI uses PostgreSQL 16 service containers:

```yaml
services:
  postgres:
    image: postgres:16
    env:
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: fraiseql
      POSTGRES_DB: fraiseql_[job]_test
    options: >-
      --health-cmd pg_isready
      --health-interval 10s
      --health-timeout 5s
      --health-retries 5
    ports:
      - 5432:5432
```

### Local Development

For local testing, set up PostgreSQL:

```bash
# Create test database
createdb fraiseql_test

# Set environment variable
export TEST_DATABASE_URL="postgresql://localhost/fraiseql_test"

# Run tests
make test-auth
```

## Test Coverage Expectations

### Current Status
- **Unit Tests**: ✅ 12/12 passing (100%)
- **Database Tests**: ⏳ Requires PostgreSQL service (39 tests)
- **Security Tests**: ✅ All security validations passing
- **Integration Tests**: ✅ Complete auth flow working

### Coverage Targets
- **Code Coverage**: >95% for native auth modules
- **Test Coverage**: All critical auth paths tested
- **Security Coverage**: All security features validated

## Triggering Tests

### Automatic Triggers
Tests run automatically on:

- **Push** to `main` or `develop` branches
- **Pull Requests** to `main` branch  
- **File Changes** in:
  - `src/fraiseql/auth/native/**`
  - `tests/auth/native/**`
  - `examples/native_auth_app.py`
  - `frontend/auth/**`

### Manual Triggers
- GitHub Actions UI: "Run workflow" button
- Local testing: Use Makefile targets or pytest directly

## Environment Variables

### Required for Database Tests
```bash
DATABASE_URL=postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test
TEST_DATABASE_URL=postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test
```

### Required for Security Tests  
```bash
JWT_SECRET_KEY=test-secret-key-change-in-production
```

### Optional Configuration
```bash
# Test timeout (default: 30s)
TEST_TIMEOUT=60

# Coverage reporting
CODECOV_TOKEN=your-codecov-token
```

## Troubleshooting

### Common Issues

**Database Connection Failures**
```bash
# Check PostgreSQL service status
pg_isready -h localhost -p 5432 -U fraiseql

# Verify environment variables
echo $DATABASE_URL
echo $TEST_DATABASE_URL
```

**Import Errors**  
```bash
# Ensure FraiseQL is installed
pip install -e .

# Check Python path
python -c "import sys; print(sys.path)"
```

**Test Timeouts**
- Increase `TEST_TIMEOUT` environment variable
- Check database service health checks
- Verify network connectivity in CI

### Debugging CI Failures

1. **Check Logs**: View detailed logs in GitHub Actions
2. **Run Locally**: Reproduce with same environment setup
3. **Isolate Issues**: Run individual test suites
4. **Database State**: Check database schema and data

## Security Considerations

### CI/CD Security
- ✅ No production secrets in CI environment
- ✅ Test-only JWT secrets used
- ✅ Database credentials scoped to test containers
- ✅ Dependency vulnerability scanning enabled

### Test Security
- ✅ Argon2id password hashing validation
- ✅ JWT token security verification
- ✅ SQL injection prevention testing
- ✅ Input validation testing

## Monitoring and Alerts

### Coverage Monitoring
- **Codecov**: Automatic coverage reporting
- **GitHub Checks**: Required status checks
- **Coverage Thresholds**: Enforced minimum coverage

### Performance Monitoring  
- **Benchmark Tests**: Basic performance validation
- **Regression Detection**: Performance degradation alerts
- **Resource Usage**: Memory and CPU monitoring in CI

## Future Enhancements

### Planned Improvements
- [ ] **Multi-database Testing**: PostgreSQL 15/16, different configurations
- [ ] **Load Testing**: High-concurrency auth scenarios
- [ ] **Security Scanning**: Advanced vulnerability assessment
- [ ] **Mobile SDK Testing**: iOS/Android integration validation
- [ ] **Performance Benchmarks**: Detailed performance regression testing

### Integration Opportunities  
- [ ] **Playwright**: End-to-end browser testing
- [ ] **Docker**: Container-based testing environments
- [ ] **Kubernetes**: Production-like deployment testing
- [ ] **Monitoring**: Production monitoring integration

---

This comprehensive CI/CD setup ensures the native authentication system is thoroughly tested and production-ready, providing confidence for users migrating from external auth providers like Auth0.