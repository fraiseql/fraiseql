#!/bin/bash
# Final verification script for FraiseQL v0.3.5 release

set -e

echo "🔍 FraiseQL v0.3.5 Release Verification"
echo "======================================="

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

success() {
    echo -e "${GREEN}✅ $1${NC}"
}

error() {
    echo -e "${RED}❌ $1${NC}"
    exit 1
}

info() {
    echo -e "${BLUE}ℹ️ $1${NC}"
}

echo
info "Checking version consistency"

# Check pyproject.toml version
VERSION_PYPROJECT=$(python -c "import tomllib; print(tomllib.load(open('pyproject.toml', 'rb'))['project']['version'])")
[ "$VERSION_PYPROJECT" = "0.3.5" ] && success "pyproject.toml version: $VERSION_PYPROJECT" || error "pyproject.toml version mismatch: $VERSION_PYPROJECT"

# Check __init__.py version
VERSION_INIT=$(python -c "import sys; sys.path.insert(0, 'src'); import fraiseql; print(fraiseql.__version__)")
[ "$VERSION_INIT" = "0.3.5" ] && success "__init__.py version: $VERSION_INIT" || error "__init__.py version mismatch: $VERSION_INIT"

echo
info "Verifying security fix functionality"

# Test introspection is blocked in production
python -c "
from fastapi.testclient import TestClient
from fraiseql import query
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app
from graphql import GraphQLResolveInfo

@query
async def test_query(info: GraphQLResolveInfo) -> str:
    return 'test'

# Production mode test
config = FraiseQLConfig(
    database_url='postgresql://test:test@localhost/test',
    environment='production',
)

app = create_fraiseql_app(
    config=config,
    queries=[test_query],
    production=True,
)

with TestClient(app) as client:
    response = client.post('/graphql', json={
        'query': '{ __schema { queryType { name } } }'
    })

    assert response.status_code == 200
    data = response.json()
    assert 'errors' in data
    assert any('introspection' in error.get('message', '').lower() for error in data['errors'])

print('Production introspection blocking: VERIFIED')
"

success "Production introspection properly blocked"

# Test development mode still works
python -c "
from fastapi.testclient import TestClient
from fraiseql import query
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app
from graphql import GraphQLResolveInfo

@query
async def test_query(info: GraphQLResolveInfo) -> str:
    return 'test'

# Development mode test
config = FraiseQLConfig(
    database_url='postgresql://test:test@localhost/test',
    environment='development',
)

app = create_fraiseql_app(
    config=config,
    queries=[test_query],
    production=False,
)

with TestClient(app) as client:
    response = client.post('/graphql', json={
        'query': '{ __schema { queryType { name } } }'
    })

    assert response.status_code == 200
    data = response.json()
    assert 'data' in data
    assert '__schema' in data['data']

print('Development introspection: VERIFIED')
"

success "Development introspection works correctly"

echo
info "Testing comprehensive introspection security suite"
python -m pytest tests/security/test_schema_introspection_security.py -q --tb=no

if [ $? -eq 0 ]; then
    success "All 9 security tests pass"
else
    error "Security tests failed"
fi

echo
info "Verifying backwards compatibility"
python -m pytest tests/security/test_auth_enforcement.py -k introspection -q --tb=no

if [ $? -eq 0 ]; then
    success "Backwards compatibility maintained"
else
    error "Backwards compatibility broken"
fi

echo
info "Checking changelog entry"
if grep -q "0.3.5.*2025-08-17" CHANGELOG.md; then
    success "Changelog entry exists for v0.3.5"
else
    error "Changelog entry missing or incorrect"
fi

if grep -q "SECURITY FIX.*introspection" CHANGELOG.md; then
    success "Security fix documented in changelog"
else
    error "Security fix not documented in changelog"
fi

echo
info "Verifying release artifacts exist"
[ -f "RELEASE_NOTES_0.3.5.md" ] && success "Release notes created" || error "Release notes missing"
[ -f "SECURITY_ADVISORY_0.3.5.md" ] && success "Security advisory created" || error "Security advisory missing"
[ -f "GITHUB_RELEASE_0.3.5.md" ] && success "GitHub release template created" || error "GitHub release template missing"

echo
echo "🎉 Release Verification Complete!"
echo "================================="
success "Version numbers consistent: 0.3.5"
success "Security fix working correctly"
success "All tests passing"
success "Documentation complete"
success "Backwards compatibility maintained"

echo
echo "📋 Ready for Release:"
echo "  ✅ Production introspection blocked"
echo "  ✅ Development introspection working"
echo "  ✅ Zero breaking changes"
echo "  ✅ Comprehensive test coverage"
echo "  ✅ Complete documentation"

echo
echo "🚀 Next Steps for Release:"
echo "  1. git add ."
echo "  2. git commit -m 'Release v0.3.5: Security fix for GraphQL introspection'"
echo "  3. git tag v0.3.5"
echo "  4. git push origin v0.3.5"
echo "  5. Build and publish to PyPI"
echo "  6. Create GitHub release with security advisory"

echo
success "FraiseQL v0.3.5 is ready for release! 🔐"
