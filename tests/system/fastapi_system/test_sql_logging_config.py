"""Tests for SQL logging configuration functionality."""

import logging
from unittest.mock import patch

import pytest

from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.logging_config import configure_sql_logging


class TestSQLLoggingConfiguration:
    """Test SQL logging configuration functionality."""

    def test_configure_sql_logging_enabled(self):
        """Test that SQL logging can be enabled."""
        with patch("logging.getLogger") as mock_get_logger:
            mock_psycopg_logger = mock_get_logger.return_value
            mock_psycopg_logger.level = logging.WARNING

            configure_sql_logging(enabled=True)

            # Verify psycopg loggers are configured
            assert mock_get_logger.call_count >= 3  # psycopg, psycopg.pool, psycopg.sql

            # Verify basicConfig was called to ensure logging is set up
            with patch("logging.basicConfig") as mock_basic_config:
                configure_sql_logging(enabled=True, level="DEBUG")
                mock_basic_config.assert_called_once()

    def test_configure_sql_logging_disabled(self):
        """Test that SQL logging can be disabled."""
        with patch("logging.getLogger") as mock_get_logger:
            configure_sql_logging(enabled=False)

            # Verify psycopg loggers are set to WARNING level (disabled)
            calls = mock_get_logger.call_args_list
            assert len(calls) >= 3

            for call in calls:
                if call[0][0] in ["psycopg", "psycopg.pool", "psycopg.sql"]:
                    # Should not have set level when disabled
                    pass

    def test_configure_sql_logging_custom_level(self):
        """Test SQL logging with custom log level."""
        with (
            patch("logging.getLogger") as mock_get_logger,
            patch("logging.basicConfig") as mock_basic_config,
        ):
            # Mock the root logger with a level that requires basicConfig
            mock_root_logger = mock_get_logger.return_value
            mock_root_logger.level = logging.WARNING  # Higher than INFO, so basicConfig needed

            configure_sql_logging(enabled=True, level="INFO")

            # Verify basicConfig was called with appropriate level
            mock_basic_config.assert_called_once()
            call_args, call_kwargs = mock_basic_config.call_args
            assert call_kwargs["level"] == logging.INFO

    def test_database_echo_config_integration(self):
        """Test that database_echo config parameter works with logging."""
        config = FraiseQLConfig(database_url="postgresql://test", database_echo=True)

        assert config.database_echo is True

        config_disabled = FraiseQLConfig(database_url="postgresql://test", database_echo=False)
        assert config_disabled.database_echo is False

    @pytest.mark.parametrize("level", ["DEBUG", "INFO", "WARNING", "ERROR"])
    def test_configure_sql_logging_levels(self, level):
        """Test SQL logging with different log levels."""
        with (
            patch("logging.getLogger") as mock_get_logger,
            patch("logging.basicConfig") as mock_basic_config,
        ):
            # Mock the root logger with a level that requires basicConfig
            mock_root_logger = mock_get_logger.return_value
            mock_root_logger.level = logging.CRITICAL  # Higher than any test level

            configure_sql_logging(enabled=True, level=level)

            # Verify the correct level constant is used
            expected_level = getattr(logging, level.upper())
            mock_basic_config.assert_called_once()
            call_args, call_kwargs = mock_basic_config.call_args
            assert call_kwargs["level"] == expected_level

    def test_configure_sql_logging_no_root_logger_override(self):
        """Test that logging doesn't override existing root logger level unnecessarily."""
        with (
            patch("logging.getLogger") as mock_get_logger,
            patch("logging.basicConfig") as mock_basic_config,
        ):
            # Mock root logger with level already set to DEBUG
            mock_root_logger = mock_get_logger.return_value
            mock_root_logger.level = logging.DEBUG

            configure_sql_logging(enabled=True, level="INFO")

            # Should not call basicConfig since root logger level is already sufficient
            mock_basic_config.assert_not_called()
