#!/bin/bash
# Enable KMS Integration Tests
# This script sets up Vault and/or AWS KMS for running integration tests

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default values
SETUP_VAULT=false
SETUP_AWS=false
USE_LOCALSTACK=false
CLEANUP=false

usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Enable KMS integration tests by setting up required services.

OPTIONS:
    -v, --vault         Setup HashiCorp Vault (Docker)
    -a, --aws           Setup AWS KMS (requires credentials)
    -l, --localstack    Use LocalStack for AWS (Docker, no real AWS credentials)
    -c, --cleanup       Cleanup running containers and stop services
    -h, --help          Show this help message

EXAMPLES:
    # Setup Vault only
    $0 --vault

    # Setup both Vault and AWS (LocalStack)
    $0 --vault --localstack

    # Setup with real AWS credentials
    $0 --vault --aws

    # Cleanup all services
    $0 --cleanup

AFTER SETUP:
    # Run Vault tests
    pytest tests/integration/security/test_kms_integration.py -k vault -v

    # Run AWS tests
    pytest tests/integration/security/test_kms_integration.py -k aws -v

    # Run all KMS tests
    pytest tests/integration/security/test_kms_integration.py -v
EOF
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -v|--vault)
            SETUP_VAULT=true
            shift
            ;;
        -a|--aws)
            SETUP_AWS=true
            shift
            ;;
        -l|--localstack)
            USE_LOCALSTACK=true
            SETUP_AWS=true
            shift
            ;;
        -c|--cleanup)
            CLEANUP=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            usage
            exit 1
            ;;
    esac
done

cleanup_services() {
    echo -e "${YELLOW}Cleaning up KMS test services...${NC}"

    # Stop Vault
    if docker ps -a | grep -q fraiseql-vault-test; then
        echo "Stopping Vault container..."
        docker rm -f fraiseql-vault-test 2>/dev/null || true
    fi

    # Stop LocalStack
    if docker ps -a | grep -q fraiseql-localstack-test; then
        echo "Stopping LocalStack container..."
        docker rm -f fraiseql-localstack-test 2>/dev/null || true
    fi

    # Clear environment file
    if [ -f "$PROJECT_ROOT/.env.test.kms" ]; then
        rm "$PROJECT_ROOT/.env.test.kms"
        echo "Removed .env.test.kms"
    fi

    echo -e "${GREEN}✓ Cleanup complete${NC}"
}

setup_vault() {
    echo -e "${YELLOW}Setting up HashiCorp Vault...${NC}"

    # Check if Docker is running
    if ! docker info > /dev/null 2>&1; then
        echo -e "${RED}✗ Docker is not running. Please start Docker first.${NC}"
        exit 1
    fi

    # Stop existing container if running
    docker rm -f fraiseql-vault-test 2>/dev/null || true

    # Start Vault in dev mode
    echo "Starting Vault container..."
    docker run -d --rm \
        --name fraiseql-vault-test \
        --cap-add=IPC_LOCK \
        -e 'VAULT_DEV_ROOT_TOKEN_ID=fraiseql-test-token' \
        -e 'VAULT_DEV_LISTEN_ADDRESS=0.0.0.0:8200' \
        -p 8200:8200 \
        vault:1.13.3

    # Wait for Vault to be ready
    echo "Waiting for Vault to be ready..."
    for i in {1..30}; do
        if curl -s http://localhost:8200/v1/sys/health > /dev/null 2>&1; then
            break
        fi
        sleep 1
    done

    # Enable transit engine
    echo "Enabling transit secrets engine..."
    docker exec fraiseql-vault-test \
        vault secrets enable -path=transit transit 2>/dev/null || true

    # Create test encryption key
    echo "Creating test encryption key..."
    docker exec fraiseql-vault-test sh -c '
        VAULT_TOKEN=fraiseql-test-token vault write -f transit/keys/fraiseql-test
    ' 2>/dev/null || true

    # Save environment variables
    cat >> "$PROJECT_ROOT/.env.test.kms" << EOF
# HashiCorp Vault Configuration
export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=fraiseql-test-token
export VAULT_TRANSIT_MOUNT=transit
export VAULT_KEY_NAME=fraiseql-test

EOF

    echo -e "${GREEN}✓ Vault is ready${NC}"
    echo "  - URL: http://localhost:8200"
    echo "  - Token: fraiseql-test-token"
    echo "  - Transit Mount: transit"
    echo "  - Test Key: fraiseql-test"
}

setup_localstack() {
    echo -e "${YELLOW}Setting up LocalStack (AWS emulator)...${NC}"

    # Check if Docker is running
    if ! docker info > /dev/null 2>&1; then
        echo -e "${RED}✗ Docker is not running. Please start Docker first.${NC}"
        exit 1
    fi

    # Stop existing container if running
    docker rm -f fraiseql-localstack-test 2>/dev/null || true

    # Start LocalStack
    echo "Starting LocalStack container..."
    docker run -d --rm \
        --name fraiseql-localstack-test \
        -p 4566:4566 \
        -e SERVICES=kms \
        -e DEBUG=1 \
        localstack/localstack:latest

    # Wait for LocalStack to be ready
    echo "Waiting for LocalStack to be ready..."
    for i in {1..30}; do
        if curl -s http://localhost:4566/_localstack/health | grep -q '"kms": "available"' 2>/dev/null; then
            break
        fi
        sleep 1
    done

    # Create KMS key
    echo "Creating KMS key in LocalStack..."
    sleep 5  # Extra wait for KMS service

    KEY_ID=$(docker exec fraiseql-localstack-test \
        aws --endpoint-url=http://localhost:4566 kms create-key \
        --region us-east-1 \
        --query 'KeyMetadata.KeyId' \
        --output text 2>/dev/null || echo "")

    if [ -z "$KEY_ID" ]; then
        echo -e "${YELLOW}⚠ Could not create KMS key automatically. You may need to create it manually.${NC}"
        KEY_ID="test-key-id"
    fi

    # Save environment variables
    cat >> "$PROJECT_ROOT/.env.test.kms" << EOF
# AWS KMS Configuration (LocalStack)
export AWS_ENDPOINT_URL=http://localhost:4566
export AWS_REGION=us-east-1
export AWS_ACCESS_KEY_ID=test
export AWS_SECRET_ACCESS_KEY=test
export AWS_KMS_KEY_ID=$KEY_ID

EOF

    echo -e "${GREEN}✓ LocalStack is ready${NC}"
    echo "  - URL: http://localhost:4566"
    echo "  - Region: us-east-1"
    echo "  - Key ID: $KEY_ID"
    echo "  - Access Key: test"
}

setup_real_aws() {
    echo -e "${YELLOW}Setting up AWS KMS (real AWS)...${NC}"

    # Check for AWS credentials
    if [ -z "$AWS_ACCESS_KEY_ID" ] || [ -z "$AWS_SECRET_ACCESS_KEY" ]; then
        echo -e "${RED}✗ AWS credentials not found in environment${NC}"
        echo "Please set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY"
        echo "Or use --localstack for local testing"
        exit 1
    fi

    if [ -z "$AWS_REGION" ]; then
        echo -e "${YELLOW}⚠ AWS_REGION not set, using us-east-1${NC}"
        AWS_REGION="us-east-1"
    fi

    if [ -z "$AWS_KMS_KEY_ID" ]; then
        echo -e "${RED}✗ AWS_KMS_KEY_ID not set${NC}"
        echo "Please create a KMS key in AWS Console and set AWS_KMS_KEY_ID"
        echo "Or run: aws kms create-key --region $AWS_REGION"
        exit 1
    fi

    # Save environment variables
    cat >> "$PROJECT_ROOT/.env.test.kms" << EOF
# AWS KMS Configuration (Real AWS)
export AWS_REGION=$AWS_REGION
export AWS_ACCESS_KEY_ID=$AWS_ACCESS_KEY_ID
export AWS_SECRET_ACCESS_KEY=$AWS_SECRET_ACCESS_KEY
export AWS_KMS_KEY_ID=$AWS_KMS_KEY_ID

EOF

    echo -e "${GREEN}✓ AWS KMS configuration saved${NC}"
    echo "  - Region: $AWS_REGION"
    echo "  - Key ID: $AWS_KMS_KEY_ID"
}

show_usage_instructions() {
    echo ""
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}  KMS Test Services Ready${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo ""
    echo "To use the services, source the environment file:"
    echo ""
    echo -e "  ${YELLOW}source .env.test.kms${NC}"
    echo ""
    echo "Then run the tests:"
    echo ""

    if [ "$SETUP_VAULT" = true ]; then
        echo "  # Vault tests"
        echo -e "  ${YELLOW}pytest tests/integration/security/test_kms_integration.py -k vault -v${NC}"
        echo ""
    fi

    if [ "$SETUP_AWS" = true ]; then
        echo "  # AWS KMS tests"
        echo -e "  ${YELLOW}pytest tests/integration/security/test_kms_integration.py -k aws -v${NC}"
        echo ""
    fi

    echo "  # All KMS tests"
    echo -e "  ${YELLOW}pytest tests/integration/security/test_kms_integration.py -v${NC}"
    echo ""
    echo "To cleanup:"
    echo -e "  ${YELLOW}$0 --cleanup${NC}"
    echo ""
}

# Main execution
if [ "$CLEANUP" = true ]; then
    cleanup_services
    exit 0
fi

if [ "$SETUP_VAULT" = false ] && [ "$SETUP_AWS" = false ]; then
    echo -e "${RED}No services specified. Use -v for Vault or -a/-l for AWS.${NC}"
    usage
    exit 1
fi

# Remove old env file
rm -f "$PROJECT_ROOT/.env.test.kms"

# Setup services
if [ "$SETUP_VAULT" = true ]; then
    setup_vault
fi

if [ "$SETUP_AWS" = true ]; then
    if [ "$USE_LOCALSTACK" = true ]; then
        setup_localstack
    else
        setup_real_aws
    fi
fi

show_usage_instructions
