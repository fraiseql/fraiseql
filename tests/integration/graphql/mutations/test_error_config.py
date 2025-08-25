import pytest

"""Tests for configurable mutation error detection."""

from fraiseql.mutations.error_config import (
    ALWAYS_DATA_CONFIG,
    DEFAULT_ERROR_CONFIG,
    STRICT_STATUS_CONFIG,
    MutationErrorConfig,
)


@pytest.mark.unit
class TestMutationErrorConfig:
    """Test error configuration functionality."""

    def test_default_config_error_detection(self) -> None:
        """Test default error detection behavior."""
        config = DEFAULT_ERROR_CONFIG

        # Success statuses
        assert not config.is_error_status("success")
        assert not config.is_error_status("completed")
        assert not config.is_error_status("ok")
        assert not config.is_error_status("done")
        assert not config.is_error_status("new")
        assert not config.is_error_status("existing")
        assert not config.is_error_status("updated")
        assert not config.is_error_status("deleted")
        assert not config.is_error_status("synced")

        # Error statuses
        assert config.is_error_status("error")
        assert config.is_error_status("failed")
        assert config.is_error_status("failed:validation")
        assert config.is_error_status("error:not_found")
        assert config.is_error_status("invalid")
        assert config.is_error_status("validation_error")

        # Noop statuses (treated as errors by default)
        assert not config.is_error_status("noop:unchanged")
        assert not config.is_error_status("noop:invalid_contract_id")
        assert not config.is_error_status("blocked:children")

    def test_strict_status_config(self) -> None:
        """Test strict status-based configuration."""
        config = STRICT_STATUS_CONFIG

        # Success statuses
        assert not config.is_error_status("success")
        assert not config.is_error_status("new")
        assert not config.is_error_status("existing")
        assert not config.is_error_status("updated")
        assert not config.is_error_status("deleted")
        assert not config.is_error_status("synced")

        # Only failed: prefix triggers errors
        assert config.is_error_status("failed:validation")
        assert config.is_error_status("failed:not_found")
        assert config.is_error_status("failed:database_error")

        # Other statuses are NOT errors (returned as data)
        assert not config.is_error_status("noop:unchanged")
        assert not config.is_error_status("noop:invalid_contract_id")
        assert not config.is_error_status("noop:existing")
        assert not config.is_error_status("blocked:children")
        assert not config.is_error_status("error")  # Generic 'error' is not an error
        assert not config.is_error_status("invalid")  # Generic 'invalid' is not an error

    def test_always_data_config(self) -> None:
        """Test configuration that always returns data."""
        config = ALWAYS_DATA_CONFIG

        # Nothing is ever an error
        assert not config.is_error_status("failed:validation")
        assert not config.is_error_status("error:critical")
        assert not config.is_error_status("invalid")
        assert not config.is_error_status("timeout")
        assert not config.is_error_status("")
        assert not config.is_error_status("any_status_at_all")

    def test_custom_config(self) -> None:
        """Test custom error configuration."""
        config = MutationErrorConfig(
            success_keywords={"ok", "good"},
            error_prefixes={"bad:", "terrible:"},
            error_as_data_prefixes={"maybe:", "warn:"},
            error_keywords={"broken", "busted"},
        )

        # Success keywords
        assert not config.is_error_status("ok")
        assert not config.is_error_status("good")

        # Error prefixes
        assert config.is_error_status("bad:input")
        assert config.is_error_status("terrible:failure")

        # Error as data prefixes (NOT errors)
        assert not config.is_error_status("maybe:invalid")
        assert not config.is_error_status("warn:deprecated")

        # Error keywords
        assert config.is_error_status("broken")
        assert config.is_error_status("something_busted")
        assert config.is_error_status("totally broken")

    def test_regex_pattern_config(self) -> None:
        """Test configuration with regex pattern."""
        import re

        config = MutationErrorConfig(
            success_keywords=set(),
            error_prefixes=set(),
            error_keywords=set(),
            # Only exact pattern "failed:word_word" is an error
            error_pattern=re.compile(r"^failed:[a-z_]+$"),
        )

        # Matches pattern
        assert config.is_error_status("failed:validation")
        assert config.is_error_status("failed:not_found")
        assert config.is_error_status("failed:database_error")

        # Doesn't match pattern
        assert not config.is_error_status("failed")
        assert not config.is_error_status("failed:")
        assert not config.is_error_status("failed:UPPERCASE")
        assert not config.is_error_status("failed:has-dash")
        assert not config.is_error_status("prefixfailed:validation")

    def test_empty_status_handling(self) -> None:
        """Test handling of empty or None statuses."""
        config = DEFAULT_ERROR_CONFIG

        assert not config.is_error_status("")
        assert not config.is_error_status(None)  # type: ignore

    def test_case_insensitive_matching(self) -> None:
        """Test that status matching is case-insensitive."""
        config = DEFAULT_ERROR_CONFIG

        assert config.is_error_status("ERROR")
        assert config.is_error_status("Failed")
        assert config.is_error_status("FAILED:VALIDATION")
        assert not config.is_error_status("SUCCESS")
        assert not config.is_error_status("OK")
