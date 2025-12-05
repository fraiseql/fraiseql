#!/bin/bash
# Test KMS integration locally using Docker Compose
# This script sets up Vault and runs the KMS tests

set -e

echo "=== FraiseQL KMS Local Testing Script ==="
echo

# Check if docker-compose is available
if ! command -v docker-compose &> /dev/null; then
    echo "❌ docker-compose not found. Please install Docker Compose."
    exit 1
fi

# Start Vault
echo "🚀 Starting Vault test container..."
docker-compose -f docker-compose.kms-test.yml up -d

# Wait for Vault to be ready
echo "⏳ Waiting for Vault to be ready..."
timeout=30
counter=0
while ! curl -s http://localhost:8200/v1/sys/health > /dev/null; do
    if [ $counter -ge $timeout ]; then
        echo "❌ Vault failed to start within ${timeout} seconds"
        docker-compose -f docker-compose.kms-test.yml logs vault
        docker-compose -f docker-compose.kms-test.yml down
        exit 1
    fi
    counter=$((counter + 1))
    sleep 1
done

echo "✅ Vault is ready"

# Set environment variables
export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=local-dev-token
export VAULT_TRANSIT_MOUNT=transit

# Initialize Vault transit engine
echo "🔧 Initializing Vault transit engine..."
docker-compose -f docker-compose.kms-test.yml exec -T vault vault secrets enable transit

# Create test keys
echo "🔑 Creating test keys..."
for key in test-integration-key test-data-key test-key-1 test-key-2; do
    docker-compose -f docker-compose.kms-test.yml exec -T vault \
        vault write -f transit/keys/$key
done

echo "✅ Vault initialized for KMS tests"
echo
echo "🔍 Running KMS tests..."

# Run the tests
if uv run pytest tests/integration/security/test_kms_integration.py::TestVaultIntegration -v; then
    echo "✅ KMS tests passed!"
else
    echo "❌ KMS tests failed"
    exit_code=$?
fi

# Cleanup
echo
echo "🧹 Cleaning up..."
docker-compose -f docker-compose.kms-test.yml down

if [ ${exit_code:-0} -eq 0 ]; then
    echo "🎉 All KMS tests completed successfully!"
else
    echo "💥 KMS tests failed with exit code ${exit_code}"
    exit ${exit_code}
fi
