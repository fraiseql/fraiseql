# Phase 1: RED - Write Failing Tests

**Status**: Ready for Implementation
**Effort**: 1 hour
**Type**: TDD - Test First

---

## Objective

Write tests that **reproduce the exact bug** reported by PrintOptim. These tests will FAIL, clearly demonstrating the problem with nested JSONB field camelCase conversion.

---

## Context

Based on codebase investigation, the bug manifests because:

1. **Schema-aware path** (`transform_with_schema`) may not recursively convert nested JSONB keys when fields aren't in the schema registry
2. **Zero-copy path** (`build_zero_copy`) has no schema awareness for nested types
3. Fields like `dns_1` may not be registered in schema, causing lookup failures

The bug manifests in two ways:
1. **Fields with underscores** (`smtp_server`, `print_servers`) return as snake_case instead of camelCase
2. **Fields with underscore+number** (`dns_1`, `dns_2`) may be missing or unconverted

---

## Verified Rust Exports

These functions are available from `fraiseql._fraiseql_rs`:

| Function | Purpose |
|----------|---------|
| `to_camel_case(s: str) -> str` | Convert single snake_case string to camelCase |
| `transform_json(json_str: str) -> str` | Transform JSON string keys to camelCase |
| `build_graphql_response(json_strings, field_name, type_name, field_selections, is_list) -> bytes` | Build complete GraphQL response |
| `initialize_schema_registry(schema_json: str)` | Initialize global schema registry |
| `reset_schema_registry_for_testing()` | Clear registry for testing |

---

## Test Files to Create

### Test 1: Regression Test (Integration)
**File**: `tests/regression/test_jsonb_nested_camelcase.py`

```python
"""Regression test for JSONB nested field camelCase conversion.

Validates that nested objects within JSONB columns have their field names
correctly converted from snake_case to camelCase in GraphQL responses.

Test patterns:
- Single nested objects (e.g., smtpServer)
- Numbered fields (e.g., dns1, dns2)
- Arrays of nested objects (e.g., printServers)
"""

import uuid
from typing import Optional

import pytest
import pytest_asyncio

from fraiseql import query
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.types import fraise_type

pytestmark = pytest.mark.integration


@fraise_type
class DnsServer:
    """DNS server nested object."""
    id: uuid.UUID
    identifier: str
    ip_address: str


@fraise_type
class SmtpServer:
    """SMTP server nested object."""
    id: uuid.UUID
    identifier: str
    ip_address: str
    port: int


@fraise_type
class PrintServer:
    """Print server nested object."""
    id: uuid.UUID
    identifier: str
    hostname: str


@fraise_type
class Gateway:
    """Gateway nested object (single word - control case)."""
    id: uuid.UUID
    identifier: str
    ip_address: str


@fraise_type(sql_source="tv_network_configuration", jsonb_column="data")
class NetworkConfiguration:
    """Network configuration with nested JSONB objects."""
    id: uuid.UUID
    identifier: str

    # Single-word nested objects (control case - should work)
    gateway: Optional[Gateway] = None

    # Underscore nested objects
    smtp_server: Optional[SmtpServer] = None

    # Underscore+number nested objects
    dns_1: Optional[DnsServer] = None
    dns_2: Optional[DnsServer] = None

    # Array of nested objects
    print_servers: Optional[list[PrintServer]] = None


@query
async def network_configuration(info, id: uuid.UUID) -> Optional[NetworkConfiguration]:
    """Get a network configuration by ID."""
    repo = info.context["db"]
    return await repo.find_one("tv_network_configuration", id=str(id))


@query
async def network_configurations(info, limit: int = 10) -> list[NetworkConfiguration]:
    """List network configurations."""
    repo = info.context["db"]
    return await repo.find("tv_network_configuration", limit=limit)


class TestJSONBNestedCamelCase:
    """Test camelCase conversion for nested JSONB objects.

    Follows FraiseQL test architecture:
    - Class-scoped database setup
    - Schema isolation via test_<classname>_<uuid>
    - SchemaAwarePool wrapper for app connections
    """

    TEST_CONFIG_ID = "01436121-0000-0000-0000-000000000000"

    @pytest_asyncio.fixture(scope="class")
    async def setup_database(self, class_db_pool, test_schema):
        """Set up database with JSONB nested objects."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")

            await conn.execute("DROP VIEW IF EXISTS tv_network_configuration CASCADE")
            await conn.execute("DROP TABLE IF EXISTS tb_network_configuration CASCADE")

            await conn.execute("""
                CREATE TABLE tb_network_configuration (
                    id UUID PRIMARY KEY,
                    identifier TEXT NOT NULL,
                    data JSONB NOT NULL
                )
            """)

            await conn.execute("""
                CREATE VIEW tv_network_configuration AS
                SELECT id, identifier, data
                FROM tb_network_configuration
            """)

            # Test data with ALL nested object patterns
            test_data = '''{
                "id": "01436121-0000-0000-0000-000000000000",
                "identifier": "Network configuration 01",
                "gateway": {
                    "id": "01432121-0000-0000-0000-000000000000",
                    "identifier": "Gateway 1",
                    "ip_address": "30.0.0.1"
                },
                "smtp_server": {
                    "id": "01435121-0000-0000-0000-000000000000",
                    "identifier": "SMTP Server 1",
                    "ip_address": "13.16.1.10",
                    "port": 587
                },
                "dns_1": {
                    "id": "01431121-0000-0000-0000-000000000001",
                    "identifier": "primary-dns-server",
                    "ip_address": "120.0.0.1"
                },
                "dns_2": {
                    "id": "01431121-0000-0000-0000-000000000002",
                    "identifier": "secondary-dns-server",
                    "ip_address": "120.0.0.2"
                },
                "print_servers": [
                    {
                        "id": "01433121-0000-0000-0000-000000000001",
                        "identifier": "PrintServer-001",
                        "hostname": "printserver01.local"
                    },
                    {
                        "id": "01433121-0000-0000-0000-000000000002",
                        "identifier": "PrintServer-002",
                        "hostname": "printserver02.local"
                    }
                ]
            }'''

            await conn.execute(f"""
                INSERT INTO tb_network_configuration (id, identifier, data)
                VALUES ('{self.TEST_CONFIG_ID}'::uuid, 'Network configuration 01', '{test_data}'::jsonb)
            """)

            await conn.commit()

        yield

    @pytest_asyncio.fixture(scope="class")
    def graphql_app(self, class_db_pool, test_schema, setup_database, clear_registry_class):
        """Create GraphQL app with schema-aware pool."""
        from contextlib import asynccontextmanager
        from fraiseql.fastapi.dependencies import set_db_pool

        class SchemaAwarePool:
            def __init__(self, pool, schema):
                self._pool = pool
                self._schema = schema

            @asynccontextmanager
            async def connection(self):
                async with self._pool.connection() as conn:
                    await conn.execute(f"SET search_path TO {self._schema}, public")
                    yield conn

            def __getattr__(self, name):
                return getattr(self._pool, name)

        wrapped_pool = SchemaAwarePool(class_db_pool, test_schema)
        set_db_pool(wrapped_pool)

        app = create_fraiseql_app(
            database_url="postgresql://test/test",
            types=[NetworkConfiguration, DnsServer, SmtpServer, PrintServer, Gateway],
            queries=[network_configuration, network_configurations],
            production=False,
        )
        return app

    async def _execute_query(self, graphql_app, query_str: str, variables: dict = None):
        """Execute GraphQL query and return response."""
        from asgi_lifespan import LifespanManager
        from httpx import ASGITransport, AsyncClient

        async with LifespanManager(graphql_app) as manager:
            transport = ASGITransport(app=manager.app)
            async with AsyncClient(transport=transport, base_url="http://test") as client:
                payload = {"query": query_str}
                if variables:
                    payload["variables"] = variables
                response = await client.post("/graphql", json=payload)
        return response

    @pytest.mark.asyncio
    async def test_single_word_nested_object_converts_to_camelcase(self, graphql_app):
        """Control test: single-word nested objects should work (gateway)."""
        query_str = """
        query GetNetworkConfig {
            networkConfigurations {
                id
                identifier
                gateway {
                    id
                    identifier
                    ipAddress
                }
            }
        }
        """

        response = await self._execute_query(graphql_app, query_str)
        assert response.status_code == 200

        result = response.json()
        assert "errors" not in result, f"GraphQL errors: {result.get('errors')}"
        assert "data" in result
        assert "networkConfigurations" in result["data"]

        configs = result["data"]["networkConfigurations"]
        assert len(configs) > 0

        config = configs[0]
        assert "gateway" in config
        assert config["gateway"]["ipAddress"] == "30.0.0.1"

    @pytest.mark.asyncio
    async def test_underscore_nested_object_converts_to_camelcase(self, graphql_app):
        """Nested objects with underscore names should convert to camelCase."""
        query_str = """
        query GetNetworkConfig {
            networkConfigurations {
                id
                identifier
                smtpServer {
                    id
                    identifier
                    ipAddress
                    port
                }
            }
        }
        """

        response = await self._execute_query(graphql_app, query_str)
        assert response.status_code == 200

        result = response.json()
        assert "errors" not in result, f"GraphQL errors: {result.get('errors')}"
        assert "data" in result

        configs = result["data"]["networkConfigurations"]
        assert len(configs) > 0

        config = configs[0]
        assert "smtpServer" in config, f"Expected 'smtpServer', got keys: {list(config.keys())}"
        assert config["smtpServer"]["ipAddress"] == "13.16.1.10"
        assert config["smtpServer"]["port"] == 587

    @pytest.mark.asyncio
    async def test_underscore_number_nested_object_is_present(self, graphql_app):
        """Numbered fields like dns_1 should convert to dns1."""
        query_str = """
        query GetNetworkConfig {
            networkConfigurations {
                id
                identifier
                dns1 {
                    id
                    identifier
                    ipAddress
                }
                dns2 {
                    id
                    identifier
                    ipAddress
                }
            }
        }
        """

        response = await self._execute_query(graphql_app, query_str)
        assert response.status_code == 200

        result = response.json()
        assert "errors" not in result, f"GraphQL errors: {result.get('errors')}"
        assert "data" in result

        configs = result["data"]["networkConfigurations"]
        assert len(configs) > 0

        config = configs[0]
        assert "dns1" in config, f"Expected 'dns1', got keys: {list(config.keys())}"
        assert "dns2" in config, f"Expected 'dns2', got keys: {list(config.keys())}"
        assert config["dns1"]["ipAddress"] == "120.0.0.1"
        assert config["dns2"]["ipAddress"] == "120.0.0.2"

    @pytest.mark.asyncio
    async def test_array_nested_objects_convert_to_camelcase(self, graphql_app):
        """Array fields like print_servers should convert to printServers."""
        query_str = """
        query GetNetworkConfig {
            networkConfigurations {
                id
                identifier
                printServers {
                    id
                    identifier
                    hostname
                }
            }
        }
        """

        response = await self._execute_query(graphql_app, query_str)
        assert response.status_code == 200

        result = response.json()
        assert "errors" not in result, f"GraphQL errors: {result.get('errors')}"
        assert "data" in result

        configs = result["data"]["networkConfigurations"]
        assert len(configs) > 0

        config = configs[0]
        assert "printServers" in config, f"Expected 'printServers', got keys: {list(config.keys())}"
        assert len(config["printServers"]) == 2
        assert config["printServers"][0]["hostname"] == "printserver01.local"

    @pytest.mark.asyncio
    async def test_all_nested_fields_in_single_query(self, graphql_app):
        """Combined test: all nested field types in one query."""
        query_str = """
        query GetNetworkConfig {
            networkConfigurations {
                id
                identifier
                gateway {
                    id
                    identifier
                    ipAddress
                }
                smtpServer {
                    id
                    identifier
                    ipAddress
                }
                dns1 {
                    id
                    identifier
                    ipAddress
                }
                dns2 {
                    id
                    identifier
                    ipAddress
                }
                printServers {
                    id
                    identifier
                    hostname
                }
            }
        }
        """

        response = await self._execute_query(graphql_app, query_str)
        assert response.status_code == 200

        result = response.json()
        assert "errors" not in result, f"GraphQL errors: {result.get('errors')}"

        configs = result["data"]["networkConfigurations"]
        assert len(configs) > 0

        config = configs[0]
        expected_fields = ["gateway", "smtpServer", "dns1", "dns2", "printServers"]
        for field in expected_fields:
            assert field in config, f"Expected '{field}', got: {list(config.keys())}"


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
```

---

### Test 2: Unit Test for CamelCase Conversion
**File**: `tests/unit/core/test_jsonb_camelcase_conversion.py`

```python
"""Unit tests for camelCase conversion with problematic patterns.

Tests the Rust functions exported to Python for camelCase conversion
of nested JSONB structures.
"""

import json

import pytest


class TestCamelCaseConversionPatterns:
    """Test camelCase conversion for problematic field patterns."""

    def test_underscore_pattern_to_camelcase(self):
        """Standard underscore patterns should convert correctly."""
        from fraiseql._fraiseql_rs import to_camel_case

        assert to_camel_case("smtp_server") == "smtpServer"
        assert to_camel_case("print_servers") == "printServers"
        assert to_camel_case("ip_address") == "ipAddress"

    def test_underscore_number_pattern_to_camelcase(self):
        """Underscore before number should produce correct output."""
        from fraiseql._fraiseql_rs import to_camel_case

        # dns_1 → dns1 (number not capitalized)
        assert to_camel_case("dns_1") == "dns1"
        assert to_camel_case("dns_2") == "dns2"
        assert to_camel_case("backup_1_id") == "backup1Id"
        assert to_camel_case("server_10_name") == "server10Name"

    def test_single_word_unchanged(self):
        """Single words should remain unchanged."""
        from fraiseql._fraiseql_rs import to_camel_case

        assert to_camel_case("gateway") == "gateway"
        assert to_camel_case("router") == "router"
        assert to_camel_case("id") == "id"

    def test_already_camelcase_unchanged(self):
        """Already camelCase strings should remain unchanged."""
        from fraiseql._fraiseql_rs import to_camel_case

        assert to_camel_case("smtpServer") == "smtpServer"
        assert to_camel_case("ipAddress") == "ipAddress"

    def test_transform_json_nested_dict(self):
        """transform_json should convert all nested keys to camelCase."""
        from fraiseql._fraiseql_rs import transform_json

        input_json = json.dumps({
            "id": "123",
            "smtp_server": {
                "ip_address": "10.0.0.1",
                "port": 25
            },
            "dns_1": {
                "ip_address": "8.8.8.8"
            },
            "print_servers": [
                {"host_name": "printer1"}
            ]
        })

        result = transform_json(input_json)
        parsed = json.loads(result)

        # Top-level keys should be camelCase
        assert "smtpServer" in parsed, f"Got keys: {list(parsed.keys())}"
        assert "dns1" in parsed, f"Got keys: {list(parsed.keys())}"
        assert "printServers" in parsed, f"Got keys: {list(parsed.keys())}"

        # Nested keys should be camelCase
        assert "ipAddress" in parsed["smtpServer"]
        assert "ipAddress" in parsed["dns1"]
        assert "hostName" in parsed["printServers"][0]


class TestBuildGraphQLResponse:
    """Test build_graphql_response for nested JSONB handling."""

    def test_nested_object_keys_converted(self):
        """build_graphql_response should convert nested object keys."""
        from fraiseql._fraiseql_rs import build_graphql_response

        json_string = json.dumps({
            "id": "123",
            "identifier": "test",
            "smtp_server": {
                "id": "456",
                "ip_address": "10.0.0.1"
            },
            "dns_1": {
                "id": "789",
                "ip_address": "8.8.8.8"
            }
        })

        response_bytes = build_graphql_response(
            [json_string],
            "networkConfiguration",
            "NetworkConfiguration",
            None,
            False,
        )

        response = json.loads(response_bytes)
        data = response["data"]["networkConfiguration"]

        assert "smtpServer" in data, f"Got keys: {list(data.keys())}"
        assert "ipAddress" in data["smtpServer"]
        assert "dns1" in data, f"Got keys: {list(data.keys())}"
        assert "ipAddress" in data["dns1"]

    def test_array_item_keys_converted(self):
        """build_graphql_response should convert keys in array items."""
        from fraiseql._fraiseql_rs import build_graphql_response

        json_string = json.dumps({
            "id": "123",
            "print_servers": [
                {"host_name": "printer1", "ip_address": "10.0.0.1"},
                {"host_name": "printer2", "ip_address": "10.0.0.2"}
            ]
        })

        response_bytes = build_graphql_response(
            [json_string],
            "config",
            "Config",
            None,
            False,
        )

        response = json.loads(response_bytes)
        data = response["data"]["config"]

        assert "printServers" in data, f"Got keys: {list(data.keys())}"
        assert "hostName" in data["printServers"][0]
        assert "ipAddress" in data["printServers"][0]

    def test_deeply_nested_keys_converted(self):
        """Deeply nested structures should have all keys converted."""
        from fraiseql._fraiseql_rs import build_graphql_response

        json_string = json.dumps({
            "id": "123",
            "network_config": {
                "primary_dns": {
                    "ip_address": "8.8.8.8",
                    "backup_servers": [
                        {"server_name": "backup1"}
                    ]
                }
            }
        })

        response_bytes = build_graphql_response(
            [json_string],
            "data",
            "Data",
            None,
            False,
        )

        response = json.loads(response_bytes)
        data = response["data"]["data"]

        assert "networkConfig" in data
        assert "primaryDns" in data["networkConfig"]
        assert "ipAddress" in data["networkConfig"]["primaryDns"]
        assert "backupServers" in data["networkConfig"]["primaryDns"]
        assert "serverName" in data["networkConfig"]["primaryDns"]["backupServers"][0]


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
```

---

## Implementation Steps

### Step 1: Create Unit Test File First
```bash
mkdir -p tests/unit/core
```

Create `tests/unit/core/test_jsonb_camelcase_conversion.py` with content above.

**Verification**:
```bash
uv run pytest tests/unit/core/test_jsonb_camelcase_conversion.py -v
```

**Expected**: `to_camel_case` tests PASS, `transform_json` and `build_graphql_response` tests may FAIL

### Step 2: Create Regression Test File
Create `tests/regression/test_jsonb_nested_camelcase.py` with content above.

**Verification**:
```bash
uv run pytest tests/regression/test_jsonb_nested_camelcase.py -v
```

**Expected**: Control test PASSES, other tests FAIL

### Step 3: Verify Test Failures Match Bug Report

**Expected Failure Patterns**:
```
PASSED test_single_word_nested_object_converts_to_camelcase
FAILED test_underscore_nested_object_converts_to_camelcase - "smtpServer" not in response
FAILED test_underscore_number_nested_object_is_present - "dns1" not in response
FAILED test_array_nested_objects_convert_to_camelcase - "printServers" not in response
```

---

## Acceptance Criteria

- [ ] Unit test file created with 8+ tests
- [ ] Regression test file created with 5 integration tests
- [ ] `to_camel_case()` unit tests PASS (function works correctly in isolation)
- [ ] `transform_json()` nested key tests reveal the bug
- [ ] Control test (single-word field) PASSES
- [ ] All underscore pattern tests FAIL with clear messages
- [ ] Tests follow FraiseQL test patterns (class-scoped fixtures, SchemaAwarePool)

---

## Commit Message

```
test(jsonb): add tests for nested JSONB camelCase conversion [RED]

Add test coverage for nested JSONB field name conversion:
- Unit tests for to_camel_case patterns (underscore, number suffix)
- Unit tests for transform_json nested key conversion
- Integration tests reproducing PrintOptim bug patterns

Tests demonstrate the bug where nested JSONB fields like smtp_server,
dns_1, and print_servers are not converted to camelCase (smtpServer,
dns1, printServers) in GraphQL responses.
```

---

## DO NOT

- Do NOT write any implementation code yet
- Do NOT modify existing test files
- Do NOT add tests for unrelated functionality

## DO

- DO verify Rust exports work before writing tests
- DO ensure test failures are descriptive
- DO follow existing FraiseQL test patterns
- DO create unit tests first (faster feedback loop)

---

**Next Phase**: Phase 2 - GREEN (Make all tests pass)
