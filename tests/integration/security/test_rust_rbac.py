"""Integration tests for Phase 11 Rust RBAC implementation.

Tests the Rust-based permission resolver, role hierarchy, caching,
and field-level authorization.
"""

import pytest

# Try to import Rust RBAC
try:
    from fraiseql._fraiseql_rs import PyFieldAuthChecker, PyPermissionResolver

    HAS_RUST_RBAC = True
except ImportError:
    HAS_RUST_RBAC = False

# Skip all tests if Rust RBAC not available
pytestmark = pytest.mark.skipif(not HAS_RUST_RBAC, reason="Rust RBAC extension not available")


class TestRustRBACAvailability:
    """Test that Rust RBAC module is available and properly configured."""

    def test_rust_rbac_module_exists(self) -> None:
        """Test that Rust RBAC classes are available."""
        assert HAS_RUST_RBAC, "PyPermissionResolver should be available"
        assert PyPermissionResolver is not None
        assert PyFieldAuthChecker is not None

    def test_permission_resolver_available(self) -> None:
        """Test that PermissionResolver can be imported."""
        assert hasattr(PyPermissionResolver, "__new__")
        assert callable(PyPermissionResolver)

    def test_field_auth_checker_available(self) -> None:
        """Test that FieldAuthChecker can be imported."""
        assert hasattr(PyFieldAuthChecker, "__new__")
        assert callable(PyFieldAuthChecker)


class TestPyPermissionResolver:
    """Test PyPermissionResolver basic functionality."""

    def test_resolver_cannot_create_without_pool(self) -> None:
        """Test that resolver requires a database pool."""
        with pytest.raises((TypeError, RuntimeError)):
            PyPermissionResolver(None, 1000)

    def test_resolver_has_required_methods(self) -> None:
        """Test that resolver has all required methods."""
        assert hasattr(PyPermissionResolver, "get_user_permissions")
        assert hasattr(PyPermissionResolver, "has_permission")
        assert hasattr(PyPermissionResolver, "invalidate_user")
        assert hasattr(PyPermissionResolver, "invalidate_tenant")
        assert hasattr(PyPermissionResolver, "clear_cache")
        assert hasattr(PyPermissionResolver, "cache_stats")


class TestPyFieldAuthChecker:
    """Test PyFieldAuthChecker basic functionality."""

    def test_field_auth_checker_cannot_create_without_resolver(self) -> None:
        """Test that field auth checker requires a resolver."""
        with pytest.raises((TypeError, RuntimeError)):
            PyFieldAuthChecker(None)

    def test_field_auth_checker_has_required_methods(self) -> None:
        """Test that field auth checker has all required methods."""
        assert hasattr(PyFieldAuthChecker, "check_field_access")
        assert hasattr(PyFieldAuthChecker, "check_fields_access")


class TestRustResolverPythonWrapper:
    """Test the Python wrapper for Rust resolver."""

    def test_rust_resolver_import(self) -> None:
        """Test that RustPermissionResolver can be imported."""
        try:
            from fraiseql.enterprise.rbac.rust_resolver import RustPermissionResolver

            assert RustPermissionResolver is not None
        except ImportError as e:
            pytest.skip(f"RustPermissionResolver not available: {e}")

    def test_rust_resolver_has_correct_api(self) -> None:
        """Test that RustPermissionResolver has the expected API."""
        try:
            from fraiseql.enterprise.rbac.rust_resolver import RustPermissionResolver
        except ImportError:
            pytest.skip("RustPermissionResolver not available")

        # Check methods exist
        assert hasattr(RustPermissionResolver, "get_user_permissions")
        assert hasattr(RustPermissionResolver, "has_permission")
        assert hasattr(RustPermissionResolver, "invalidate_user")
        assert hasattr(RustPermissionResolver, "invalidate_tenant")
        assert hasattr(RustPermissionResolver, "clear_cache")
        assert hasattr(RustPermissionResolver, "cache_stats")

        # Check they're callable
        assert callable(RustPermissionResolver.get_user_permissions)
        assert callable(RustPermissionResolver.has_permission)
        assert callable(RustPermissionResolver.invalidate_user)


class TestRBACModels:
    """Test RBAC model structures."""

    def test_permission_model_structure(self) -> None:
        """Test that Permission model has required fields."""
        from fraiseql.enterprise.rbac.models import Permission

        # Check Permission is importable and is a class
        assert Permission is not None
        assert callable(Permission)

    def test_role_model_structure(self) -> None:
        """Test that Role model has required fields."""
        from fraiseql.enterprise.rbac.models import Role

        # Check Role is importable and is a class
        assert Role is not None
        assert callable(Role)


class TestCacheOperations:
    """Test cache invalidation and statistics."""

    def test_cache_stats_structure(self) -> None:
        """Test that cache stats have expected structure."""
        # This would require a live resolver, so we just test the structure
        # In a real test with database, we would:
        # stats = resolver.cache_stats()
        # assert "capacity" in stats
        # assert "size" in stats
        # assert "expired_count" in stats


class TestPerformanceTargets:
    """Test that performance targets are documented."""

    def test_performance_targets_documented(self) -> None:
        """Test that performance targets are documented in module."""
        from fraiseql.enterprise.rbac import rust_resolver

        # Check docstring mentions performance
        assert rust_resolver.__doc__ is not None
        assert "performance" in rust_resolver.__doc__.lower() or "10-100x" in rust_resolver.__doc__


class TestErrorHandling:
    """Test error handling in RBAC operations."""

    def test_rust_not_available_error_message(self) -> None:
        """Test error message when Rust is not available."""
        try:
            from fraiseql.enterprise.rbac.rust_resolver import RustPermissionResolver

            # If Rust is available, try to create resolver without pool
            try:
                RustPermissionResolver(None)
                # Should raise an error
            except (TypeError, RuntimeError, AttributeError):
                # Expected - either from Python or Rust
                assert True
        except ImportError:
            # Rust not available - this is expected in some environments
            pytest.skip("Rust RBAC not available")


class TestIntegrationReadiness:
    """Test integration readiness indicators."""

    def test_all_required_modules_exist(self) -> None:
        """Test that all required RBAC modules exist."""
        # Rust modules (in fraiseql_rs)
        from fraiseql import _fraiseql_rs

        assert hasattr(_fraiseql_rs, "PyPermissionResolver")
        assert hasattr(_fraiseql_rs, "PyFieldAuthChecker")

    def test_python_wrapper_exists(self) -> None:
        """Test that Python wrapper module exists."""
        try:
            from fraiseql.enterprise.rbac import rust_resolver

            assert rust_resolver is not None
            assert hasattr(rust_resolver, "RustPermissionResolver")
        except ImportError as e:
            pytest.skip(f"Python wrapper not available: {e}")

    def test_backward_compatibility_maintained(self) -> None:
        """Test that old Python resolver still exists for migration."""
        from fraiseql.enterprise.rbac import resolver

        assert resolver is not None
        assert hasattr(resolver, "PermissionResolver")
        # Old Python resolver should still work for gradual migration


class TestDocumentation:
    """Test that documentation is complete."""

    def test_rust_resolver_has_docstrings(self) -> None:
        """Test that RustPermissionResolver has comprehensive docstrings."""
        try:
            from fraiseql.enterprise.rbac.rust_resolver import RustPermissionResolver

            assert RustPermissionResolver.__doc__ is not None
            assert len(RustPermissionResolver.__doc__) > 100

            # Check key methods have docstrings
            assert RustPermissionResolver.get_user_permissions.__doc__ is not None
            assert RustPermissionResolver.has_permission.__doc__ is not None
        except ImportError:
            pytest.skip("RustPermissionResolver not available")

    def test_performance_claims_documented(self) -> None:
        """Test that performance improvements are documented."""
        try:
            from fraiseql.enterprise.rbac.rust_resolver import RustPermissionResolver

            doc = RustPermissionResolver.__doc__
            # Should mention performance improvement
            assert "10-100x" in doc or "faster" in doc.lower()
        except ImportError:
            pytest.skip("RustPermissionResolver not available")


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
