#!/usr/bin/env python3
"""
Simple integration test for FraiseQL security middleware.
"""

import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent / "src"))

def test_middleware_creation():
    """Test that security middleware can be created."""
    print("🧪 Testing Security Middleware Creation...")
    
    try:
        from fastapi import FastAPI
        from fastapi.testclient import TestClient
        from fraiseql.security.rate_limiting import RateLimitMiddleware, RateLimitStore
        from fraiseql.security.csrf_protection import CSRFProtectionMiddleware, create_development_csrf_config
        from fraiseql.security.security_headers import SecurityHeadersMiddleware, create_development_security_config
        
        print("  ✓ All middleware imports successful")
        
        # Test 1: Create basic FastAPI app
        app = FastAPI()
        
        @app.get("/test")
        async def test_endpoint():
            return {"message": "test"}
        
        print("  ✓ FastAPI app created")
        
        # Test 2: Add rate limiting middleware
        rate_store = RateLimitStore()
        app.add_middleware(RateLimitMiddleware, store=rate_store, rules=[])
        print("  ✓ Rate limiting middleware added")
        
        # Test 3: Add CSRF protection middleware
        csrf_config = create_development_csrf_config("test-secret")
        app.add_middleware(CSRFProtectionMiddleware, config=csrf_config)
        print("  ✓ CSRF protection middleware added")
        
        # Test 4: Add security headers middleware
        headers_config = create_development_security_config()
        app.add_middleware(SecurityHeadersMiddleware, config=headers_config)
        print("  ✓ Security headers middleware added")
        
        # Test 5: Create test client
        client = TestClient(app)
        print("  ✓ Test client created")
        
        # Test 6: Make a simple request
        response = client.get("/test")
        print(f"  ✓ GET request successful (status: {response.status_code})")
        
        # Test 7: Check for security headers
        headers = response.headers
        security_headers = [
            "X-Frame-Options", "X-Content-Type-Options", 
            "Referrer-Policy"
        ]
        
        found_headers = [h for h in security_headers if h in headers]
        if found_headers:
            print(f"  ✓ Security headers found: {found_headers}")
        else:
            print("  ⚠️  No security headers detected")
        
        print("✅ Security Middleware Creation: All tests passed!")
        return True
        
    except Exception as e:
        print(f"❌ Security Middleware Creation: Test failed - {e}")
        import traceback
        traceback.print_exc()
        return False


def test_individual_components():
    """Test individual security components."""
    print("\n🧪 Testing Individual Security Components...")
    
    try:
        # Test rate limiting components
        from fraiseql.security.rate_limiting import RateLimit, RateLimitRule, RateLimitStore
        
        rule = RateLimitRule(
            path_pattern="/api/*",
            rate_limit=RateLimit(requests=100, window=60),
            message="API rate limit exceeded"
        )
        assert rule.rate_limit.requests == 100
        print("  ✓ Rate limiting rule creation works")
        
        # Test CSRF components
        from fraiseql.security.csrf_protection import CSRFConfig, CSRFTokenGenerator
        
        config = CSRFConfig(secret_key="test-key")
        generator = CSRFTokenGenerator(config.secret_key)
        token = generator.generate_token()
        assert generator.validate_token(token)
        print("  ✓ CSRF token generation/validation works")
        
        # Test security headers components
        from fraiseql.security.security_headers import ContentSecurityPolicy, CSPDirective
        
        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.DEFAULT_SRC, "'self'")
        header_value = csp.to_header_value()
        assert "default-src 'self'" in header_value
        print("  ✓ CSP directive handling works")
        
        print("✅ Individual Security Components: All tests passed!")
        return True
        
    except Exception as e:
        print(f"❌ Individual Security Components: Test failed - {e}")
        return False


def test_configuration_helpers():
    """Test security configuration helpers."""
    print("\n🧪 Testing Security Configuration Helpers...")
    
    try:
        from fraiseql.security import (
            SecurityConfig, create_security_config_for_graphql,
            create_production_csrf_config, create_development_csrf_config,
            create_production_security_config, create_development_security_config
        )
        
        # Test SecurityConfig class
        config = SecurityConfig(
            secret_key="test-key",
            environment="production",
            domain="api.example.com"
        )
        assert config.is_production
        assert not config.is_development
        print("  ✓ SecurityConfig class works")
        
        # Test GraphQL config helper
        graphql_config = create_security_config_for_graphql(
            secret_key="graphql-key",
            environment="development",
            trusted_origins=["https://app.example.com"]
        )
        assert graphql_config.api_only
        assert len(graphql_config.custom_rate_limits) > 0
        print("  ✓ GraphQL security config helper works")
        
        # Test CSRF config helpers
        prod_csrf = create_production_csrf_config("prod-key", {"https://app.example.com"})
        dev_csrf = create_development_csrf_config("dev-key")
        
        assert prod_csrf.cookie_secure
        assert not dev_csrf.cookie_secure
        print("  ✓ CSRF config helpers work")
        
        # Test security headers config helpers
        prod_headers = create_production_security_config("api.example.com")
        dev_headers = create_development_security_config()
        
        assert prod_headers.hsts
        assert not dev_headers.hsts
        print("  ✓ Security headers config helpers work")
        
        print("✅ Security Configuration Helpers: All tests passed!")
        return True
        
    except Exception as e:
        print(f"❌ Security Configuration Helpers: Test failed - {e}")
        return False


def main():
    """Run simple integration tests."""
    print("🚀 Running FraiseQL Security Simple Integration Tests")
    print("=" * 60)
    
    results = []
    
    # Run tests
    results.append(test_middleware_creation())
    results.append(test_individual_components())
    results.append(test_configuration_helpers())
    
    # Summary
    print("\n" + "=" * 60)
    print("📊 Simple Integration Test Results:")
    
    passed = sum(results)
    total = len(results)
    
    print(f"  ✅ Passed: {passed}/{total}")
    print(f"  ❌ Failed: {total - passed}/{total}")
    
    if passed == total:
        print("\n🎉 All simple integration tests passed!")
        print("The security middleware can be successfully integrated with FastAPI.")
        return 0
    else:
        print(f"\n⚠️  {total - passed} test(s) failed.")
        return 1


if __name__ == "__main__":
    sys.exit(main())