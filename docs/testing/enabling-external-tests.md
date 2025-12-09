# Enabling External Service Tests

**Version:** v1.7.0
**Last Updated:** 2025-11-24

This guide shows you how to enable the 10 skipped tests that require external services.

---

## Quick Start

### Option 1: Automated Setup Script (Recommended)

```bash
# Setup Vault + LocalStack (no AWS credentials needed)
./tests/scripts/enable-kms-tests.sh --vault --localstack

# Source the environment
source .env.test.kms

# Run all KMS tests
pytest tests/integration/security/test_kms_integration.py -v
```

### Option 2: Manual Setup

Follow the detailed instructions below for each service.

---

## KMS Integration Tests (6 tests)

### HashiCorp Vault Tests (3 tests)

**What's Tested:**
- Encrypt/decrypt roundtrip with real Vault
- Data key generation
- Multiple key isolation

**Prerequisites:**
- Docker installed and running

**Setup Steps:**

1. **Start Vault in dev mode:**
   ```bash
   docker run -d --rm \
     --name fraiseql-vault-test \
     --cap-add=IPC_LOCK \
     -e 'VAULT_DEV_ROOT_TOKEN_ID=fraiseql-test-token' \
     -p 8200:8200 \
     vault:1.13.3
   ```

2. **Enable transit engine:**
   ```bash
   docker exec fraiseql-vault-test \
     vault secrets enable -path=transit transit
   ```

3. **Create encryption key:**
   ```bash
   docker exec -e VAULT_TOKEN=fraiseql-test-token fraiseql-vault-test \
     vault write -f transit/keys/fraiseql-test
   ```

4. **Set environment variables:**
   ```bash
   export VAULT_ADDR=http://localhost:8200
   export VAULT_TOKEN=fraiseql-test-token
   export VAULT_TRANSIT_MOUNT=transit
   export VAULT_KEY_NAME=fraiseql-test
   ```

5. **Run tests:**
   ```bash
   pytest tests/integration/security/test_kms_integration.py -k vault -v
   ```

**Expected Output:**
```
tests/integration/security/test_kms_integration.py::TestVaultIntegration::test_encrypt_decrypt_roundtrip PASSED
tests/integration/security/test_kms_integration.py::TestVaultIntegration::test_data_key_generation PASSED
tests/integration/security/test_kms_integration.py::TestVaultIntegration::test_different_keys_isolation PASSED

================ 3 passed in 2.45s ================
```

**Cleanup:**
```bash
docker rm -f fraiseql-vault-test
```

---

### AWS KMS Tests (3 tests)

**What's Tested:**
- Encrypt/decrypt with AWS KMS
- Data key generation
- Multiple key isolation

#### Option A: LocalStack (Recommended for Testing)

**Prerequisites:**
- Docker installed and running

**Setup Steps:**

1. **Start LocalStack:**
   ```bash
   docker run -d --rm \
     --name fraiseql-localstack-test \
     -p 4566:4566 \
     -e SERVICES=kms \
     localstack/localstack:latest
   ```

2. **Wait for LocalStack to be ready:**
   ```bash
   # Wait until health check shows KMS available
   curl http://localhost:4566/_localstack/health
   ```

3. **Create KMS key:**
   ```bash
   docker exec fraiseql-localstack-test \
     aws --endpoint-url=http://localhost:4566 kms create-key \
     --region us-east-1 \
     --query 'KeyMetadata.KeyId' \
     --output text
   # Save the output KEY_ID
   ```

4. **Set environment variables:**
   ```bash
   export AWS_ENDPOINT_URL=http://localhost:4566
   export AWS_REGION=us-east-1
   export AWS_ACCESS_KEY_ID=test
   export AWS_SECRET_ACCESS_KEY=test
   export AWS_KMS_KEY_ID=<KEY_ID from step 3>
   ```

5. **Run tests:**
   ```bash
   pytest tests/integration/security/test_kms_integration.py -k aws -v
   ```

**Cleanup:**
```bash
docker rm -f fraiseql-localstack-test
```

#### Option B: Real AWS KMS

**Prerequisites:**
- AWS account with KMS permissions
- AWS credentials configured

**Setup Steps:**

1. **Create KMS key (one-time):**
   ```bash
   aws kms create-key --region us-east-1 --query 'KeyMetadata.KeyId' --output text
   # Save the output KEY_ID
   ```

2. **Set environment variables:**
   ```bash
   export AWS_REGION=us-east-1
   export AWS_ACCESS_KEY_ID=<your access key>
   export AWS_SECRET_ACCESS_KEY=<your secret key>
   export AWS_KMS_KEY_ID=<KEY_ID from step 1>
   ```

3. **Run tests:**
   ```bash
   pytest tests/integration/security/test_kms_integration.py -k aws -v
   ```

**Note:** This will create actual AWS API calls and may incur minimal costs (<$0.01).

---

## Cascade Tests (4 tests) - NOT YET AVAILABLE

**Status:** 🚧 Feature Not Implemented

The Cascade tests are skipped because the GraphQL Cascade feature is still in development. The tests define the expected behavior but the feature implementation is incomplete.

**Tests:**
- `test_cascade_end_to_end` - Complete cascade flow
- `test_cascade_with_error_response` - Error handling
- `test_cascade_mutation_updates_cache` - Cache updates
- `test_cascade_delete_propagates` - Delete propagation

**Why They Skip:**
The test fixtures are defined (`tests/fixtures/cascade/conftest.py`), but the core Cascade feature (`fraiseql.cascade`) is not yet implemented. The tests will start passing once the feature is complete.

**When Will This Be Available:**
Planned for FraiseQL v2.1 (Q1 2026)

**How to Track Progress:**
- GitHub Issue: #xxx (Cascade Feature Implementation)
- Project Board: FraiseQL v2.1 Roadmap

**Current Workaround:**
Use manual cache invalidation with `@mutation(cache_invalidate=["posts", "users"])` decorator.

---

## Using the Setup Script

The `enable-kms-tests.sh` script automates all the setup above.

### Script Location
```bash
./tests/scripts/enable-kms-tests.sh
```

### Usage Examples

**Setup everything (Vault + LocalStack):**
```bash
./tests/scripts/enable-kms-tests.sh --vault --localstack
source .env.test.kms
pytest tests/integration/security/test_kms_integration.py -v
```

**Setup Vault only:**
```bash
./tests/scripts/enable-kms-tests.sh --vault
source .env.test.kms
pytest tests/integration/security/test_kms_integration.py -k vault -v
```

**Setup with real AWS:**
```bash
# Set AWS credentials first
export AWS_ACCESS_KEY_ID=your_key
export AWS_SECRET_ACCESS_KEY=your_secret
export AWS_KMS_KEY_ID=your_key_id

# Run script
./tests/scripts/enable-kms-tests.sh --vault --aws
source .env.test.kms
pytest tests/integration/security/test_kms_integration.py -v
```

**Cleanup all services:**
```bash
./tests/scripts/enable-kms-tests.sh --cleanup
```

### Script Options

```
OPTIONS:
    -v, --vault         Setup HashiCorp Vault (Docker)
    -a, --aws           Setup AWS KMS (requires credentials)
    -l, --localstack    Use LocalStack for AWS (Docker, no real AWS credentials)
    -c, --cleanup       Cleanup running containers and stop services
    -h, --help          Show help message
```

---

## Environment File

The setup script creates `.env.test.kms` with all necessary environment variables:

```bash
# HashiCorp Vault Configuration
export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=fraiseql-test-token
export VAULT_TRANSIT_MOUNT=transit
export VAULT_KEY_NAME=fraiseql-test

# AWS KMS Configuration (LocalStack)
export AWS_ENDPOINT_URL=http://localhost:4566
export AWS_REGION=us-east-1
export AWS_ACCESS_KEY_ID=test
export AWS_SECRET_ACCESS_KEY=test
export AWS_KMS_KEY_ID=<generated-key-id>
```

**To use:**
```bash
source .env.test.kms
```

---

## CI/CD Integration

### GitHub Actions

To enable KMS tests in CI, add secrets and update workflow:

```yaml
# .github/workflows/test-kms.yml
name: KMS Integration Tests

on: [push, pull_request]

jobs:
  test-vault:
    runs-on: ubuntu-latest
    services:
      vault:
        image: vault:1.13.3
        env:
          VAULT_DEV_ROOT_TOKEN_ID: test-token
        ports:
          - 8200:8200
        options: --cap-add=IPC_LOCK

    steps:
      - uses: actions/checkout@v4
      - name: Setup Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.13'

      - name: Install dependencies
        run: |
          pip install -e ".[dev]"

      - name: Enable Vault transit
        run: |
          docker exec ${{ job.services.vault.id }} \
            vault secrets enable -path=transit transit
          docker exec -e VAULT_TOKEN=test-token ${{ job.services.vault.id }} \
            vault write -f transit/keys/test-key

      - name: Run Vault tests
        env:
          VAULT_ADDR: http://localhost:8200
          VAULT_TOKEN: test-token
          VAULT_TRANSIT_MOUNT: transit
          VAULT_KEY_NAME: test-key
        run: |
          pytest tests/integration/security/test_kms_integration.py -k vault -v

  test-aws-localstack:
    runs-on: ubuntu-latest
    services:
      localstack:
        image: localstack/localstack:latest
        env:
          SERVICES: kms
        ports:
          - 4566:4566

    steps:
      - uses: actions/checkout@v4
      - name: Setup Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.13'

      - name: Install dependencies
        run: |
          pip install -e ".[dev]"

      - name: Create KMS key
        run: |
          # Wait for LocalStack
          sleep 10
          # Create key
          KEY_ID=$(docker exec ${{ job.services.localstack.id}} \
            aws --endpoint-url=http://localhost:4566 kms create-key \
            --region us-east-1 --query 'KeyMetadata.KeyId' --output text)
          echo "KMS_KEY_ID=$KEY_ID" >> $GITHUB_ENV

      - name: Run AWS tests
        env:
          AWS_ENDPOINT_URL: http://localhost:4566
          AWS_REGION: us-east-1
          AWS_ACCESS_KEY_ID: test
          AWS_SECRET_ACCESS_KEY: test
          AWS_KMS_KEY_ID: ${{ env.KMS_KEY_ID }}
        run: |
          pytest tests/integration/security/test_kms_integration.py -k aws -v
```

---

## Troubleshooting

### Vault Container Won't Start

**Problem:** Port 8200 already in use

**Solution:**
```bash
# Find what's using port 8200
lsof -i :8200

# Kill existing Vault containers
docker rm -f $(docker ps -a | grep vault | awk '{print $1}')
```

### LocalStack KMS Not Available

**Problem:** KMS service shows as "unavailable"

**Solution:**
```bash
# Check LocalStack logs
docker logs fraiseql-localstack-test

# Restart with explicit KMS service
docker rm -f fraiseql-localstack-test
docker run -d --rm --name fraiseql-localstack-test \
  -p 4566:4566 \
  -e SERVICES=kms \
  -e DEBUG=1 \
  localstack/localstack:latest

# Wait longer for startup
sleep 15
```

### Tests Still Skipping

**Problem:** Environment variables not set

**Solution:**
```bash
# Verify environment variables
echo $VAULT_ADDR
echo $AWS_REGION

# Re-source the environment file
source .env.test.kms

# Or set manually
export VAULT_ADDR=http://localhost:8200
# ... (other vars)
```

### AWS Real Tests Failing

**Problem:** Insufficient permissions

**Solution:**
Ensure your AWS IAM user/role has these permissions:
```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "kms:CreateKey",
        "kms:Encrypt",
        "kms:Decrypt",
        "kms:GenerateDataKey",
        "kms:DescribeKey"
      ],
      "Resource": "*"
    }
  ]
}
```

---

## Summary

| Service | Tests | Setup Time | Requires | Command |
|---------|-------|------------|----------|---------|
| **Vault** | 3 | 2 min | Docker | `./tests/scripts/enable-kms-tests.sh -v` |
| **AWS (LocalStack)** | 3 | 3 min | Docker | `./tests/scripts/enable-kms-tests.sh -l` |
| **AWS (Real)** | 3 | 1 min | AWS Account | `./tests/scripts/enable-kms-tests.sh -a` |
| **Cascade** | 4 | N/A | Feature WIP | Not yet available |
| **Total** | **10** | **5-6 min** | **Docker** | `./tests/scripts/enable-kms-tests.sh -vl` |

---

## Related Documentation

- [Skipped Tests Overview](skipped-tests.md) - Complete list of all skipped tests
- [Security Configuration](../security/configuration.md) - Production KMS setup

- [KMS Architecture ADR](../architecture/decisions/0003-kms-architecture.md) - KMS design decisions

---

**Last Updated:** 2025-11-24
**Maintained By:** FraiseQL Core Team
**Questions:** https://github.com/fraiseql/fraiseql/issues
