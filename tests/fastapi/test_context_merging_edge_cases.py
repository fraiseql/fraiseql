"""Comprehensive tests for context merging edge cases."""

import asyncio
import time
from typing import Any, Dict, List, Optional
from unittest.mock import Mock

import pytest
from fastapi import Request
from fastapi.testclient import TestClient

# Import database fixtures for real PostgreSQL testing
from tests.database_conftest import *  # noqa: F403

import fraiseql

pytestmark = pytest.mark.database


# Test types
@fraiseql.type
class ContextInfo:
    """Type to expose context information in queries."""

    source: str
    priority: int
    data: Dict[str, Any]
    timestamp: float


@fraiseql.type
class User:
    """User type for testing authentication context."""

    id: int
    name: str
    roles: List[str]


# Test queries that use context
@fraiseql.query
async def get_context_info(info) -> ContextInfo:
    """Query that returns context information."""
    ctx = info.context
    return ContextInfo(
        source=ctx.get("source", "unknown"),
        priority=ctx.get("priority", 0),
        data=ctx.get("custom_data", {}),
        timestamp=ctx.get("timestamp", 0.0),
    )


@fraiseql.query
async def get_current_user(info) -> Optional[User]:
    """Query that returns current user from context."""
    user_data = info.context.get("user")
    if not user_data:
        return None
    return User(**user_data)


@fraiseql.type
class MergedData:
    """Type to expose merged context data."""
    source: str
    priority: int
    request_id: Optional[str]
    user: Optional[Dict[str, Any]]
    custom_data: Optional[Dict[str, Any]]
    auth_timestamp: Optional[float]
    custom_timestamp: Optional[float]
    level: Optional[str]
    value: Optional[int]
    low_only: Optional[str]
    medium_only: Optional[str]
    high_only: Optional[str]
    shared: Optional[str]
    features: Optional[Dict[str, Any]]
    user_id: Optional[int]


@fraiseql.query
async def get_merged_data(info) -> MergedData:
    """Query that returns all merged context data."""
    ctx = info.context
    # Convert context dict to MergedData type
    return MergedData(
        source=ctx.get("source", "unknown"),
        priority=ctx.get("priority", 0),
        request_id=ctx.get("request_id"),
        user=ctx.get("user"),
        custom_data=ctx.get("custom_data"),
        auth_timestamp=ctx.get("auth_timestamp"),
        custom_timestamp=ctx.get("custom_timestamp"),
        level=ctx.get("level"),
        value=ctx.get("value"),
        low_only=ctx.get("low_only"),
        medium_only=ctx.get("medium_only"),
        high_only=ctx.get("high_only"),
        shared=ctx.get("shared"),
        features=ctx.get("features") or (ctx.get("custom_data", {}).get("feature_flags") if ctx.get("custom_data") else None),
        user_id=ctx.get("user_id") or (ctx.get("user", {}).get("id") if ctx.get("user") else None)
    )


class TestMultipleContextSources:
    """Test merging context from multiple sources."""

    def test_multiple_context_getters(self, create_fraiseql_app_with_db):
        """Test merging contexts from multiple getter functions."""

        # Define multiple context getters
        async def base_context_getter(request: Request) -> Dict[str, Any]:
            """Base context with common data."""
            return {
                "source": "base",
                "priority": 1,
                "timestamp": time.time(),
                "db": None,
                "request_id": "base-123",
            }

        async def auth_context_getter(request: Request) -> Dict[str, Any]:
            """Authentication context."""
            # Simulate extracting user from headers
            auth_header = request.headers.get("Authorization", "")
            user = None
            if auth_header.startswith("Bearer "):
                token = auth_header[7:]
                if token == "valid-token":
                    user = {"id": 1, "name": "Test User", "roles": ["user", "admin"]}

            return {
                "source": "auth",  # This will override base
                "priority": 2,  # This will override base
                "user": user,
                "auth_timestamp": time.time(),
            }

        async def custom_context_getter(request: Request) -> Dict[str, Any]:
            """Custom business logic context."""
            return {
                "source": "custom",  # This will override auth
                "priority": 3,  # This will override auth
                "custom_data": {
                    "feature_flags": {"new_ui": True, "beta": False},
                    "request_path": str(request.url.path),
                },
                "custom_timestamp": time.time(),
            }

        # Create a merged context getter
        async def merged_context_getter(request: Request) -> Dict[str, Any]:
            """Merge multiple context sources."""
            contexts = await asyncio.gather(
                base_context_getter(request),
                auth_context_getter(request),
                custom_context_getter(request),
            )

            # Merge contexts with later ones overriding earlier ones
            merged = {}
            for ctx in contexts:
                merged.update(ctx)

            return merged

        # Create app with merged context
        app = create_fraiseql_app_with_db(
            types=[ContextInfo, User, MergedData],
            queries=[get_context_info, get_current_user, get_merged_data],
            context_getter=merged_context_getter,
            production=False,
        )

        client = TestClient(app)

        # Test with authentication
        response = client.post(
            "/graphql",
            json={"query": "{ getContextInfo { source priority } }"},
            headers={"Authorization": "Bearer valid-token"},
        )

        assert response.status_code == 200
        data = response.json()

        # Should have the last context's values (custom)
        assert data["data"]["getContextInfo"]["source"] == "custom"
        assert data["data"]["getContextInfo"]["priority"] == 3

        # Test that all contexts were merged
        response = client.post(
            "/graphql",
            json={"query": "{ getMergedData { source priority requestId user customData } }"},
            headers={"Authorization": "Bearer valid-token"},
        )

        result = response.json()
        assert response.status_code == 200, f"Response: {result}"
        merged_data = result["data"]["getMergedData"]

        # Should have data from all contexts
        assert merged_data["requestId"] is not None  # From base (camelCase)
        assert merged_data["user"] is not None  # From auth
        assert merged_data["customData"] is not None  # From custom (camelCase)
        assert merged_data["source"] == "custom"  # Last one wins

    def test_context_override_precedence(self, create_fraiseql_app_with_db):
        """Test that context override follows correct precedence."""
        precedence_log = []

        async def low_priority_context(request: Request) -> Dict[str, Any]:
            precedence_log.append("low")
            return {"level": "low", "value": 1, "low_only": "low_data", "shared": "from_low"}

        async def medium_priority_context(request: Request) -> Dict[str, Any]:
            precedence_log.append("medium")
            return {
                "level": "medium",
                "value": 2,
                "medium_only": "medium_data",
                "shared": "from_medium",
            }

        async def high_priority_context(request: Request) -> Dict[str, Any]:
            precedence_log.append("high")
            return {"level": "high", "value": 3, "high_only": "high_data", "shared": "from_high"}

        async def priority_merged_context(request: Request) -> Dict[str, Any]:
            """Merge with explicit priority order."""
            # Clear log for each request
            precedence_log.clear()

            # Get contexts in priority order
            low = await low_priority_context(request)
            medium = await medium_priority_context(request)
            high = await high_priority_context(request)

            # Merge with increasing priority
            merged = {}
            merged.update(low)
            merged.update(medium)
            merged.update(high)

            merged["precedence_order"] = precedence_log.copy()
            return merged

        app = create_fraiseql_app_with_db(
            types=[ContextInfo, MergedData],
            queries=[get_merged_data],
            context_getter=priority_merged_context,
            production=False,
        )

        client = TestClient(app)

        response = client.post("/graphql", json={"query": "{ getMergedData { level value shared lowOnly mediumOnly highOnly } }"})

        result = response.json()
        assert response.status_code == 200, f"Response: {result}"
        data = result["data"]["getMergedData"]

        # Verify precedence
        assert data["level"] == "high"  # Highest priority wins
        assert data["value"] == 3
        assert data["shared"] == "from_high"  # Overridden value

        # Verify all unique values are preserved
        assert data["lowOnly"] == "low_data"  # camelCase
        assert data["mediumOnly"] == "medium_data"  # camelCase
        assert data["highOnly"] == "high_data"  # camelCase

        # Note: precedence_order is not part of MergedData type

    def test_partial_context_merging(self, create_fraiseql_app_with_db):
        """Test merging contexts where some sources return None or empty."""

        async def maybe_auth_context(request: Request) -> Optional[Dict[str, Any]]:
            """Auth context that might return None."""
            auth_header = request.headers.get("Authorization")
            if not auth_header:
                return None  # No auth context

            return {"user": {"id": 1, "name": "Authenticated", "roles": ["user"]}}

        async def maybe_feature_context(request: Request) -> Dict[str, Any]:
            """Feature flags that might be empty."""
            if request.headers.get("X-Beta-User"):
                return {"features": {"beta": True, "experimental": True}}

            return {}  # Empty context

        async def always_base_context(request: Request) -> Dict[str, Any]:
            """Base context that's always present."""
            return {
                "timestamp": time.time(),
                "request_path": str(request.url.path),
                "default_user": {"id": 0, "name": "Anonymous", "roles": ["guest"]},
            }

        async def safe_merged_context(request: Request) -> Dict[str, Any]:
            """Safely merge contexts handling None and empty."""
            contexts = await asyncio.gather(
                always_base_context(request),
                maybe_auth_context(request),
                maybe_feature_context(request),
            )

            merged = {}
            for ctx in contexts:
                if ctx:  # Skip None contexts
                    merged.update(ctx)

            # Use authenticated user if available, otherwise default
            if "user" not in merged and "default_user" in merged:
                merged["user"] = merged["default_user"]

            return merged

        app = create_fraiseql_app_with_db(
            types=[User, MergedData],
            queries=[get_current_user, get_merged_data],
            context_getter=safe_merged_context,
            production=False,
        )

        client = TestClient(app)

        # Test without authentication
        response = client.post("/graphql", json={"query": "{ getCurrentUser { id name } }"})

        result = response.json()
        assert response.status_code == 200, f"Response: {result}"
        data = result.get("data")
        assert data is not None, f"No data in response: {result}"
        assert data.get("getCurrentUser") is not None, f"getCurrentUser is None: {result}"
        assert data["getCurrentUser"]["id"] == 0
        assert data["getCurrentUser"]["name"] == "Anonymous"

        # Test with authentication
        response = client.post(
            "/graphql",
            json={"query": "{ getCurrentUser { id name } }"},
            headers={"Authorization": "Bearer token"},
        )

        data = response.json()["data"]
        assert data["getCurrentUser"]["id"] == 1
        assert data["getCurrentUser"]["name"] == "Authenticated"

        # Test with beta features
        response = client.post(
            "/graphql", json={"query": "{ getMergedData { features } }"}, headers={"X-Beta-User": "true"}
        )

        result = response.json()
        assert response.status_code == 200, f"Response: {result}"
        data = result["data"]["getMergedData"]
        assert data["features"] is not None
        assert data["features"]["beta"] is True
        assert data["features"]["experimental"] is True


class TestAsyncContextGetters:
    """Test async context getter patterns."""

    @pytest.mark.asyncio
    async def test_concurrent_async_context_getters(self):
        """Test multiple async context getters running concurrently."""
        # Track timing to ensure concurrent execution
        timing_log = []

        async def slow_db_context(request: Request) -> Dict[str, Any]:
            """Simulate slow database connection."""
            start = time.time()
            await asyncio.sleep(0.1)  # Simulate DB connection
            timing_log.append(("db", start, time.time()))

            return {"db": "mock_db_connection", "db_latency": 0.1}

        async def slow_cache_context(request: Request) -> Dict[str, Any]:
            """Simulate slow cache lookup."""
            start = time.time()
            await asyncio.sleep(0.1)  # Simulate cache lookup
            timing_log.append(("cache", start, time.time()))

            return {"cache": "mock_cache_client", "cache_latency": 0.1}

        async def slow_auth_context(request: Request) -> Dict[str, Any]:
            """Simulate slow auth service."""
            start = time.time()
            await asyncio.sleep(0.1)  # Simulate auth check
            timing_log.append(("auth", start, time.time()))

            return {"user": {"id": 1, "name": "Async User", "roles": ["user"]}, "auth_latency": 0.1}

        async def concurrent_context_getter(request: Request) -> Dict[str, Any]:
            """Get all contexts concurrently."""
            timing_log.clear()
            start_time = time.time()

            # Run all context getters concurrently
            contexts = await asyncio.gather(
                slow_db_context(request), slow_cache_context(request), slow_auth_context(request)
            )

            # Merge results
            merged = {"total_latency": time.time() - start_time}
            for ctx in contexts:
                merged.update(ctx)

            return merged

        # Create mock request
        mock_request = Mock(spec=Request)
        mock_request.url.path = "/graphql"
        mock_request.headers = {}

        # Test concurrent execution
        context = await concurrent_context_getter(mock_request)

        # All three should have run concurrently, so total time ~0.1s not ~0.3s
        assert context["total_latency"] < 0.2  # Should be ~0.1s plus overhead

        # Verify all contexts were fetched
        assert "db" in context
        assert "cache" in context
        assert "user" in context

        # Verify they ran concurrently (overlapping times)
        db_times = next(t for t in timing_log if t[0] == "db")
        cache_times = next(t for t in timing_log if t[0] == "cache")
        auth_times = next(t for t in timing_log if t[0] == "auth")

        # They should have started at roughly the same time
        start_times = [db_times[1], cache_times[1], auth_times[1]]
        assert max(start_times) - min(start_times) < 0.01

    @pytest.mark.asyncio
    async def test_async_context_error_handling(self):
        """Test error handling in async context getters."""

        async def failing_context(request: Request) -> Dict[str, Any]:
            """Context getter that fails."""
            raise ValueError("Context fetch failed")

        async def timeout_context(request: Request) -> Dict[str, Any]:
            """Context getter that times out."""
            await asyncio.sleep(10)  # Will timeout
            return {"should_not": "reach_here"}

        async def working_context(request: Request) -> Dict[str, Any]:
            """Context getter that works."""
            return {"status": "ok", "data": "valid"}

        async def resilient_context_getter(request: Request) -> Dict[str, Any]:
            """Context getter with error handling."""
            results = {"errors": [], "successful_contexts": 0}

            # Try to get each context with error handling
            try:
                failing = await failing_context(request)
                results.update(failing)
                results["successful_contexts"] += 1
            except Exception as e:
                results["errors"].append(f"failing_context: {e!s}")

            try:
                # Add timeout
                timeout = await asyncio.wait_for(timeout_context(request), timeout=0.1)
                results.update(timeout)
                results["successful_contexts"] += 1
            except TimeoutError:
                results["errors"].append("timeout_context: Timeout")
            except Exception as e:
                results["errors"].append(f"timeout_context: {e!s}")

            try:
                working = await working_context(request)
                results.update(working)
                results["successful_contexts"] += 1
            except Exception as e:
                results["errors"].append(f"working_context: {e!s}")

            return results

        # Create mock request
        mock_request = Mock(spec=Request)

        # Test resilient context getter
        context = await resilient_context_getter(mock_request)

        # Should have handled errors gracefully
        assert len(context["errors"]) == 2
        assert "failing_context: Context fetch failed" in context["errors"]
        assert "timeout_context: Timeout" in context["errors"]

        # Should have successful context
        assert context["successful_contexts"] == 1
        assert context["status"] == "ok"
        assert context["data"] == "valid"

    def test_async_context_with_dependencies(self, create_fraiseql_app_with_db):
        """Test async context getters with dependencies between them."""

        async def get_user_id_context(request: Request) -> Dict[str, Any]:
            """Get user ID from request."""
            auth_header = request.headers.get("Authorization", "")
            if auth_header.startswith("Bearer "):
                return {"user_id": 123}
            return {"user_id": None}

        async def get_user_profile_context(
            request: Request, user_id: Optional[int]
        ) -> Dict[str, Any]:
            """Get user profile based on user ID."""
            if user_id:
                # Simulate async profile fetch
                await asyncio.sleep(0.01)
                return {
                    "user_profile": {
                        "id": user_id,
                        "name": "Test User",
                        "email": "test@example.com",
                    }
                }
            return {"user_profile": None}

        async def get_user_permissions_context(
            request: Request, user_id: Optional[int]
        ) -> Dict[str, Any]:
            """Get user permissions based on user ID."""
            if user_id:
                # Simulate async permissions fetch
                await asyncio.sleep(0.01)
                return {"permissions": ["read", "write", "admin"]}
            return {"permissions": ["read"]}  # Default permissions

        async def dependent_context_getter(request: Request) -> Dict[str, Any]:
            """Context getter with dependencies."""
            # First get user ID
            user_id_ctx = await get_user_id_context(request)
            user_id = user_id_ctx.get("user_id")

            # Then get profile and permissions in parallel
            profile_task = get_user_profile_context(request, user_id)
            perms_task = get_user_permissions_context(request, user_id)

            profile_ctx, perms_ctx = await asyncio.gather(
                profile_task,
                perms_task,
            )

            # Merge all contexts
            merged = {}
            merged.update(user_id_ctx)
            merged.update(profile_ctx)
            merged.update(perms_ctx)

            return merged

        app = create_fraiseql_app_with_db(
            types=[User, MergedData],
            queries=[get_merged_data],
            context_getter=dependent_context_getter,
            production=False,
        )

        client = TestClient(app)

        # Test without auth
        response = client.post("/graphql", json={"query": "{ getMergedData { userId user } }"})

        result = response.json()
        assert response.status_code == 200, f"Response: {result}"
        data = result["data"]["getMergedData"]
        assert data["userId"] is None  # camelCase
        assert data["user"] is None  # This is how MergedData stores user_profile
        # Note: permissions field is not part of MergedData type

        # Test with auth
        response = client.post(
            "/graphql",
            json={"query": "{ getMergedData { userId } }"},
            headers={"Authorization": "Bearer valid-token"},
        )

        result = response.json()
        assert response.status_code == 200, f"Response: {result}"
        data = result["data"]["getMergedData"]
        assert data["userId"] == 123  # camelCase
        # Note: user_profile and permissions are not directly accessible in MergedData type


class TestContextMergingEdgeCases:
    """Test edge cases in context merging."""

    def test_deeply_nested_context_merging(self, create_fraiseql_app_with_db):
        """Test merging deeply nested context objects."""

        # Create types for nested structure
        @fraiseql.type
        class Level3Data:
            value_a: Optional[str]
            value_b: Optional[str]
            shared: str

        @fraiseql.type
        class Level2Data:
            level3: Level3Data
            level2_a: Optional[str]
            level2_b: Optional[str]

        @fraiseql.type
        class Level1Data:
            level2: Level2Data
            arrays: List[int]

        @fraiseql.type
        class NestedMergedData:
            level1: Level1Data

        async def deep_context_a(request: Request) -> Dict[str, Any]:
            return {
                "level1": {
                    "level2": {
                        "level3": {"value_a": "from_a", "shared": "a_wins"},
                        "level2_a": "only_in_a",
                    },
                    "arrays": [1, 2, 3],
                }
            }

        async def deep_context_b(request: Request) -> Dict[str, Any]:
            return {
                "level1": {
                    "level2": {
                        "level3": {
                            "value_b": "from_b",
                            "shared": "b_wins",  # Should override
                        },
                        "level2_b": "only_in_b",
                    },
                    "arrays": [4, 5, 6],  # Should override
                }
            }

        async def deep_merge_context(request: Request) -> Dict[str, Any]:
            """Deep merge contexts."""
            ctx_a = await deep_context_a(request)
            ctx_b = await deep_context_b(request)

            # Simple merge (last wins for conflicts)
            import copy

            merged = copy.deepcopy(ctx_a)

            def deep_update(base: dict, update: dict) -> dict:
                for key, value in update.items():
                    if key in base and isinstance(base[key], dict) and isinstance(value, dict):
                        deep_update(base[key], value)
                    else:
                        base[key] = value
                return base

            deep_update(merged, ctx_b)
            return merged

        # Create a specific query for nested data
        @fraiseql.query
        async def get_nested_merged_data(info) -> NestedMergedData:
            """Query that returns nested merged data."""
            ctx = info.context
            # Only pass the level1 data that NestedMergedData expects
            if "level1" in ctx:
                return NestedMergedData(level1=ctx["level1"])
            else:
                return None

        app = create_fraiseql_app_with_db(
            types=[Level3Data, Level2Data, Level1Data, NestedMergedData],
            queries=[get_nested_merged_data],
            context_getter=deep_merge_context,
            production=False
        )

        client = TestClient(app)

        response = client.post("/graphql", json={"query": "{ getNestedMergedData { level1 { level2 { level3 { valueA valueB shared } level2A level2B } arrays } } }"})

        result = response.json()
        assert response.status_code == 200, f"Response: {result}"
        assert result.get("data") is not None, f"No data in response: {result}"
        assert result["data"].get("getNestedMergedData") is not None, f"getNestedMergedData is None: {result}"
        data = result["data"]["getNestedMergedData"]

        # Check deep merge results
        level3 = data["level1"]["level2"]["level3"]
        assert level3["valueA"] == "from_a"  # Preserved from A (camelCase)
        assert level3["valueB"] == "from_b"  # Added from B (camelCase)
        assert level3["shared"] == "b_wins"  # B overwrote A

        level2 = data["level1"]["level2"]
        assert level2["level2A"] == "only_in_a"  # Preserved (camelCase)
        assert level2["level2B"] == "only_in_b"  # Added (camelCase)

        # Arrays are replaced, not merged
        assert data["level1"]["arrays"] == [4, 5, 6]

    def test_context_with_circular_references(self, clear_registry):
        """Test context merging with circular references."""

        async def circular_context(request: Request) -> Dict[str, Any]:
            """Create context with circular references."""
            ctx = {"user": {"id": 1, "name": "User"}, "request": {"path": "/graphql"}}

            # Create circular reference
            ctx["user"]["context"] = ctx
            ctx["request"]["user"] = ctx["user"]

            return ctx

        # This test verifies that circular references don't cause issues
        # The actual behavior depends on how the GraphQL resolver handles it
        mock_request = Mock(spec=Request)

        # Should not raise an exception
        try:
            context = asyncio.run(circular_context(mock_request))
            assert context["user"]["context"] is context  # Circular ref exists
        except Exception as e:
            pytest.fail(f"Circular reference handling failed: {e}")

    def test_context_key_conflicts(self, create_fraiseql_app_with_db):
        """Test handling of conflicting context keys."""

        async def system_context(request: Request) -> Dict[str, Any]:
            """System-level context."""
            return {
                "user": "system",  # String value
                "priority": "high",
                "timestamp": 1234567890,
                "meta": {"source": "system"},
            }

        async def user_context(request: Request) -> Dict[str, Any]:
            """User-level context."""
            return {
                "user": {
                    "id": 1,
                    "name": "Real User",
                    "roles": ["user"],
                },  # Dict value (different type)
                "priority": 100,  # Number instead of string
                "timestamp": 9876543210,  # Newer timestamp
                "meta": {"source": "user", "extra": "data"},
            }

        async def conflict_resolution_context(request: Request) -> Dict[str, Any]:
            """Resolve conflicts with custom logic."""
            sys_ctx = await system_context(request)
            user_ctx = await user_context(request)

            # Custom conflict resolution
            merged = {}

            # User context takes precedence
            merged.update(sys_ctx)
            merged.update(user_ctx)

            # But preserve some system values with different keys
            merged["system_user"] = sys_ctx["user"]
            merged["system_priority"] = sys_ctx["priority"]

            # Merge meta objects
            merged["meta"] = {**sys_ctx["meta"], **user_ctx["meta"]}

            return merged

        app = create_fraiseql_app_with_db(
            types=[MergedData],
            queries=[get_merged_data],
            context_getter=conflict_resolution_context,
            production=False,
        )

        client = TestClient(app)

        response = client.post("/graphql", json={"query": "{ getMergedData { user priority } }"})

        result = response.json()
        assert response.status_code == 200, f"Response: {result}"
        data = result["data"]["getMergedData"]

        # User context won for direct conflicts
        assert data["user"] == {"id": 1, "name": "Real User", "roles": ["user"]}
        assert data["priority"] == 100
        # Note: timestamp and other fields are not part of MergedData type

        # Note: system_user, system_priority, and meta fields are not part of MergedData type
        # They are in the context but not exposed through GraphQL
