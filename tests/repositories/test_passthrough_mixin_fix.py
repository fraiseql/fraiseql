"""Test that PassthroughMixin respects json_passthrough configuration."""

import pytest
from unittest.mock import MagicMock, AsyncMock

from fraiseql.repositories.passthrough_mixin import PassthroughMixin
from fraiseql.core.raw_json_executor import RawJSONResult


class BaseRepository:
    """Base repository for testing."""

    async def find(self, *args, **kwargs):
        """Mock find method."""
        return [{"id": 1, "name": "test"}]

    async def find_one(self, *args, **kwargs):
        """Mock find_one method."""
        return {"id": 1, "name": "test"}


class MockRepository(PassthroughMixin, BaseRepository):
    """Mock repository with PassthroughMixin."""
    pass


class TestPassthroughMixinFix:
    """Test that PassthroughMixin correctly respects json_passthrough configuration."""

    def test_production_mode_does_not_force_passthrough(self):
        """Test that production mode alone doesn't enable passthrough."""
        repo = MockRepository()

        # Set context with production mode but json_passthrough=False
        repo.context = {
            "mode": "production",
            "json_passthrough": False,
            "execution_mode": None,
        }

        # Should NOT use passthrough
        assert repo._should_use_passthrough() is False

    def test_staging_mode_does_not_force_passthrough(self):
        """Test that staging mode alone doesn't enable passthrough."""
        repo = MockRepository()

        # Set context with staging mode but json_passthrough=False
        repo.context = {
            "mode": "staging",
            "json_passthrough": False,
        }

        # Should NOT use passthrough
        assert repo._should_use_passthrough() is False

    def test_passthrough_enabled_when_json_passthrough_true(self):
        """Test that passthrough is enabled when json_passthrough=True."""
        repo = MockRepository()

        # Set context with json_passthrough=True
        repo.context = {
            "mode": "production",
            "json_passthrough": True,
        }

        # Should use passthrough
        assert repo._should_use_passthrough() is True

    def test_passthrough_disabled_in_development_by_default(self):
        """Test that development mode doesn't enable passthrough."""
        repo = MockRepository()

        # Set context with development mode
        repo.context = {
            "mode": "development",
            "json_passthrough": False,
        }

        # Should NOT use passthrough
        assert repo._should_use_passthrough() is False

    @pytest.mark.asyncio
    async def test_find_does_not_wrap_when_passthrough_disabled(self):
        """Test that find() doesn't wrap results when passthrough is disabled."""
        repo = MockRepository()

        # Disable passthrough
        repo.context = {
            "mode": "production",
            "json_passthrough": False,
        }

        result = await repo.find()

        # Should return raw result, not RawJSONResult
        assert isinstance(result, list)
        assert not isinstance(result, RawJSONResult)
        assert result == [{"id": 1, "name": "test"}]

    @pytest.mark.asyncio
    async def test_find_wraps_when_passthrough_enabled(self):
        """Test that find() wraps results when passthrough is enabled."""
        repo = MockRepository()

        # Enable passthrough
        repo.context = {
            "mode": "production",
            "json_passthrough": True,
            "_passthrough_field": "testField",
        }

        result = await repo.find()

        # Should return RawJSONResult
        assert isinstance(result, RawJSONResult)

    def test_execution_mode_passthrough_enables(self):
        """Test that execution_mode='passthrough' enables passthrough."""
        repo = MockRepository()

        repo.context = {
            "mode": "production",
            "json_passthrough": False,
            "execution_mode": "passthrough",  # This should enable it
        }

        assert repo._should_use_passthrough() is True

    def test_passthrough_enabled_flag_enables(self):
        """Test that _passthrough_enabled flag enables passthrough."""
        repo = MockRepository()

        repo.context = {
            "mode": "production",
            "json_passthrough": False,
            "_passthrough_enabled": True,  # This should enable it
        }

        assert repo._should_use_passthrough() is True

    @pytest.mark.parametrize("mode,json_pass,exec_mode,enabled_flag,expected", [
        # Production mode tests
        ("production", False, None, False, False),  # CRITICAL: Production doesn't force passthrough
        ("production", True, None, False, True),    # json_passthrough enables it
        ("production", False, "passthrough", False, True),  # execution_mode enables it
        ("production", False, None, True, True),    # _passthrough_enabled enables it

        # Staging mode tests
        ("staging", False, None, False, False),     # CRITICAL: Staging doesn't force passthrough
        ("staging", True, None, False, True),       # json_passthrough enables it

        # Development mode tests
        ("development", False, None, False, False), # Development doesn't enable
        ("development", True, None, False, True),   # json_passthrough enables it
    ])
    def test_passthrough_configuration_matrix(self, mode, json_pass, exec_mode, enabled_flag, expected):
        """Test all combinations of passthrough configuration."""
        repo = MockRepository()

        repo.context = {
            "mode": mode,
            "json_passthrough": json_pass,
        }

        if exec_mode:
            repo.context["execution_mode"] = exec_mode
        if enabled_flag:
            repo.context["_passthrough_enabled"] = enabled_flag

        assert repo._should_use_passthrough() == expected, (
            f"Failed for mode={mode}, json_passthrough={json_pass}, "
            f"execution_mode={exec_mode}, _passthrough_enabled={enabled_flag}"
        )
