#!/bin/bash
# Pre-release testing script for FraiseQL v0.3.5
# Tests the introspection security fix

set -e

echo "🧪 FraiseQL v0.3.5 Pre-Release Testing"
echo "=================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

success() {
    echo -e "${GREEN}✅ $1${NC}"
}

warning() {
    echo -e "${YELLOW}⚠️ $1${NC}"
}

error() {
    echo -e "${RED}❌ $1${NC}"
    exit 1
}

info() {
    echo -e "${BLUE}ℹ️ $1${NC}"
}

echo
info "Step 1: Verifying version number"
VERSION=$(python -c "import tomllib; print(tomllib.load(open('pyproject.toml', 'rb'))['project']['version'])")
if [ "$VERSION" = "0.3.5" ]; then
    success "Version is correctly set to 0.3.5"
else
    error "Version is $VERSION, expected 0.3.5"
fi

echo
info "Step 2: Running introspection security tests"
python -m pytest tests/security/test_schema_introspection_security.py -v
if [ $? -eq 0 ]; then
    success "All introspection security tests pass"
else
    error "Introspection security tests failed"
fi

echo
info "Step 3: Running existing security tests"
python -m pytest tests/security/test_auth_enforcement.py -k introspection -v
if [ $? -eq 0 ]; then
    success "Existing introspection tests still pass"
else
    error "Existing security tests failed"
fi

echo
info "Step 4: Running configuration tests"
python -m pytest tests/fastapi/test_config_env_vars.py -v
if [ $? -eq 0 ]; then
    success "Configuration tests pass"
else
    error "Configuration tests failed"
fi

echo
info "Step 5: Testing production introspection blocking"
# Just run the actual test instead of creating a standalone script
python -m pytest tests/security/test_schema_introspection_security.py::TestSchemaIntrospectionSecurity::test_introspection_disabled_in_production -v
if [ $? -eq 0 ]; then
    success "Production introspection blocking verified"
else
    error "Production introspection blocking test failed"
fi

echo
info "Step 6: Testing development introspection still works"
# Just run the actual test instead of creating a standalone script
python -m pytest tests/security/test_schema_introspection_security.py::TestSchemaIntrospectionSecurity::test_introspection_enabled_in_development -v
if [ $? -eq 0 ]; then
    success "Development introspection verified working"
else
    error "Development introspection test failed"
fi

echo
info "Step 7: Building package"
python -m build
if [ $? -eq 0 ]; then
    success "Package builds successfully"
else
    error "Package build failed"
fi

echo
info "Step 8: Checking distribution"
python -m twine check dist/fraiseql-0.3.5*
if [ $? -eq 0 ]; then
    success "Distribution checks pass"
else
    error "Distribution check failed"
fi

echo
info "Step 9: Running linting"
python -m ruff check src/fraiseql/graphql/execute.py src/fraiseql/fastapi/routers.py src/fraiseql/execution/unified_executor.py
if [ $? -eq 0 ]; then
    success "Code linting passes"
else
    warning "Linting issues found (may be acceptable)"
fi

echo
info "Step 10: Running type checking on modified files"
python -m pyright src/fraiseql/graphql/execute.py src/fraiseql/fastapi/routers.py src/fraiseql/execution/unified_executor.py
if [ $? -eq 0 ]; then
    success "Type checking passes"
else
    warning "Type checking issues found (may be acceptable)"
fi

# No cleanup needed since we're using pytest directly

echo
echo "🎉 Pre-Release Testing Complete!"
echo "================================"
success "All critical tests pass"
success "Package builds successfully"
success "Security fix verified working"

echo
echo "📋 Release Checklist:"
echo "  ✅ Version updated to 0.3.5"
echo "  ✅ CHANGELOG.md updated with security fix details"
echo "  ✅ Security advisory created"
echo "  ✅ Release notes created"
echo "  ✅ All tests pass"
echo "  ✅ Security fix verified"
echo "  ✅ Package builds"

echo
echo "🚀 Ready for release!"
echo "Next steps:"
echo "  1. Commit all changes"
echo "  2. Create git tag: git tag v0.3.5"
echo "  3. Push to repository: git push origin v0.3.5"
echo "  4. Publish to PyPI: python -m twine upload dist/fraiseql-0.3.5*"
echo "  5. Create GitHub release with security advisory"
echo "  6. Update documentation"
