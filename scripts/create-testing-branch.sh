#!/bin/bash
# Create a testing branch for CamelForge feature

set -e

echo "🚀 Creating CamelForge testing branch..."

# Create feature branch
git checkout -b feature/camelforge-integration

# Add testing utilities
echo "📝 Adding testing utilities..."

cat > test-camelforge.sh << 'EOF'
#!/bin/bash
# Quick CamelForge testing script

echo "🧪 Testing CamelForge Integration"
echo "================================"

# Test 1: Verify no breaking changes
echo "✅ Test 1: Backward compatibility (CamelForge disabled)"
export FRAISEQL_CAMELFORGE_BETA=false
python -m pytest tests/field_threshold/ -v

# Test 2: Basic CamelForge functionality
echo "✅ Test 2: CamelForge enabled (basic)"
export FRAISEQL_CAMELFORGE_BETA=true
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server
python -m pytest tests/field_threshold/test_camelforge_integration.py -v

# Test 3: Full CamelForge test suite
echo "✅ Test 3: Full CamelForge test suite"
unset FRAISEQL_CAMELFORGE_ALLOWLIST
python -m pytest tests/field_threshold/ -k camelforge -v

echo "🎉 All tests completed!"
echo "To enable CamelForge in your app:"
echo "export FRAISEQL_CAMELFORGE_BETA=true"
EOF

chmod +x test-camelforge.sh

# Add demo configuration
cat > demo-camelforge-config.py << 'EOF'
"""Demo configuration for testing CamelForge."""

from fraiseql.fastapi.config import FraiseQLConfig

# Safe testing configuration
safe_camelforge_config = FraiseQLConfig(
    database_url="your-database-url-here",

    # CamelForge settings (safe defaults)
    camelforge_enabled=True,
    camelforge_function="turbo.fn_camelforge",
    camelforge_entity_mapping=True,
    jsonb_field_limit_threshold=20,

    # Feature flags for safety
    enable_feature_flags=True,
    feature_flags_source="environment",

    # Development settings
    environment="development",
    enable_playground=True,
)

# Performance testing configuration
performance_camelforge_config = FraiseQLConfig(
    database_url="your-database-url-here",

    # Optimized for performance testing
    camelforge_enabled=True,
    camelforge_function="turbo.fn_camelforge",
    jsonb_field_limit_threshold=50,  # Higher threshold for more CamelForge usage

    # Production-like settings
    environment="production",
    enable_playground=False,
)
EOF

# Add testing documentation
cat > TESTING_INSTRUCTIONS.md << 'EOF'
# CamelForge Testing Instructions

## Quick Start

1. **Run the test script:**
   ```bash
   ./test-camelforge.sh
   ```

2. **Test with your application:**
   ```bash
   # Enable CamelForge for testing
   export FRAISEQL_CAMELFORGE_BETA=true
   export FRAISEQL_CAMELFORGE_DEBUG=true

   # Start your app
   python your_app.py
   ```

3. **Test specific entities:**
   ```bash
   # Only test dns_server entities
   export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server
   ```

## What to Test

- ✅ All existing GraphQL queries still work
- ✅ Response format is identical to before
- ✅ Performance is same or better
- ✅ No errors in logs
- ✅ Complex queries (50+ fields) still work

## Rollback if Needed

```bash
export FRAISEQL_CAMELFORGE_BETA=false
# or restart your app
```
EOF

git add .
git commit -m "Add CamelForge testing utilities and documentation"

echo "✅ Testing branch created: feature/camelforge-integration"
echo "📋 Testing files added:"
echo "   - test-camelforge.sh (quick test script)"
echo "   - demo-camelforge-config.py (example configurations)"
echo "   - TESTING_INSTRUCTIONS.md (step-by-step guide)"
echo ""
echo "🎯 Next steps for the other team:"
echo "1. git checkout feature/camelforge-integration"
echo "2. ./test-camelforge.sh"
echo "3. Follow TESTING_INSTRUCTIONS.md"
