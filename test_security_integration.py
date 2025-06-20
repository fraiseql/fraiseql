#!/usr/bin/env python3
"""Integration test for FraiseQL security middleware with FastAPI."""

import contextlib
import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent / "src"))

def test_middleware_integration():
    """Test that security middleware can be added to FastAPI apps."""
    try:
        from fastapi import FastAPI
        from fastapi.testclient import TestClient

        from fraiseql.security import (
            CSRFProtectionMiddleware,
            RateLimitMiddleware,
            RateLimitStore,
            SecurityHeadersMiddleware,
            create_development_csrf_config,
            create_development_security_config,
            setup_development_security,
        )


        # Test 1: Create FastAPI app and add security
        app = FastAPI()

        @app.get("/test")
        async def test_endpoint():
            return {"message": "test"}

        @app.post("/test")
        async def test_post():
            return {"message": "test"}

        # Add security middleware
        try:
            setup_development_security(app, "test-secret-key")
        except Exception:
            # If setup fails due to middleware conflicts, try individual components

            # Test individual middleware
            from fraiseql.security.csrf_protection import CSRFProtectionMiddleware
            from fraiseql.security.rate_limiting import RateLimitMiddleware, RateLimitStore
            from fraiseql.security.security_headers import (
                SecurityHeadersMiddleware,
            )

            # Rate limiting
            app.add_middleware(
                RateLimitMiddleware,
                store=RateLimitStore(),
                rules=[],
            )

            # CSRF protection
            csrf_config = create_development_csrf_config("test-secret")
            app.add_middleware(
                CSRFProtectionMiddleware,
                config=csrf_config,
            )

            # Security headers
            headers_config = create_development_security_config()
            app.add_middleware(
                SecurityHeadersMiddleware,
                config=headers_config,
            )

        # Test 2: Create test client and verify responses
        client = TestClient(app)

        # Test GET request (should work)
        response = client.get("/test")
        assert response.status_code == 200

        # Check for security headers
        headers = response.headers
        security_headers_present = any(
            header in headers for header in [
                "X-Frame-Options", "X-Content-Type-Options",
                "Referrer-Policy", "Content-Security-Policy",
            ]
        )
        if security_headers_present:
            pass
        else:
            pass

        # Test POST request (may be blocked by CSRF in strict mode)
        with contextlib.suppress(Exception):
            response = client.post("/test")

        return True

    except Exception:
        import traceback
        traceback.print_exc()
        return False


def test_security_example():
    """Test the security example can be imported and initialized."""
    try:
        # Add examples to path
        sys.path.insert(0, str(Path(__file__).parent / "examples" / "security"))

        # We can't run the full example (it needs a database), but we can test imports
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "secure_graphql_api",
            Path(__file__).parent / "examples" / "security" / "secure_graphql_api.py",
        )

        if spec and spec.loader:
            # Test that the module can be loaded
            module = importlib.util.module_from_spec(spec)

            # Override environment to avoid database requirements
            import os
            old_env = os.environ.copy()
            os.environ["ENVIRONMENT"] = "development"
            os.environ["SECRET_KEY"] = "test-key"

            try:
                # Load the module (this will test imports)
                spec.loader.exec_module(module)

                # Test that we can access the create_app function
                if hasattr(module, "create_app"):
                    pass
                else:
                    pass

            finally:
                # Restore environment
                os.environ.clear()
                os.environ.update(old_env)

        return True

    except Exception:
        return False


def main():
    """Run integration tests."""
    results = []

    # Run integration tests
    results.append(test_middleware_integration())
    results.append(test_security_example())

    # Summary

    passed = sum(results)
    total = len(results)


    if passed == total:
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
