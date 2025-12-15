"""Tests for cache key generation module."""

from datetime import UTC, date, datetime
from uuid import UUID

import pytest

from fraiseql.caching.cache_key import CacheKeyBuilder


# Tests for CacheKeyBuilder initialization
@pytest.mark.unit
class TestCacheKeyBuilderInit:
    """Tests for CacheKeyBuilder initialization."""

    def test_init_default_prefix(self) -> None:
        """CacheKeyBuilder uses 'fraiseql' as default prefix."""
        builder = CacheKeyBuilder()

        assert builder.prefix == "fraiseql"

    def test_init_custom_prefix(self) -> None:
        """CacheKeyBuilder accepts custom prefix."""
        builder = CacheKeyBuilder(prefix="myapp")

        assert builder.prefix == "myapp"


# Tests for build_key method
@pytest.mark.unit
class TestBuildKey:
    """Tests for CacheKeyBuilder.build_key method."""

    @pytest.fixture
    def builder(self) -> CacheKeyBuilder:
        """Create cache key builder."""
        return CacheKeyBuilder(prefix="test")

    def test_build_key_simple(self, builder: CacheKeyBuilder) -> None:
        """build_key creates simple key with prefix and query name."""
        key = builder.build_key("users")

        assert key == "test:users"

    def test_build_key_with_tenant_id(self, builder: CacheKeyBuilder) -> None:
        """build_key includes tenant_id for multi-tenant isolation."""
        key = builder.build_key("users", tenant_id="tenant_123")

        assert key == "test:tenant_123:users"

    def test_build_key_with_simple_filters(self, builder: CacheKeyBuilder) -> None:
        """build_key includes simple filter values."""
        key = builder.build_key("users", filters={"status": "active"})

        assert "status:active" in key

    def test_build_key_with_multiple_filters(self, builder: CacheKeyBuilder) -> None:
        """build_key sorts multiple filters for consistency."""
        key1 = builder.build_key("users", filters={"status": "active", "role": "admin"})
        key2 = builder.build_key("users", filters={"role": "admin", "status": "active"})

        assert key1 == key2  # Same filters should produce same key

    def test_build_key_with_operator_filters(self, builder: CacheKeyBuilder) -> None:
        """build_key handles operator dict filters."""
        key = builder.build_key("orders", filters={"amount": {"gte": 100, "lte": 1000}})

        assert "amount" in key
        assert "gte:100" in key
        assert "lte:1000" in key

    def test_build_key_with_order_by_tuples(self, builder: CacheKeyBuilder) -> None:
        """build_key includes order_by clauses from tuples."""
        key = builder.build_key("users", order_by=[("name", "asc"), ("created_at", "desc")])

        assert "order:name:asc" in key
        assert "order:created_at:desc" in key

    def test_build_key_with_pagination(self, builder: CacheKeyBuilder) -> None:
        """build_key includes pagination parameters."""
        key = builder.build_key("users", limit=10, offset=20)

        assert "limit:10" in key
        assert "offset:20" in key

    def test_build_key_with_kwargs(self, builder: CacheKeyBuilder) -> None:
        """build_key includes extra kwargs."""
        key = builder.build_key("users", include_deleted=True, version="v2")

        assert "include_deleted:true" in key
        assert "version:v2" in key

    def test_build_key_with_all_parameters(self, builder: CacheKeyBuilder) -> None:
        """build_key combines all parameters correctly."""
        key = builder.build_key(
            "users",
            tenant_id="t123",
            filters={"status": "active"},
            order_by=[("name", "asc")],
            limit=10,
            offset=0,
            extra_param="value",
        )

        assert key.startswith("test:t123:users:")
        assert "status:active" in key
        assert "order:name:asc" in key
        assert "limit:10" in key
        assert "offset:0" in key
        assert "extra_param:value" in key

    def test_build_key_with_empty_order_by(self, builder: CacheKeyBuilder) -> None:
        """build_key handles empty order_by gracefully."""
        key = builder.build_key("users", order_by=[])

        assert key == "test:users"


# Tests for _serialize_value method
@pytest.mark.unit
class TestSerializeValue:
    """Tests for CacheKeyBuilder._serialize_value method."""

    @pytest.fixture
    def builder(self) -> CacheKeyBuilder:
        """Create cache key builder."""
        return CacheKeyBuilder()

    def test_serialize_value_uuid(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value converts UUID to string."""
        uuid_val = UUID("12345678-1234-5678-1234-567812345678")

        result = builder._serialize_value(uuid_val)

        assert result == "12345678-1234-5678-1234-567812345678"

    def test_serialize_value_datetime(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value converts datetime to ISO format."""
        dt = datetime(2024, 1, 15, 10, 30, 0, tzinfo=UTC)

        result = builder._serialize_value(dt)

        assert result == "2024-01-15T10:30:00+00:00"

    def test_serialize_value_date(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value converts date to ISO format."""
        d = date(2024, 1, 15)

        result = builder._serialize_value(d)

        assert result == "2024-01-15"

    def test_serialize_value_list_strings(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value joins sorted string lists."""
        result = builder._serialize_value(["banana", "apple", "cherry"])

        assert result == "apple,banana,cherry"

    def test_serialize_value_list_complex(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value hashes complex lists."""
        result = builder._serialize_value([{"a": 1}, {"b": 2}])

        # Should be a hash (8 chars from md5)
        assert len(result) == 8
        assert result.isalnum()

    def test_serialize_value_bool_true(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value converts True to 'true'."""
        result = builder._serialize_value(True)

        assert result == "true"

    def test_serialize_value_bool_false(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value converts False to 'false'."""
        result = builder._serialize_value(False)

        assert result == "false"

    def test_serialize_value_none(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value converts None to 'null'."""
        result = builder._serialize_value(None)

        assert result == "null"

    def test_serialize_value_int(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value converts int to string."""
        result = builder._serialize_value(42)

        assert result == "42"

    def test_serialize_value_float(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value converts float to string."""
        result = builder._serialize_value(3.14)

        assert result == "3.14"

    def test_serialize_value_string(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value passes strings through."""
        result = builder._serialize_value("hello")

        assert result == "hello"

    def test_serialize_value_tuple(self, builder: CacheKeyBuilder) -> None:
        """_serialize_value handles tuples like lists."""
        result = builder._serialize_value(("apple", "banana"))

        assert result == "apple,banana"


# Tests for _serialize_filter method
@pytest.mark.unit
class TestSerializeFilter:
    """Tests for CacheKeyBuilder._serialize_filter method."""

    @pytest.fixture
    def builder(self) -> CacheKeyBuilder:
        """Create cache key builder."""
        return CacheKeyBuilder()

    def test_serialize_filter_simple_value(self, builder: CacheKeyBuilder) -> None:
        """_serialize_filter handles simple equality filters."""
        result = builder._serialize_filter("status", "active")

        assert result == "status:active"

    def test_serialize_filter_operator_dict(self, builder: CacheKeyBuilder) -> None:
        """_serialize_filter handles operator dict filters."""
        result = builder._serialize_filter("amount", {"gte": 100, "lte": 500})

        assert "amount" in result
        assert "gte:100" in result
        assert "lte:500" in result

    def test_serialize_filter_uuid_value(self, builder: CacheKeyBuilder) -> None:
        """_serialize_filter handles UUID values."""
        uuid_val = UUID("12345678-1234-5678-1234-567812345678")

        result = builder._serialize_filter("user_id", uuid_val)

        assert result == "user_id:12345678-1234-5678-1234-567812345678"


# Tests for build_mutation_pattern method
@pytest.mark.unit
class TestBuildMutationPattern:
    """Tests for CacheKeyBuilder.build_mutation_pattern method."""

    @pytest.fixture
    def builder(self) -> CacheKeyBuilder:
        """Create cache key builder."""
        return CacheKeyBuilder(prefix="myapp")

    def test_build_mutation_pattern(self, builder: CacheKeyBuilder) -> None:
        """build_mutation_pattern creates wildcard pattern for table."""
        pattern = builder.build_mutation_pattern("users")

        assert pattern == "myapp:users:*"

    def test_build_mutation_pattern_different_tables(self, builder: CacheKeyBuilder) -> None:
        """build_mutation_pattern works for different tables."""
        assert builder.build_mutation_pattern("orders") == "myapp:orders:*"
        assert builder.build_mutation_pattern("products") == "myapp:products:*"


# Tests for key consistency
@pytest.mark.unit
class TestKeyConsistency:
    """Tests for cache key consistency."""

    def test_same_params_produce_same_key(self) -> None:
        """Same parameters always produce the same cache key."""
        builder = CacheKeyBuilder()

        key1 = builder.build_key(
            "users",
            tenant_id="t1",
            filters={"status": "active", "role": "admin"},
            limit=10,
        )
        key2 = builder.build_key(
            "users",
            tenant_id="t1",
            filters={"role": "admin", "status": "active"},  # Different order
            limit=10,
        )

        assert key1 == key2

    def test_different_tenants_different_keys(self) -> None:
        """Different tenants produce different keys."""
        builder = CacheKeyBuilder()

        key1 = builder.build_key("users", tenant_id="tenant_a")
        key2 = builder.build_key("users", tenant_id="tenant_b")

        assert key1 != key2

    def test_different_filters_different_keys(self) -> None:
        """Different filters produce different keys."""
        builder = CacheKeyBuilder()

        key1 = builder.build_key("users", filters={"status": "active"})
        key2 = builder.build_key("users", filters={"status": "inactive"})

        assert key1 != key2
