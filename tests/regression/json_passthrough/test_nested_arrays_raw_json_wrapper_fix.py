"""Test for raw_json_wrapper fix: nested arrays bug in production mode.

Bug: FraiseQL v0.1.0-v0.11.0 had a bug in raw_json_wrapper.py where dict/list
results were converted to RawJSONResult too early, bypassing GraphQL field resolution.

This caused nested arrays (list[CustomType]) to be flattened or return incorrect data.

This test directly verifies the raw_json_wrapper fix.
"""

from unittest.mock import MagicMock

import pytest

from fraiseql.core.json_passthrough import JSONPassthrough
from fraiseql.core.raw_json_executor import RawJSONResult
from fraiseql.gql.raw_json_wrapper import create_raw_json_resolver


class TestRawJSONWrapperFix:
    """Test that raw_json_wrapper correctly handles JSONPassthrough without premature conversion."""

    @pytest.mark.asyncio
    async def test_json_passthrough_not_converted_to_raw_json_result(self):
        """CRITICAL: Verify raw_json_wrapper does NOT convert JSONPassthrough to RawJSONResult.

        Before fix: raw_json_wrapper converted dict/list to RawJSONResult immediately
        After fix: raw_json_wrapper returns JSONPassthrough unchanged, allowing GraphQL
                   to resolve nested fields
        """
        # Create mock data as JSONPassthrough (what repository returns)
        user_data = JSONPassthrough(
            {
                "id": 1,
                "name": "John Doe",
                "posts": [
                    {"id": 101, "title": "Post 1"},
                    {"id": 102, "title": "Post 2"},
                ],
            }
        )

        # Create async resolver that returns JSONPassthrough
        async def resolver(info):
            return user_data

        # Wrap with raw_json_resolver (this is where the bug was)
        wrapped = create_raw_json_resolver(resolver, "user")

        # Mock production mode context
        mock_info = MagicMock()
        mock_info.context = {
            "mode": "production",
            "json_passthrough": True,
            "json_passthrough_in_production": True,
        }

        # Execute resolver
        result = await wrapped(None, mock_info)

        # CRITICAL ASSERTIONS: Result should be JSONPassthrough, NOT RawJSONResult
        assert not isinstance(result, RawJSONResult), (
            "BUG DETECTED: raw_json_wrapper converted JSONPassthrough to RawJSONResult! "
            "This bypasses GraphQL field resolution, breaking nested arrays."
        )

        assert isinstance(result, JSONPassthrough), (
            "Result must remain JSONPassthrough to allow GraphQL to resolve nested fields"
        )

        # Verify data is accessible (JSONPassthrough should work like a dict)
        assert result.id == 1
        assert result.name == "John Doe"
        assert isinstance(result.posts, list)
        assert len(result.posts) == 2

    def test_sync_json_passthrough_not_converted(self):
        """Test sync version of raw_json_wrapper also doesn't convert JSONPassthrough."""
        user_data = JSONPassthrough(
            {
                "id": 1,
                "name": "Jane Doe",
                "posts": [{"id": 201, "title": "Sync Post"}],
            }
        )

        # Sync resolver
        def resolver(info):
            return user_data

        wrapped = create_raw_json_resolver(resolver, "user")

        mock_info = MagicMock()
        mock_info.context = {
            "mode": "production",
            "json_passthrough": True,
            "json_passthrough_in_production": True,
        }

        result = wrapped(None, mock_info)

        # Same assertions as async version
        assert not isinstance(result, RawJSONResult)
        assert isinstance(result, JSONPassthrough)
        assert result.id == 1
        assert result.name == "Jane Doe"

    @pytest.mark.asyncio
    async def test_raw_json_result_passed_through_unchanged(self):
        """Test that explicit RawJSONResult (from raw SQL) is still returned correctly.

        The fix should NOT break the legitimate use case where raw SQL queries
        return pre-selected JSON as RawJSONResult.
        """
        # Simulate raw SQL query returning pre-selected JSON
        raw_json = RawJSONResult('{"id": 1, "name": "Test"}')

        async def resolver(info):
            return raw_json

        wrapped = create_raw_json_resolver(resolver, "user")

        mock_info = MagicMock()
        mock_info.context = {
            "mode": "production",
            "json_passthrough": True,
        }

        result = await wrapped(None, mock_info)

        # RawJSONResult should be returned unchanged
        assert isinstance(result, RawJSONResult)
        assert result.json_string == '{"id": 1, "name": "Test"}'

    @pytest.mark.asyncio
    async def test_dict_not_converted_in_production_mode(self):
        """Test that plain dict results are NOT converted to RawJSONResult.

        This was the core bug: converting dict to RawJSONResult too early.
        """
        # Plain dict (not JSONPassthrough)
        user_dict = {
            "id": 1,
            "name": "Test User",
            "posts": [{"id": 1, "title": "Test"}],
        }

        async def resolver(info):
            return user_dict

        wrapped = create_raw_json_resolver(resolver, "user")

        mock_info = MagicMock()
        mock_info.context = {
            "mode": "production",
            "json_passthrough": True,
            "json_passthrough_in_production": True,
        }

        result = await wrapped(None, mock_info)

        # CRITICAL: Should return dict unchanged, NOT RawJSONResult
        assert not isinstance(result, RawJSONResult), (
            "BUG: raw_json_wrapper converted dict to RawJSONResult!"
        )
        assert isinstance(result, dict)
        assert result["id"] == 1
        assert result["name"] == "Test User"

    @pytest.mark.asyncio
    async def test_list_not_converted_in_production_mode(self):
        """Test that list results are NOT converted to RawJSONResult."""
        user_list = [
            {"id": 1, "name": "User 1"},
            {"id": 2, "name": "User 2"},
        ]

        async def resolver(info):
            return user_list

        wrapped = create_raw_json_resolver(resolver, "users")

        mock_info = MagicMock()
        mock_info.context = {
            "mode": "production",
            "json_passthrough": True,
            "json_passthrough_in_production": True,
        }

        result = await wrapped(None, mock_info)

        # Should return list unchanged
        assert not isinstance(result, RawJSONResult)
        assert isinstance(result, list)
        assert len(result) == 2

    @pytest.mark.asyncio
    async def test_none_not_converted(self):
        """Test that None results are NOT converted to RawJSONResult."""

        async def resolver(info):
            return None

        wrapped = create_raw_json_resolver(resolver, "user")

        mock_info = MagicMock()
        mock_info.context = {
            "mode": "production",
            "json_passthrough": True,
        }

        result = await wrapped(None, mock_info)

        # None should remain None
        assert result is None
        assert not isinstance(result, RawJSONResult)


class TestBugReproduction:
    """Tests that would have failed before the fix (demonstrating the bug)."""

    @pytest.mark.asyncio
    async def test_buggy_behavior_would_return_raw_json_result(self):
        """This test demonstrates what the buggy code would have done.

        Before fix: Returning RawJSONResult would bypass GraphQL, causing:
        - Nested arrays to be flattened
        - Field selection from query to be ignored
        - Custom resolvers to not run
        """
        user_data = JSONPassthrough(
            {
                "id": 1,
                "name": "John",
                "posts": [{"id": 1, "title": "Post"}],
            }
        )

        async def resolver(info):
            return user_data

        wrapped = create_raw_json_resolver(resolver, "user")

        mock_info = MagicMock()
        mock_info.context = {
            "mode": "production",
            "json_passthrough": True,
            "json_passthrough_in_production": True,
        }

        result = await wrapped(None, mock_info)

        # The FIXED code returns JSONPassthrough
        # The BUGGY code would have converted to RawJSONResult(json.dumps(user_data))
        #
        # Demonstration of bug impact:
        # if isinstance(result, RawJSONResult):
        #     # This would bypass GraphQL's field resolution
        #     # Nested 'posts' array would not be resolved correctly
        #     # Field selection would be ignored
        #     raise AssertionError("BUG: Premature RawJSONResult conversion!")

        # After fix, this passes:
        assert isinstance(result, JSONPassthrough)
