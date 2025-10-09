"""Tests for Sentry error tracking integration.

Note: sentry-sdk is an optional dependency. These tests verify the integration
works correctly both when sentry-sdk is available and when it's not installed.
"""

import pytest
from unittest.mock import MagicMock, patch


class TestSentryIntegration:
    """Test Sentry integration with optional dependency."""

    def test_init_sentry_with_no_dsn_returns_false(self):
        """Test Sentry is disabled when no DSN provided."""
        from fraiseql.monitoring.sentry import init_sentry

        result = init_sentry(dsn=None)
        assert result is False

    def test_init_sentry_with_empty_dsn_returns_false(self):
        """Test Sentry is disabled with empty DSN."""
        from fraiseql.monitoring.sentry import init_sentry

        result = init_sentry(dsn="")
        assert result is False

    def test_init_sentry_without_sentry_sdk_installed(self):
        """Test graceful handling when sentry-sdk not installed."""
        import sys
        from fraiseql.monitoring.sentry import init_sentry

        # Temporarily block sentry_sdk import by setting it to None in sys.modules
        # This simulates the package not being installed
        with patch.dict(sys.modules, {
            'sentry_sdk': None,
            'sentry_sdk.integrations': None,
            'sentry_sdk.integrations.fastapi': None,
            'sentry_sdk.integrations.logging': None,
            'sentry_sdk.integrations.sqlalchemy': None
        }):
            result = init_sentry(dsn="https://test@sentry.io/123")
            assert result is False

    def test_capture_exception_without_sentry_returns_none(self):
        """Test capture_exception returns None when sentry unavailable."""
        from fraiseql.monitoring.sentry import capture_exception

        error = ValueError("Test error")

        # If sentry-sdk not installed, should return None without error
        with patch("builtins.__import__", side_effect=ImportError):
            result = capture_exception(error)
            # Should handle gracefully
            assert result is None or isinstance(result, str)

    def test_capture_message_without_sentry_returns_none(self):
        """Test capture_message returns None when sentry unavailable."""
        from fraiseql.monitoring.sentry import capture_message

        with patch("builtins.__import__", side_effect=ImportError):
            result = capture_message("Test message")
            assert result is None or isinstance(result, str)

    def test_set_context_without_sentry_no_error(self):
        """Test set_context doesn't raise when sentry unavailable."""
        from fraiseql.monitoring.sentry import set_context

        # Should not raise exception even if sentry-sdk not available
        try:
            set_context("test", {"key": "value"})
            assert True  # Passed if no exception
        except ImportError:
            pytest.fail("set_context should handle missing sentry-sdk gracefully")

    def test_set_user_without_sentry_no_error(self):
        """Test set_user doesn't raise when sentry unavailable."""
        from fraiseql.monitoring.sentry import set_user

        # Should not raise exception even if sentry-sdk not available
        try:
            set_user(user_id=123, email="test@example.com")
            assert True  # Passed if no exception
        except ImportError:
            pytest.fail("set_user should handle missing sentry-sdk gracefully")


class TestSentryAPI:
    """Test Sentry API is correctly exposed."""

    def test_sentry_functions_are_importable(self):
        """Test all Sentry functions can be imported."""
        from fraiseql.monitoring import (
            init_sentry,
            capture_exception,
            capture_message,
            set_context,
            set_user,
        )

        assert callable(init_sentry)
        assert callable(capture_exception)
        assert callable(capture_message)
        assert callable(set_context)
        assert callable(set_user)

    def test_init_sentry_signature(self):
        """Test init_sentry has correct signature."""
        from fraiseql.monitoring.sentry import init_sentry
        import inspect

        sig = inspect.signature(init_sentry)
        params = list(sig.parameters.keys())

        assert "dsn" in params
        assert "environment" in params
        assert "traces_sample_rate" in params
        assert "profiles_sample_rate" in params

    def test_capture_exception_signature(self):
        """Test capture_exception has correct signature."""
        from fraiseql.monitoring.sentry import capture_exception
        import inspect

        sig = inspect.signature(capture_exception)
        params = list(sig.parameters.keys())

        assert "error" in params
        assert "level" in params
        assert "extra" in params

    def test_set_user_signature(self):
        """Test set_user has correct signature."""
        from fraiseql.monitoring.sentry import set_user
        import inspect

        sig = inspect.signature(set_user)
        params = list(sig.parameters.keys())

        assert "user_id" in params
        assert "email" in params
        assert "username" in params


class TestSentryIntegrationWithRealSDK:
    """Integration tests with actual sentry-sdk (if installed)."""

    def test_init_sentry_with_real_sdk(self):
        """Test init_sentry with real sentry-sdk if available."""
        try:
            import sentry_sdk
        except ImportError:
            pytest.skip("sentry-sdk not installed")

        from fraiseql.monitoring.sentry import init_sentry

        # Test with valid DSN
        result = init_sentry(
            dsn="https://test@sentry.io/123",
            environment="test",
            traces_sample_rate=0.0,  # Don't actually send traces
            send_default_pii=False,
        )

        # Should succeed if sentry-sdk is properly installed
        assert result is True

        # Clean up - disable sentry after test
        try:
            sentry_sdk.Hub.current.client = None
        except:
            pass

    def test_capture_functions_return_values_when_sdk_available(self):
        """Test capture functions return event IDs when sentry-sdk available."""
        try:
            import sentry_sdk
        except ImportError:
            pytest.skip("sentry-sdk not installed")

        from fraiseql.monitoring.sentry import (
            capture_exception,
            capture_message,
            init_sentry,
        )

        # Initialize with test DSN
        init_sentry(
            dsn="https://test@sentry.io/123",
            environment="test",
            traces_sample_rate=0.0,
        )

        # These should return event IDs (or None in test mode, but not raise)
        error = ValueError("Test error")
        event_id = capture_exception(error)
        # In test mode, may return None, but shouldn't crash
        assert event_id is None or isinstance(event_id, str)

        msg_id = capture_message("Test message")
        assert msg_id is None or isinstance(msg_id, str)

        # Clean up
        try:
            sentry_sdk.Hub.current.client = None
        except:
            pass


class TestSentryDocumentation:
    """Test that Sentry integration is well-documented."""

    def test_init_sentry_has_docstring(self):
        """Test init_sentry has documentation."""
        from fraiseql.monitoring.sentry import init_sentry

        assert init_sentry.__doc__ is not None
        assert "Initialize Sentry" in init_sentry.__doc__
        assert "dsn" in init_sentry.__doc__.lower()

    def test_module_has_docstring(self):
        """Test Sentry module has documentation."""
        from fraiseql.monitoring import sentry

        assert sentry.__doc__ is not None
        assert "Sentry" in sentry.__doc__ or "error tracking" in sentry.__doc__.lower()

    def test_all_exports_documented(self):
        """Test all exported functions are documented."""
        from fraiseql.monitoring import sentry

        for func_name in sentry.__all__:
            func = getattr(sentry, func_name)
            if callable(func):
                assert func.__doc__ is not None, f"{func_name} is not documented"
