#!/bin/bash
# Test script for building and verifying FraiseQL package

set -e

echo "🔧 Testing FraiseQL package build..."

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Create temporary directory for testing
TEMP_DIR=$(mktemp -d)
echo "📁 Working in temporary directory: $TEMP_DIR"

# Save current directory
ORIGINAL_DIR=$(pwd)

# Function to cleanup
cleanup() {
    cd "$ORIGINAL_DIR"
    rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

# Install build tools
echo "📦 Installing build tools..."
pip install -q build twine

# Build the package
echo "🏗️  Building distribution packages..."
python -m build

# Check the built files
echo "📋 Checking built distributions..."
ls -la dist/

# Check with twine
echo "🔍 Running twine check..."
twine check dist/*

# Test installation in clean environment
echo "🧪 Testing installation in clean virtual environment..."
cd "$TEMP_DIR"
python -m venv test-env
source test-env/bin/activate

# Install the wheel
echo "📥 Installing FraiseQL wheel..."
pip install "$ORIGINAL_DIR"/dist/*.whl

# Test basic import
echo "🐍 Testing basic import..."
python -c "import fraiseql; print(f'✅ FraiseQL {fraiseql.__version__} imported successfully')"

# Test that main components are available
echo "🧩 Testing component imports..."
python -c "
from fraiseql import (
    fraise_type, fraise_input, fraise_field,
    build_fraiseql_schema, CQRSRepository
)
print('✅ Core components imported successfully')
"

# Test with dependencies
echo "📚 Testing with optional dependencies..."
pip install fastapi uvicorn
python -c "
from fraiseql import create_fraiseql_app
print('✅ FastAPI integration available')
"

deactivate

# Summary
echo ""
echo "✨ ${GREEN}Package build test completed successfully!${NC}"
echo ""
echo "📊 Build artifacts:"
du -h "$ORIGINAL_DIR"/dist/*
echo ""
echo "🚀 Ready to publish to PyPI!"
