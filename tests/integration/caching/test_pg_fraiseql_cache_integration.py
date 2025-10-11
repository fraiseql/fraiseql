"""Integration tests for pg_fraiseql_cache extension with FraiseQL.

This module tests the automatic cache invalidation provided by the
pg_fraiseql_cache PostgreSQL extension.

Test Phases:
- Phase 4.1: Extension Detection
- Phase 4.2: Domain Version Checking
- Phase 4.3: CASCADE Rule Generation
- Phase 4.4: Automatic Trigger Setup
"""

import logging
from unittest.mock import AsyncMock, MagicMock

import pytest

from fraiseql.caching import CacheConfig, PostgresCache

logger = logging.getLogger(__name__)


class TestExtensionDetection:
    """Phase 4.1: Test automatic detection of pg_fraiseql_cache extension."""

    @pytest.fixture
    def mock_pool(self):
        """Create mock database pool."""
        return MagicMock()

    @pytest.fixture
    def cache_config(self):
        """Create cache configuration."""
        return CacheConfig(enabled=True, default_ttl=300)

    @pytest.mark.asyncio
    async def test_extension_detected_when_installed(self, mock_pool, cache_config):
        """Test that FraiseQL detects pg_fraiseql_cache when installed.

        Expected behavior:
        - Query pg_extension table during initialization
        - Set has_domain_versioning = True
        - Set extension_version to detected version
        - Log success message
        """
        # Setup mock to simulate extension installed
        mock_cursor = AsyncMock()
        mock_cursor.execute = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=("1.0",))  # Extension version
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)

        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.commit = AsyncMock()
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool.connection = MagicMock(return_value=mock_conn)

        # Create cache backend
        cache = PostgresCache(connection_pool=mock_pool, auto_initialize=False)

        # Ensure initialization runs
        await cache._ensure_initialized()

        # Verify extension was detected
        assert hasattr(cache, "has_domain_versioning"), "has_domain_versioning property missing"
        assert cache.has_domain_versioning is True, "Extension should be detected"

        assert hasattr(cache, "extension_version"), "extension_version property missing"
        assert cache.extension_version == "1.0", "Version should be 1.0"

        # Verify pg_extension was queried
        calls = [str(call) for call in mock_cursor.execute.call_args_list]
        extension_query_found = any("pg_extension" in call for call in calls)
        assert extension_query_found, "Should query pg_extension table"

    @pytest.mark.asyncio
    async def test_fallback_when_extension_not_installed(self, mock_pool, cache_config):
        """Test that FraiseQL works without pg_fraiseql_cache extension.

        Expected behavior:
        - Query pg_extension table during initialization
        - Set has_domain_versioning = False
        - Set extension_version = None
        - Log fallback message
        - Continue to work with TTL-only caching
        """
        # Setup mock to simulate extension NOT installed
        mock_cursor = AsyncMock()
        mock_cursor.execute = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=None)  # No extension
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)

        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.commit = AsyncMock()
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool.connection = MagicMock(return_value=mock_conn)

        # Create cache backend
        cache = PostgresCache(connection_pool=mock_pool, auto_initialize=False)

        # Ensure initialization runs
        await cache._ensure_initialized()

        # Verify fallback behavior
        assert hasattr(cache, "has_domain_versioning"), "has_domain_versioning property missing"
        assert cache.has_domain_versioning is False, "Extension should NOT be detected"

        assert hasattr(cache, "extension_version"), "extension_version property missing"
        assert cache.extension_version is None, "Version should be None"

    @pytest.mark.asyncio
    async def test_extension_detection_logs_correctly(self, mock_pool, cache_config, caplog):
        """Test that extension detection produces appropriate log messages.

        Expected behavior:
        - Log success when extension found
        - Log fallback when extension not found
        """
        # Test with extension installed
        mock_cursor = AsyncMock()
        mock_cursor.execute = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=("1.0",))
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)

        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.commit = AsyncMock()
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool.connection = MagicMock(return_value=mock_conn)

        with caplog.at_level(logging.INFO):
            cache = PostgresCache(connection_pool=mock_pool, auto_initialize=False)
            await cache._ensure_initialized()

            # Check for success log message
            log_messages = [record.message for record in caplog.records]
            assert any(
                "pg_fraiseql_cache" in msg and "1.0" in msg for msg in log_messages
            ), "Should log extension detection with version"

    @pytest.mark.asyncio
    async def test_properties_accessible_before_initialization(self, mock_pool):
        """Test that properties are accessible even before initialization.

        Expected behavior:
        - Properties should exist with default values
        - Should not raise AttributeError
        """
        cache = PostgresCache(connection_pool=mock_pool, auto_initialize=False)

        # Should be accessible (will fail until implemented)
        try:
            has_versioning = cache.has_domain_versioning
            version = cache.extension_version
            # If we get here, properties exist (might be None or False)
            assert has_versioning is not None or version is None  # Just checking accessibility
        except AttributeError as e:
            pytest.fail(f"Properties should be accessible: {e}")

    @pytest.mark.asyncio
    async def test_extension_detection_only_runs_once(self, mock_pool):
        """Test that extension detection only happens once per cache instance.

        Expected behavior:
        - First call to _ensure_initialized() should query pg_extension
        - Subsequent calls should use cached result
        """
        mock_cursor = AsyncMock()
        mock_cursor.execute = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=("1.0",))
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)

        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.commit = AsyncMock()
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool.connection = MagicMock(return_value=mock_conn)

        cache = PostgresCache(connection_pool=mock_pool, auto_initialize=False)

        # First initialization
        await cache._ensure_initialized()
        first_call_count = mock_cursor.execute.call_count

        # Second initialization (should be skipped)
        await cache._ensure_initialized()
        second_call_count = mock_cursor.execute.call_count

        # Call count should be the same (no new queries)
        assert (
            first_call_count == second_call_count
        ), "Extension detection should only run once"

    @pytest.mark.asyncio
    async def test_graceful_fallback_on_extension_query_error(self, mock_pool, caplog):
        """Test graceful fallback when extension detection query fails.

        Expected behavior:
        - If pg_extension query fails (e.g., permissions), don't crash
        - Fall back to has_domain_versioning = False
        - Log warning message
        - Continue to work normally
        """
        import psycopg

        # Setup mock to simulate query error on pg_extension query
        mock_cursor = AsyncMock()

        # Track call count to know which query is being executed
        call_count = 0

        async def mock_execute(query, *args):
            nonlocal call_count
            call_count += 1
            # First two calls are CREATE TABLE queries (succeed)
            if call_count <= 2:
                return
            # Third call is pg_extension query (fail with permission error)
            raise psycopg.errors.InsufficientPrivilege("permission denied for table pg_extension")

        mock_cursor.execute = mock_execute
        mock_cursor.fetchone = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)

        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.commit = AsyncMock()
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool.connection = MagicMock(return_value=mock_conn)

        # Create cache backend
        with caplog.at_level(logging.WARNING):
            cache = PostgresCache(connection_pool=mock_pool, auto_initialize=False)
            await cache._ensure_initialized()

            # Verify graceful fallback
            assert cache.has_domain_versioning is False, "Should fallback to no versioning"
            assert cache.extension_version is None, "Version should be None on error"

            # Check for warning log
            log_messages = [record.message for record in caplog.records]
            assert any(
                "Failed to detect pg_fraiseql_cache" in msg for msg in log_messages
            ), "Should log warning on error"


class TestTenantIdInCacheKeys:
    """Phase 4.2.1: Test that cache keys include tenant_id for security isolation."""

    @pytest.fixture
    def mock_pool(self):
        """Create mock database pool."""
        return MagicMock()

    @pytest.fixture
    def mock_cache_backend(self):
        """Create mock cache backend."""
        return AsyncMock()

    @pytest.mark.asyncio
    async def test_cache_key_includes_tenant_id(self):
        """Test that cache keys include tenant_id for isolation.

        Expected behavior:
        - Cache keys should include tenant_id as second component
        - Format: "fraiseql:{tenant_id}:view_name:..."
        - Different tenants get different cache keys for same query
        """
        from uuid import uuid4

        from fraiseql.caching.cache_key import CacheKeyBuilder

        tenant1 = uuid4()
        tenant2 = uuid4()

        # Create cache key builder
        builder = CacheKeyBuilder()

        # Build keys for same query, different tenants
        key1 = builder.build_key("users", tenant_id=tenant1, filters={"status": "active"})
        key2 = builder.build_key("users", tenant_id=tenant2, filters={"status": "active"})

        # Keys MUST be different for different tenants
        assert key1 != key2, "Different tenants must have different cache keys"

        # Keys MUST include tenant_id
        assert str(tenant1) in key1, f"Cache key must include tenant_id: {key1}"
        assert str(tenant2) in key2, f"Cache key must include tenant_id: {key2}"

        # Verify tenant_id is in the correct position (second component)
        key1_parts = key1.split(":")

        assert len(key1_parts) >= 3, "Cache key should have at least 3 parts"
        assert key1_parts[0] == "fraiseql", "First part should be prefix"
        assert key1_parts[1] == str(tenant1), "Second part should be tenant_id"
        assert key1_parts[2] == "users", "Third part should be view name"

    @pytest.mark.asyncio
    async def test_cache_key_without_tenant_id(self):
        """Test that cache keys work without tenant_id for backward compatibility.

        Expected behavior:
        - If no tenant_id provided, should still generate valid key
        - Key should not have empty component
        """
        from fraiseql.caching.cache_key import CacheKeyBuilder

        builder = CacheKeyBuilder()

        # Build key without tenant_id (backward compatibility)
        key = builder.build_key("users", filters={"status": "active"})

        # Should still be valid
        assert key is not None
        assert "users" in key
        assert key.startswith("fraiseql:")

    @pytest.mark.asyncio
    async def test_cached_repository_passes_tenant_id_to_cache_key(
        self, mock_pool, mock_cache_backend
    ):
        """Test that CachedRepository extracts and passes tenant_id to cache key builder.

        Expected behavior:
        - CachedRepository should extract tenant_id from context
        - Pass tenant_id to CacheKeyBuilder.build_key()
        - Cache keys should be tenant-isolated
        """
        from uuid import uuid4

        from fraiseql.caching import CacheConfig
        from fraiseql.caching.repository_integration import CachedRepository
        from fraiseql.caching.result_cache import ResultCache
        from fraiseql.db import FraiseQLRepository

        tenant_id = uuid4()

        # Create base repository with tenant context
        base_repo = FraiseQLRepository(pool=mock_pool, context={"tenant_id": tenant_id})

        # Create cache with mock backend
        cache_config = CacheConfig(enabled=True, default_ttl=300)
        cache = ResultCache(backend=mock_cache_backend, config=cache_config)

        # Create cached repository
        cached_repo = CachedRepository(base_repo, cache)

        # Mock cache miss
        mock_cache_backend.get.return_value = None

        # Mock database result
        mock_cursor = AsyncMock()
        mock_cursor.execute = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[])
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)

        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool.connection = MagicMock(return_value=mock_conn)

        # Execute find query
        await cached_repo.find("users", status="active")

        # Verify cache.get was called
        assert mock_cache_backend.get.call_count >= 1, "Cache should be checked"

        # Get the cache key that was used
        cache_key = mock_cache_backend.get.call_args[0][0]

        # Verify tenant_id is in the cache key
        assert str(tenant_id) in cache_key, f"Cache key must include tenant_id: {cache_key}"

    @pytest.mark.asyncio
    async def test_different_tenants_get_different_cache_entries(
        self, mock_pool, mock_cache_backend
    ):
        """Test that different tenants don't share cache entries (SECURITY TEST).

        Expected behavior:
        - Tenant A and Tenant B query same data
        - Each should get their own cache entry
        - Cache keys must be different
        """
        from uuid import uuid4

        from fraiseql.caching import CacheConfig
        from fraiseql.caching.repository_integration import CachedRepository
        from fraiseql.caching.result_cache import ResultCache
        from fraiseql.db import FraiseQLRepository

        tenant_a = uuid4()
        tenant_b = uuid4()

        # Track cache keys used
        cache_keys_used = []

        def track_cache_get(key):
            cache_keys_used.append(key)

        mock_cache_backend.get = AsyncMock(side_effect=track_cache_get)
        mock_cache_backend.set = AsyncMock()

        # Mock database
        mock_cursor = AsyncMock()
        mock_cursor.execute = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[])
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)

        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool.connection = MagicMock(return_value=mock_conn)

        cache_config = CacheConfig(enabled=True, default_ttl=300)
        cache = ResultCache(backend=mock_cache_backend, config=cache_config)

        # Tenant A queries
        base_repo_a = FraiseQLRepository(pool=mock_pool, context={"tenant_id": tenant_a})
        cached_repo_a = CachedRepository(base_repo_a, cache)
        await cached_repo_a.find("users", status="active")

        # Tenant B queries (same query)
        base_repo_b = FraiseQLRepository(pool=mock_pool, context={"tenant_id": tenant_b})
        cached_repo_b = CachedRepository(base_repo_b, cache)
        await cached_repo_b.find("users", status="active")

        # Verify we tracked 2 cache lookups
        assert len(cache_keys_used) == 2, "Should have 2 cache lookups"

        # Verify cache keys are DIFFERENT
        key_a = cache_keys_used[0]
        key_b = cache_keys_used[1]
        assert key_a != key_b, "Different tenants MUST have different cache keys (SECURITY!)"

        # Verify each key contains its respective tenant_id
        assert str(tenant_a) in key_a, f"Tenant A key must contain tenant_a: {key_a}"
        assert str(tenant_b) in key_b, f"Tenant B key must contain tenant_b: {key_b}"


class TestCacheValueStructure:
    """Phase 4.2.2: Test cache value structure with version metadata."""

    @pytest.fixture
    def mock_pool(self):
        """Create mock database pool."""
        return MagicMock()

    @pytest.mark.asyncio
    async def test_cache_set_accepts_versions_parameter(self, mock_pool):
        """Test that PostgresCache.set() accepts versions parameter.

        Expected behavior:
        - set() should accept optional versions parameter
        - When extension is enabled AND versions provided, wrap value with metadata
        - When extension is disabled OR no versions, store value directly
        """
        from fraiseql.caching.postgres_cache import PostgresCache

        # Mock: extension installed
        mock_cursor = AsyncMock()
        mock_cursor.execute = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=("1.0",))
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)

        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.commit = AsyncMock()
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool.connection = MagicMock(return_value=mock_conn)

        cache = PostgresCache(connection_pool=mock_pool, auto_initialize=False)
        await cache._ensure_initialized()

        # Should accept versions parameter without error
        test_value = [{"id": 1}]
        test_versions = {"user": 42}

        # This should not raise an error
        await cache.set("test_key", test_value, ttl=300, versions=test_versions)

    @pytest.mark.asyncio
    async def test_cache_get_with_metadata_method_exists(self, mock_pool):
        """Test that PostgresCache has get_with_metadata() method.

        Expected behavior:
        - get_with_metadata() method should exist
        - Should return tuple of (result, versions)
        """
        from fraiseql.caching.postgres_cache import PostgresCache

        cache = PostgresCache(connection_pool=mock_pool, auto_initialize=False)

        # Method should exist
        assert hasattr(cache, "get_with_metadata"), "get_with_metadata() method should exist"


class TestVersionChecking:
    """Phase 4.2.3: Test domain version checking for cache invalidation."""

    @pytest.mark.skip(reason="Phase 4.2.3 not yet implemented")
    @pytest.mark.asyncio
    async def test_cache_invalidated_on_data_change(self):
        """Test that cache is invalidated when underlying data changes."""

    @pytest.mark.skip(reason="Phase 4.2.3 not yet implemented")
    @pytest.mark.asyncio
    async def test_tenant_isolated_version_checks(self):
        """Test that version checks are tenant-isolated (CRITICAL SECURITY TEST)."""


class TestCascadeRules:
    """Phase 4.3: Test CASCADE rule generation from GraphQL schema.

    These tests will be implemented after Phase 4.2 is complete.
    """

    @pytest.mark.skip(reason="Phase 4.3 not yet implemented")
    @pytest.mark.asyncio
    async def test_cascade_invalidation(self):
        """Test that updating parent invalidates child cache."""


class TestTriggerSetup:
    """Phase 4.4: Test automatic trigger setup for watched tables.

    These tests will be implemented after Phase 4.3 is complete.
    """

    @pytest.mark.skip(reason="Phase 4.4 not yet implemented")
    @pytest.mark.asyncio
    async def test_automatic_trigger_setup(self):
        """Test that triggers are set up automatically on startup."""
