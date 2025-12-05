"""Tests for APQ mode configuration."""

import pytest

pytestmark = pytest.mark.integration


class TestAPQModeEnum:
    """Tests for APQMode enum."""

    def test_apq_mode_enum_exists(self) -> None:
        """Test APQMode enum is importable."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode is not None

    def test_apq_mode_has_optional_value(self) -> None:
        """Test APQMode has 'optional' value."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode.OPTIONAL == "optional"
        assert APQMode.OPTIONAL.value == "optional"

    def test_apq_mode_has_required_value(self) -> None:
        """Test APQMode has 'required' value."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode.REQUIRED == "required"
        assert APQMode.REQUIRED.value == "required"

    def test_apq_mode_has_disabled_value(self) -> None:
        """Test APQMode has 'disabled' value."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode.DISABLED == "disabled"
        assert APQMode.DISABLED.value == "disabled"

    def test_apq_mode_string_comparison(self) -> None:
        """Test APQMode values can be compared to strings."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode.OPTIONAL == "optional"
        assert APQMode.REQUIRED == "required"
        assert APQMode.DISABLED == "disabled"


class TestAPQModeConfig:
    """Tests for apq_mode in FraiseQLConfig."""

    def test_apq_mode_default_is_optional(self) -> None:
        """Test apq_mode defaults to 'optional'."""
        from fraiseql.fastapi.config import APQMode, FraiseQLConfig

        config = FraiseQLConfig(database_url="postgresql://localhost/test")
        assert config.apq_mode == APQMode.OPTIONAL

    def test_apq_mode_can_be_set_to_required(self) -> None:
        """Test apq_mode can be set to 'required'."""
        from fraiseql.fastapi.config import APQMode, FraiseQLConfig

        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            apq_mode="required",
        )
        assert config.apq_mode == APQMode.REQUIRED

    def test_apq_mode_can_be_set_to_disabled(self) -> None:
        """Test apq_mode can be set to 'disabled'."""
        from fraiseql.fastapi.config import APQMode, FraiseQLConfig

        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            apq_mode="disabled",
        )
        assert config.apq_mode == APQMode.DISABLED

    def test_apq_mode_accepts_enum_value(self) -> None:
        """Test apq_mode accepts APQMode enum directly."""
        from fraiseql.fastapi.config import APQMode, FraiseQLConfig

        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            apq_mode=APQMode.REQUIRED,
        )
        assert config.apq_mode == APQMode.REQUIRED

    def test_apq_mode_invalid_value_raises_error(self) -> None:
        """Test invalid apq_mode raises validation error."""
        from pydantic import ValidationError

        from fraiseql.fastapi.config import FraiseQLConfig

        with pytest.raises(ValidationError):
            FraiseQLConfig(
                database_url="postgresql://localhost/test",
                apq_mode="invalid_mode",
            )


class TestAPQModeHelpers:
    """Tests for APQMode helper methods."""

    def test_allows_arbitrary_queries_optional(self) -> None:
        """Test optional mode allows arbitrary queries."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode.OPTIONAL.allows_arbitrary_queries() is True

    def test_allows_arbitrary_queries_required(self) -> None:
        """Test required mode does NOT allow arbitrary queries."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode.REQUIRED.allows_arbitrary_queries() is False

    def test_allows_arbitrary_queries_disabled(self) -> None:
        """Test disabled mode allows arbitrary queries (APQ ignored)."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode.DISABLED.allows_arbitrary_queries() is True

    def test_processes_apq_optional(self) -> None:
        """Test optional mode processes APQ requests."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode.OPTIONAL.processes_apq() is True

    def test_processes_apq_required(self) -> None:
        """Test required mode processes APQ requests."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode.REQUIRED.processes_apq() is True

    def test_processes_apq_disabled(self) -> None:
        """Test disabled mode does NOT process APQ requests."""
        from fraiseql.fastapi.config import APQMode

        assert APQMode.DISABLED.processes_apq() is False
