# Enterprise Workflow

FraiseQL's enterprise features (Vault KMS, Auth0 authentication) run in a separate CI pipeline that doesn't block main development. This document explains how the enterprise workflow works and when to use it.

## Overview

**Why separate enterprise CI?**

- **Reliability**: Main CI uses only PostgreSQL (100% reliable)
- **Speed**: Enterprise tests don't slow down regular development
- **Flexibility**: Enterprise features can be tested independently
- **Cost**: External services aren't needed for every PR

## Workflow Architecture

```
Main CI Pipeline (quality-gate.yml)
├── Runs on every PR
├── PostgreSQL only
├── Fast feedback (~10-12 min)
└── Required for merge ✅

Enterprise CI Pipeline (enterprise-tests.yml)
├── Runs weekly + manual
├── Vault + Auth0 services
├── Comprehensive testing
└── Optional (doesn't block) ⚠️
```

## When Enterprise CI Runs

### Automatic Triggers

1. **Weekly Schedule**: Every Monday at 6 AM UTC
2. **Main Branch Push**: When code is merged to `main`

### Manual Triggers

Via GitHub Actions UI:
1. Go to repository → Actions tab
2. Select "Enterprise Tests (Optional)" workflow
3. Click "Run workflow"
4. Optionally specify test filter (e.g., `requires_vault`)

## Test Categories

### Vault KMS Tests

**Purpose**: Test encryption/decryption with HashiCorp Vault

**Requirements**:
- Vault server running in development mode
- Transit engine enabled
- Test encryption keys created

**Markers**: `@pytest.mark.requires_vault`

**Example**:
```python
@pytest.mark.requires_vault
def test_data_encryption(vault_client):
    """Test encrypting sensitive data."""
    encrypted = vault_client.encrypt("test-key", "sensitive data")
    assert encrypted is not None
```

### Auth0 Tests

**Purpose**: Test authentication and authorization flows

**Requirements**:
- Auth0 mock responses (no real Auth0 server needed)
- JWT validation logic
- User context handling

**Markers**: `@pytest.mark.requires_auth0`

**Example**:
```python
@pytest.mark.requires_auth0
def test_jwt_validation(auth_config):
    """Test JWT token validation."""
    token = create_test_jwt()
    user = validate_jwt_token(token)
    assert user is not None
```

## Local Enterprise Testing

### Vault Setup

```bash
# Start Vault in development mode
docker run -d --name vault \
  -p 8200:8200 \
  -e VAULT_DEV_ROOT_TOKEN_ID=fraiseql-ci-token \
  hashicorp/vault:latest

# Set environment variables
export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=fraiseql-ci-token

# Initialize for testing
curl -X POST -H "X-Vault-Token: $VAULT_TOKEN" \
  http://localhost:8200/v1/sys/mounts/transit \
  -d '{"type":"transit"}'

# Create test keys
for key in test-integration-key test-data-key; do
  curl -X POST -H "X-Vault-Token: $VAULT_TOKEN" \
    http://localhost:8200/v1/transit/keys/$key
done
```

### Running Enterprise Tests Locally

```bash
# Run all enterprise tests (requires services)
pytest -m 'requires_vault or requires_auth0'

# Run only Vault tests
pytest -m 'requires_vault'

# Run only Auth0 tests
pytest -m 'requires_auth0'

# Run with verbose output
pytest -m 'requires_vault' -v
```

### Skipping Enterprise Tests

```bash
# Run everything except enterprise features
pytest -m 'not requires_vault and not requires_auth0'

# This is the same as main CI
pytest -m 'requires_postgres and not requires_vault and not requires_auth0'
```

## CI Reliability Features

### Vault Startup Handling

**Problem**: Vault containers can be slow to start or flaky

**Solutions**:
- **Exponential backoff**: 2^attempt seconds (up to ~17 minutes)
- **Health checks**: Wait for `/v1/sys/health` endpoint
- **Grace period**: 20 seconds before first health check
- **Retry limit**: 10 attempts maximum

### Failure Handling

**Enterprise job failures don't stop the workflow**:
- `continue-on-error: true` on enterprise jobs
- Summary job always succeeds (logs results)
- Test artifacts uploaded for debugging

### Service Dependencies

**PostgreSQL**: Always available (required for all tests)

**Vault**: Optional, with fallback handling:
```python
@pytest.mark.skipif(not os.environ.get("VAULT_ADDR"), reason="Vault not available")
def test_vault_feature():
    pass
```

**Auth0**: Uses mocks, no external service required

## Workflow Results

### Success Path

```
✅ Vault KMS Tests: success
✅ Auth0 Tests: success
✅ All enterprise tests passed!
```

### Partial Failure Path

```
⚠️  Vault KMS Tests: failure (logged)
✅ Auth0 Tests: success
⚠️  Some enterprise tests failed or were skipped
```

### Complete Skip Path

```
- Vault KMS Tests: skipped (service unavailable)
- Auth0 Tests: skipped (service unavailable)
⚠️  Some enterprise tests failed or were skipped
```

## Debugging Enterprise Failures

### Vault Issues

**Container not starting**:
```bash
# Check container status
docker ps | grep vault

# Check logs
docker logs vault

# Restart container
docker restart vault
```

**Health check failing**:
```bash
# Manual health check
curl http://localhost:8200/v1/sys/health

# Check Vault status
curl -H "X-Vault-Token: $VAULT_TOKEN" \
  http://localhost:8200/v1/sys/seal-status
```

**Encryption failing**:
```bash
# List available keys
curl -H "X-Vault-Token: $VAULT_TOKEN" \
  http://localhost:8200/v1/transit/keys

# Test encryption manually
curl -X POST -H "X-Vault-Token: $VAULT_TOKEN" \
  -d '{"plaintext":"dGVzdA=="}' \
  http://localhost:8200/v1/transit/encrypt/test-key
```

### Auth0 Issues

**Mock setup problems**:
- Check mock configuration in test fixtures
- Verify JWT generation logic
- Ensure test tokens are properly formatted

**Validation failures**:
- Check token expiration
- Verify audience/issuer claims
- Confirm signing algorithm

## Enterprise Test Development

### Adding New Enterprise Tests

1. **Choose appropriate marker**:
   ```python
   @pytest.mark.requires_vault  # For Vault features
   @pytest.mark.requires_auth0  # For Auth0 features
   ```

2. **Handle service unavailability**:
   ```python
   @pytest.mark.requires_vault
   @pytest.mark.skipif(not os.environ.get("VAULT_ADDR"), reason="Vault not available")
   def test_vault_feature(vault_client):
       pass
   ```

3. **Use proper fixtures**:
   ```python
   def test_vault_encryption(vault_kms_config):
       # vault_kms_config has Vault settings
       pass
   ```

### Enterprise Feature Checklist

- [ ] Tests use appropriate `@pytest.mark.requires_*` markers
- [ ] Tests handle service unavailability gracefully
- [ ] Tests use config fixtures, not direct FraiseQLConfig creation
- [ ] Documentation updated for new enterprise features
- [ ] CI enterprise workflow tested (manual trigger)

## Release Process

### Before Release

1. **Manual enterprise test run**:
   - Trigger enterprise workflow manually
   - Verify all tests pass
   - Review any failures

2. **Service verification**:
   - Ensure Vault and Auth0 services are properly configured
   - Check for service outages or API changes

3. **Fallback planning**:
   - Document how to proceed if enterprise tests fail
   - Consider enterprise features as "best effort" for releases

### Release Criteria

- ✅ Main CI passes (required)
- ✅ Enterprise CI passes (recommended)
- ✅ Critical enterprise features documented
- ✅ Fallback behavior implemented for service outages

## Best Practices

### For Developers

1. **Test locally first**: Run enterprise tests before pushing
2. **Use markers correctly**: Don't mark basic tests as enterprise
3. **Handle failures gracefully**: Enterprise tests should skip when services unavailable
4. **Document requirements**: Explain what enterprise services are needed

### For Maintainers

1. **Monitor enterprise CI**: Check weekly results
2. **Update service configurations**: Keep Vault/Auth0 settings current
3. **Review failures**: Investigate enterprise test failures regularly
4. **Balance reliability vs. coverage**: Don't let flaky tests block development

### For Contributors

1. **Ask about enterprise features**: Check if new features need enterprise testing
2. **Follow existing patterns**: Use established fixtures and markers
3. **Test enterprise features**: If adding enterprise functionality, test it
4. **Update documentation**: Document any new enterprise requirements

## Troubleshooting

### Common Issues

**"Vault not available" errors**:
- Ensure Vault container is running
- Check VAULT_ADDR and VAULT_TOKEN environment variables
- Verify transit engine is enabled

**Auth0 mock failures**:
- Check mock setup in test fixtures
- Verify JWT token format
- Ensure test keys are properly configured

**CI timeout issues**:
- Enterprise tests have longer timeouts
- Check for slow external service calls
- Consider optimizing test setup

### Getting Help

1. **Check CI logs**: Review GitHub Actions enterprise workflow runs
2. **Local reproduction**: Try running tests locally with services
3. **Service status**: Check if Vault/Auth0 services are having issues
4. **Documentation**: Review this guide and related docs

## Future Improvements

- **Service mocking**: Reduce dependency on real external services
- **Parallel execution**: Run Vault and Auth0 tests simultaneously
- **Performance monitoring**: Track enterprise test execution times
- **Automated retries**: Better handling of transient service failures
- **Service alternatives**: Support for other KMS/Auth providers

## Reference

- [CI Architecture](./ci-architecture.md)
- [Pytest Markers](./pytest-markers.md)
- [Config Fixtures](./config-fixtures.md)
- [Developer Guide](./developer-guide.md)
