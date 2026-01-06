"""Test feature flag behavior for Rust query builder.

Phase 7 Integration Tests - Feature Flags
"""

import os
from unittest.mock import patch

from fraiseql.sql.query_builder_adapter import (
    _should_use_rust,
    get_query_builder_metrics,
)


class TestFeatureFlags:
    """Test feature flag configuration for query builder selection."""

    def test_rust_disabled_by_default(self) -> None:
        """Test that Rust is disabled by default (safe default)."""
        with patch.dict(os.environ, {}, clear=True):
            # Re-import to get fresh config
            # Force reload
            import importlib

            from fraiseql import config

            importlib.reload(config)

            assert config.USE_RUST_QUERY_BUILDER is False
            assert config.RUST_QUERY_BUILDER_PERCENTAGE == 0

    def test_rust_enable_via_env(self) -> None:
        """Test enabling Rust via environment variable."""
        with patch.dict(os.environ, {"FRAISEQL_USE_RUST_QUERY_BUILDER": "true"}, clear=False):
            import importlib

            from fraiseql import config

            importlib.reload(config)

            assert config.USE_RUST_QUERY_BUILDER is True

    def test_gradual_rollout_percentage(self) -> None:
        """Test gradual rollout percentage configuration."""
        with patch.dict(os.environ, {"FRAISEQL_RUST_QB_PERCENTAGE": "50"}, clear=False):
            import importlib

            from fraiseql import config

            importlib.reload(config)

            assert config.RUST_QUERY_BUILDER_PERCENTAGE == 50

    def test_fallback_on_error_default(self) -> None:
        """Test that fallback is enabled by default."""
        with patch.dict(os.environ, {}, clear=True):
            import importlib

            from fraiseql import config

            importlib.reload(config)

            assert config.RUST_QB_FALLBACK_ON_ERROR is True


class TestQueryBuilderMetrics:
    """Test metrics collection for query builder usage."""

    def test_metrics_initialization(self) -> None:
        """Test that metrics start at zero."""
        stats = get_query_builder_metrics()

        # Should have zero calls initially (or from other tests)
        assert "rust_calls" in stats
        assert "python_calls" in stats
        assert "rust_errors" in stats
        assert "total_calls" in stats
        assert "rust_percentage" in stats

    def test_metrics_structure(self) -> None:
        """Test that metrics have expected structure."""
        stats = get_query_builder_metrics()

        expected_keys = {
            "rust_calls",
            "python_calls",
            "rust_errors",
            "rust_fallbacks",
            "total_calls",
            "rust_percentage",
            "rust_error_rate",
            "avg_rust_time_ms",
            "avg_python_time_ms",
        }

        assert set(stats.keys()) == expected_keys


class TestGradualRollout:
    """Test gradual rollout behavior."""

    @patch("fraiseql.sql.query_builder_adapter.RUST_AVAILABLE", True)
    @patch("fraiseql.sql.query_builder_adapter.USE_RUST_QUERY_BUILDER", False)
    @patch("fraiseql.sql.query_builder_adapter.RUST_QUERY_BUILDER_PERCENTAGE", 100)
    def test_100_percent_rollout(self) -> None:
        """Test that 100% rollout always uses Rust."""
        # With 100% percentage, should always return True
        assert _should_use_rust() is True

    @patch("fraiseql.sql.query_builder_adapter.RUST_AVAILABLE", True)
    @patch("fraiseql.sql.query_builder_adapter.USE_RUST_QUERY_BUILDER", False)
    @patch("fraiseql.sql.query_builder_adapter.RUST_QUERY_BUILDER_PERCENTAGE", 0)
    def test_0_percent_rollout(self) -> None:
        """Test that 0% rollout never uses Rust."""
        assert _should_use_rust() is False

    @patch("fraiseql.sql.query_builder_adapter.RUST_AVAILABLE", True)
    @patch("fraiseql.sql.query_builder_adapter.USE_RUST_QUERY_BUILDER", True)
    def test_explicit_enable_overrides_percentage(self) -> None:
        """Test that explicit enable overrides percentage."""
        # Even with 0% percentage, explicit enable should use Rust
        assert _should_use_rust() is True

    @patch("fraiseql.sql.query_builder_adapter.RUST_AVAILABLE", False)
    @patch("fraiseql.sql.query_builder_adapter.USE_RUST_QUERY_BUILDER", True)
    def test_rust_unavailable_returns_false(self) -> None:
        """Test that unavailable Rust returns False even if enabled."""
        assert _should_use_rust() is False
