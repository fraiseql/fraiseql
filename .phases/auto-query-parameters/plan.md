# Implementation Plan: Auto-Wire Query Parameters

## Overview

FraiseQL has an inconsistency in how query parameters are handled for list-returning queries. Currently only `where` is auto-wired, while `orderBy`, `limit`, `offset`, and Relay pagination parameters (`first`, `after`, `last`, `before`) are not.

## Current State

| Parameter | Type Generated | Auto-Wired to Resolvers |
|-----------|---------------|------------------------|
| `where` | `create_graphql_where_input()` | `_add_where_parameter_if_needed()` |
| `orderBy` | `create_graphql_order_by_input()` | Missing |
| `limit` | N/A (Int) | Missing |
| `offset` | N/A (Int) | Missing |
| `first` | N/A (Int) | Missing (Relay) |
| `after` | N/A (String) | Missing (Relay) |
| `last` | N/A (Int) | Missing (Relay) |
| `before` | N/A (String) | Missing (Relay) |

## Target State

All parameters should be auto-wired for queries returning `list[FraiseType]` or `Connection[FraiseType]`.

## Pre-Implementation Discovery

### Existing Infrastructure (Already Implemented)

**`FraiseQLRepository.find()` already supports all kwargs** (db.py:1657-1742):
- `limit` - extracted and applied (line 1737-1738)
- `offset` - extracted and applied (line 1741-1742)
- `order_by` - handles multiple formats (lines 1706-1734):
  - Objects with `.to_sql()` method
  - Objects with `._to_sql_order_by()` method
  - Dict format (converted via `_convert_order_by_input_to_sql`)
  - String format (raw SQL)

**`CQRSRepository._convert_order_by_to_tuples()`** (repository.py:604-636) handles:
- List of tuples: `[("created_at", "DESC"), ("id", "ASC")]`
- GraphQL dict/list format (auto-converted)
- OrderBySet objects with `.instructions` attribute

**This means most backend work is already done.** The main implementation is adding auto-wiring in `query_builder.py`.

---

## Phase 0: Test Infrastructure Setup

### Objective

Set up shared test fixtures following QA-recommended architecture. Use explicit patterns matching `test_graphql_query_execution_complete.py` instead of helper abstractions.

### Architecture Decision (QA Recommendation)

**DO NOT create `GraphQLTestHelper` class** - it adds unnecessary abstraction. Instead:
- Create focused fixture utilities in a new `tests/fixtures/graphql/` module
- Use explicit patterns in tests (schema building + execution visible)
- Split tests by scope: schema-only vs execution tests

### File Structure

```
tests/fixtures/graphql/
  ├── __init__.py
  └── conftest_graphql.py          # gql_mock_pool, gql_context, setup_graphql_table

tests/integration/graphql/queries/
  ├── test_query_parameters_schema.py       # Schema-only tests (no DB)
  ├── test_query_parameters_execution.py    # Execution tests (DB required)
  └── test_query_parameters_relay.py        # Relay tests (DB required)
```

### Phase 0.1: Create GraphQL Fixtures Module

**File: `tests/fixtures/graphql/__init__.py`**

```python
"""GraphQL testing fixtures for FraiseQL."""
```

**File: `tests/fixtures/graphql/conftest_graphql.py`**

```python
"""GraphQL-specific test fixtures.

These fixtures provide utilities for testing GraphQL schema generation
and query execution. They follow the explicit pattern from
test_graphql_query_execution_complete.py.

Fixture hierarchy:
- gql_mock_pool: Creates mock pool wrapping db_connection
- gql_context: GraphQL context dict with FraiseQLRepository
- setup_graphql_table: Factory to create JSONB tables/views
- seed_graphql_data: Factory to seed JSONB data
"""

from contextlib import asynccontextmanager
from typing import Any

import pytest
import pytest_asyncio
from unittest.mock import MagicMock


@pytest.fixture
def gql_mock_pool(db_connection):
    """Create a mock pool that wraps db_connection for FraiseQLRepository.

    This follows the pattern from test_graphql_query_execution_complete.py.

    Usage:
        def test_something(gql_mock_pool):
            repo = FraiseQLRepository(pool=gql_mock_pool)
    """
    mock_pool = MagicMock()

    @asynccontextmanager
    async def mock_connection():
        yield db_connection

    mock_pool.connection = mock_connection
    return mock_pool


@pytest.fixture
def gql_context(gql_mock_pool):
    """Create GraphQL context dict with FraiseQLRepository.

    Usage:
        async def test_query(gql_context):
            result = await execute_graphql(schema, query, context_value=gql_context)
    """
    from fraiseql.db import FraiseQLRepository

    return {"db": FraiseQLRepository(pool=gql_mock_pool)}


@pytest_asyncio.fixture
async def setup_graphql_table(db_connection, clear_registry):
    """Factory fixture to create JSONB-backed tables and views.

    Creates:
    - tb_{name}: Table with id (UUID) and data (JSONB)
    - v_{name}: View selecting id and data

    Usage:
        async def test_something(setup_graphql_table):
            await setup_graphql_table("users")
            # Creates tb_users and v_users
    """
    async def _setup(table_name: str, extra_columns: str | None = None):
        columns = "id UUID PRIMARY KEY DEFAULT gen_random_uuid(), data JSONB NOT NULL"
        if extra_columns:
            columns = f"{columns}, {extra_columns}"

        await db_connection.execute(f"""
            DROP TABLE IF EXISTS tb_{table_name} CASCADE;
            DROP VIEW IF EXISTS v_{table_name} CASCADE;

            CREATE TABLE tb_{table_name} ({columns});

            CREATE VIEW v_{table_name} AS
            SELECT id, data FROM tb_{table_name};
        """)

    return _setup


@pytest_asyncio.fixture
async def seed_graphql_data(db_connection):
    """Factory fixture to seed JSONB data into tables.

    Usage:
        async def test_something(setup_graphql_table, seed_graphql_data):
            await setup_graphql_table("users")
            await seed_graphql_data("tb_users", [
                {"name": "Alice", "age": 30},
                {"name": "Bob", "age": 25},
            ])
    """
    import json

    async def _seed(table_name: str, records: list[dict[str, Any]]):
        for record in records:
            json_str = json.dumps(record).replace("'", "''")
            await db_connection.execute(f"""
                INSERT INTO {table_name} (data) VALUES ('{json_str}'::jsonb)
            """)

    return _seed
```

### Phase 0.2: Update Root conftest.py

Add import to `tests/conftest.py`:

```python
# Add with other fixture imports
try:
    from tests.fixtures.graphql.conftest_graphql import (  # noqa: F401
        gql_mock_pool,
        gql_context,
        setup_graphql_table,
        seed_graphql_data,
    )
except ImportError:
    pass  # Skip if dependencies not available
```

### Phase 0.3: Verify Fixtures Work

**File: `tests/integration/graphql/queries/test_query_parameters_fixtures.py`**

```python
"""Smoke tests for GraphQL query parameter test fixtures."""

import pytest

pytestmark = pytest.mark.integration


class TestGraphQLFixtures:
    """Verify test fixtures work correctly."""

    @pytest.mark.asyncio
    async def test_setup_graphql_table_creates_table_and_view(
        self, db_connection, setup_graphql_table
    ):
        """setup_graphql_table should create table and view."""
        await setup_graphql_table("fixture_test")

        # Verify table exists
        result = await db_connection.execute("""
            SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_name = 'tb_fixture_test'
            )
        """)
        row = await result.fetchone()
        assert row[0] is True, "Table should exist"

        # Verify view exists
        result = await db_connection.execute("""
            SELECT EXISTS (
                SELECT FROM information_schema.views
                WHERE table_name = 'v_fixture_test'
            )
        """)
        row = await result.fetchone()
        assert row[0] is True, "View should exist"

    @pytest.mark.asyncio
    async def test_seed_graphql_data_inserts_records(
        self, db_connection, setup_graphql_table, seed_graphql_data
    ):
        """seed_graphql_data should insert JSONB records."""
        await setup_graphql_table("seed_test")
        await seed_graphql_data("tb_seed_test", [
            {"name": "Alice", "value": 1},
            {"name": "Bob", "value": 2},
        ])

        result = await db_connection.execute("SELECT COUNT(*) FROM tb_seed_test")
        row = await result.fetchone()
        assert row[0] == 2, "Should have 2 records"

    @pytest.mark.asyncio
    async def test_gql_context_provides_repository(self, gql_context):
        """gql_context should provide FraiseQLRepository."""
        from fraiseql.db import FraiseQLRepository

        assert "db" in gql_context
        assert isinstance(gql_context["db"], FraiseQLRepository)
```

### Verification Commands

```bash
uv run pytest tests/integration/graphql/queries/test_query_parameters_fixtures.py -v
```

---

## Phase 1: OrderBy Auto-Wiring

### Phase 1.1: RED - Write Failing Schema Tests

**File: `tests/integration/graphql/queries/test_query_parameters_schema.py`**

```python
"""Schema generation tests for auto-wired query parameters.

These tests verify that the GraphQL schema correctly includes auto-wired
parameters (orderBy, limit, offset) for queries returning list[FraiseType].

NO DATABASE REQUIRED - these tests only check schema generation.
"""

from dataclasses import dataclass
from uuid import UUID

import pytest
from graphql import GraphQLEnumType, GraphQLInputObjectType, GraphQLInt, GraphQLList

from fraiseql import query
from fraiseql import type as fraiseql_type
from fraiseql.gql.schema_builder import build_fraiseql_schema

pytestmark = pytest.mark.integration


class TestOrderBySchemaGeneration:
    """Test that orderBy parameter is auto-added to schema."""

    @pytest.fixture(autouse=True)
    def auto_clear(self, clear_registry):
        """Use standard clear_registry fixture."""
        yield

    def test_list_query_has_order_by_parameter(self):
        """Queries returning list[FraiseType] should have orderBy parameter."""
        @fraiseql_type(sql_source="v_order_by_users")
        @dataclass
        class OrderByUser:
            id: UUID
            name: str
            age: int

        @query
        async def order_by_users(info) -> list[OrderByUser]:
            return []

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("orderByUsers")

        assert field is not None, "Query field should exist"
        assert "orderBy" in field.args, "orderBy parameter should be auto-added"

    def test_order_by_parameter_is_list_type(self):
        """orderBy should be a list to support multiple sort criteria."""
        @fraiseql_type(sql_source="v_order_list_users")
        @dataclass
        class OrderListUser:
            id: UUID
            name: str

        @query
        async def order_list_users(info) -> list[OrderListUser]:
            return []

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("orderListUsers")
        order_by_arg = field.args["orderBy"]

        assert isinstance(order_by_arg.type, GraphQLList)

    def test_order_by_input_has_type_fields(self):
        """OrderByInput should have fields matching the return type."""
        @fraiseql_type(sql_source="v_order_fields_users")
        @dataclass
        class OrderFieldsUser:
            id: UUID
            name: str
            age: int
            email: str

        @query
        async def order_fields_users(info) -> list[OrderFieldsUser]:
            return []

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("orderFieldsUsers")
        inner_type = field.args["orderBy"].type.of_type

        assert isinstance(inner_type, GraphQLInputObjectType)
        assert "name" in inner_type.fields
        assert "age" in inner_type.fields
        assert "email" in inner_type.fields

    def test_order_by_field_is_enum_type(self):
        """Each field in OrderByInput should be OrderDirection enum."""
        @fraiseql_type(sql_source="v_order_enum_users")
        @dataclass
        class OrderEnumUser:
            id: UUID
            name: str

        @query
        async def order_enum_users(info) -> list[OrderEnumUser]:
            return []

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("orderEnumUsers")
        inner_type = field.args["orderBy"].type.of_type
        name_field = inner_type.fields["name"]

        field_type = name_field.type
        if hasattr(field_type, "of_type"):
            field_type = field_type.of_type

        assert isinstance(field_type, GraphQLEnumType)

    def test_manual_order_by_not_duplicated(self):
        """If resolver already has orderBy, don't add another."""
        @fraiseql_type(sql_source="v_manual_order_users")
        @dataclass
        class ManualOrderUser:
            id: UUID
            name: str

        OrderByInput = ManualOrderUser.OrderBy

        @query
        async def manual_order_users(
            info, order_by: OrderByInput | None = None
        ) -> list[ManualOrderUser]:
            return []

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("manualOrderUsers")

        order_params = [k for k in field.args.keys() if "order" in k.lower()]
        assert len(order_params) == 1, f"Should not duplicate: {order_params}"

    def test_single_return_type_no_order_by(self):
        """Single FraiseType return should NOT have orderBy."""
        @fraiseql_type(sql_source="v_single_user")
        @dataclass
        class SingleUser:
            id: UUID
            name: str

        @query
        async def single_user(info, id: UUID) -> SingleUser | None:
            return None

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("singleUser")

        assert "orderBy" not in field.args


class TestPaginationSchemaGeneration:
    """Test that limit/offset parameters are auto-added to schema."""

    @pytest.fixture(autouse=True)
    def auto_clear(self, clear_registry):
        yield

    def test_list_query_has_limit_parameter(self):
        """Queries returning list[FraiseType] should have limit parameter."""
        @fraiseql_type(sql_source="v_limit_items")
        @dataclass
        class LimitItem:
            id: UUID
            name: str

        @query
        async def limit_items(info) -> list[LimitItem]:
            return []

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("limitItems")

        assert "limit" in field.args

    def test_list_query_has_offset_parameter(self):
        """Queries returning list[FraiseType] should have offset parameter."""
        @fraiseql_type(sql_source="v_offset_items")
        @dataclass
        class OffsetItem:
            id: UUID
            name: str

        @query
        async def offset_items(info) -> list[OffsetItem]:
            return []

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("offsetItems")

        assert "offset" in field.args

    def test_limit_and_offset_are_int_type(self):
        """limit and offset should be Int type."""
        @fraiseql_type(sql_source="v_int_type_items")
        @dataclass
        class IntTypeItem:
            id: UUID

        @query
        async def int_type_items(info) -> list[IntTypeItem]:
            return []

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("intTypeItems")

        assert field.args["limit"].type == GraphQLInt
        assert field.args["offset"].type == GraphQLInt

    def test_manual_limit_not_duplicated(self):
        """If resolver already has limit, don't add another."""
        @fraiseql_type(sql_source="v_manual_limit_items")
        @dataclass
        class ManualLimitItem:
            id: UUID

        @query
        async def manual_limit_items(
            info, limit: int | None = None
        ) -> list[ManualLimitItem]:
            return []

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("manualLimitItems")

        limit_params = [k for k in field.args.keys() if k == "limit"]
        assert len(limit_params) == 1
```

### Phase 1.2: RED - Write Failing Execution Tests

**File: `tests/integration/graphql/queries/test_query_parameters_execution.py`**

```python
"""Execution tests for auto-wired query parameters.

These tests verify that orderBy, limit, offset parameters actually work
at runtime with real database queries.

DATABASE REQUIRED - tests execute real GraphQL queries.
"""

from dataclasses import dataclass

import pytest

from fraiseql import query
from fraiseql import type as fraiseql_type
from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.graphql.execute import execute_graphql

pytestmark = pytest.mark.integration


class TestOrderByExecution:
    """Test orderBy parameter works at runtime."""

    @pytest.mark.asyncio
    async def test_order_by_ascending(
        self, clear_registry, db_connection, gql_context,
        setup_graphql_table, seed_graphql_data
    ):
        """orderBy with ASC should sort ascending."""
        await setup_graphql_table("order_asc_users")
        await seed_graphql_data("tb_order_asc_users", [
            {"name": "Charlie", "age": 35},
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25},
        ])

        @fraiseql_type(sql_source="v_order_asc_users", jsonb_column="data")
        @dataclass
        class OrderAscUser:
            id: str
            name: str
            age: int

        @query
        async def order_asc_users(info, order_by=None) -> list[OrderAscUser]:
            db = info.context["db"]
            return await db.find("v_order_asc_users", info=info, order_by=order_by)

        schema = build_fraiseql_schema(query_types=[OrderAscUser, order_asc_users])
        result = await execute_graphql(
            schema,
            '{ orderAscUsers(orderBy: [{age: ASC}]) { name age } }',
            context_value=gql_context,
        )

        assert result.errors is None
        users = result.data["orderAscUsers"]
        ages = [u["age"] for u in users]
        assert ages == sorted(ages), f"Should be ascending: {ages}"

    @pytest.mark.asyncio
    async def test_order_by_descending(
        self, clear_registry, db_connection, gql_context,
        setup_graphql_table, seed_graphql_data
    ):
        """orderBy with DESC should sort descending."""
        await setup_graphql_table("order_desc_users")
        await seed_graphql_data("tb_order_desc_users", [
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25},
            {"name": "Charlie", "age": 35},
        ])

        @fraiseql_type(sql_source="v_order_desc_users", jsonb_column="data")
        @dataclass
        class OrderDescUser:
            id: str
            name: str
            age: int

        @query
        async def order_desc_users(info, order_by=None) -> list[OrderDescUser]:
            db = info.context["db"]
            return await db.find("v_order_desc_users", info=info, order_by=order_by)

        schema = build_fraiseql_schema(query_types=[OrderDescUser, order_desc_users])
        result = await execute_graphql(
            schema,
            '{ orderDescUsers(orderBy: [{age: DESC}]) { name age } }',
            context_value=gql_context,
        )

        assert result.errors is None
        users = result.data["orderDescUsers"]
        ages = [u["age"] for u in users]
        assert ages == sorted(ages, reverse=True), f"Should be descending: {ages}"

    @pytest.mark.asyncio
    async def test_order_by_string_field(
        self, clear_registry, db_connection, gql_context,
        setup_graphql_table, seed_graphql_data
    ):
        """orderBy should work on string fields."""
        await setup_graphql_table("order_str_users")
        await seed_graphql_data("tb_order_str_users", [
            {"name": "Charlie"},
            {"name": "Alice"},
            {"name": "Bob"},
        ])

        @fraiseql_type(sql_source="v_order_str_users", jsonb_column="data")
        @dataclass
        class OrderStrUser:
            id: str
            name: str

        @query
        async def order_str_users(info, order_by=None) -> list[OrderStrUser]:
            db = info.context["db"]
            return await db.find("v_order_str_users", info=info, order_by=order_by)

        schema = build_fraiseql_schema(query_types=[OrderStrUser, order_str_users])
        result = await execute_graphql(
            schema,
            '{ orderStrUsers(orderBy: [{name: ASC}]) { name } }',
            context_value=gql_context,
        )

        assert result.errors is None
        names = [u["name"] for u in result.data["orderStrUsers"]]
        assert names == sorted(names)

    @pytest.mark.asyncio
    async def test_order_by_with_where(
        self, clear_registry, db_connection, gql_context,
        setup_graphql_table, seed_graphql_data
    ):
        """orderBy should work with where clause."""
        await setup_graphql_table("order_where_users")
        await seed_graphql_data("tb_order_where_users", [
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25},
            {"name": "Charlie", "age": 35},
            {"name": "Diana", "age": 20},
        ])

        @fraiseql_type(sql_source="v_order_where_users", jsonb_column="data")
        @dataclass
        class OrderWhereUser:
            id: str
            name: str
            age: int

        @query
        async def order_where_users(
            info, where=None, order_by=None
        ) -> list[OrderWhereUser]:
            db = info.context["db"]
            return await db.find(
                "v_order_where_users", info=info, where=where, order_by=order_by
            )

        schema = build_fraiseql_schema(query_types=[OrderWhereUser, order_where_users])
        result = await execute_graphql(
            schema,
            '''{ orderWhereUsers(where: {age: {gte: 25}}, orderBy: [{age: DESC}]) { name age } }''',
            context_value=gql_context,
        )

        assert result.errors is None
        users = result.data["orderWhereUsers"]
        ages = [u["age"] for u in users]

        assert all(age >= 25 for age in ages)
        assert ages == sorted(ages, reverse=True)


class TestPaginationExecution:
    """Test limit/offset parameters work at runtime."""

    @pytest.mark.asyncio
    async def test_limit_restricts_results(
        self, clear_registry, db_connection, gql_context,
        setup_graphql_table, seed_graphql_data
    ):
        """limit should restrict number of results."""
        await setup_graphql_table("limit_exec_items")
        await seed_graphql_data("tb_limit_exec_items", [
            {"name": f"Item {i}", "seq": i} for i in range(20)
        ])

        @fraiseql_type(sql_source="v_limit_exec_items", jsonb_column="data")
        @dataclass
        class LimitExecItem:
            id: str
            name: str

        @query
        async def limit_exec_items(info, limit=None) -> list[LimitExecItem]:
            db = info.context["db"]
            return await db.find("v_limit_exec_items", info=info, limit=limit)

        schema = build_fraiseql_schema(query_types=[LimitExecItem, limit_exec_items])
        result = await execute_graphql(
            schema,
            '{ limitExecItems(limit: 5) { name } }',
            context_value=gql_context,
        )

        assert result.errors is None
        assert len(result.data["limitExecItems"]) == 5

    @pytest.mark.asyncio
    async def test_offset_skips_results(
        self, clear_registry, db_connection, gql_context,
        setup_graphql_table, seed_graphql_data
    ):
        """offset should skip initial results."""
        await setup_graphql_table("offset_exec_items")
        await seed_graphql_data("tb_offset_exec_items", [
            {"name": f"Item {i}", "seq": i} for i in range(20)
        ])

        @fraiseql_type(sql_source="v_offset_exec_items", jsonb_column="data")
        @dataclass
        class OffsetExecItem:
            id: str
            seq: int

        @query
        async def offset_exec_items(
            info, order_by=None, offset=None
        ) -> list[OffsetExecItem]:
            db = info.context["db"]
            return await db.find(
                "v_offset_exec_items", info=info, order_by=order_by, offset=offset
            )

        schema = build_fraiseql_schema(query_types=[OffsetExecItem, offset_exec_items])

        # Get all ordered
        all_result = await execute_graphql(
            schema,
            '{ offsetExecItems(orderBy: [{seq: ASC}]) { seq } }',
            context_value=gql_context,
        )

        # Get with offset
        offset_result = await execute_graphql(
            schema,
            '{ offsetExecItems(orderBy: [{seq: ASC}], offset: 5) { seq } }',
            context_value=gql_context,
        )

        assert offset_result.errors is None
        all_items = all_result.data["offsetExecItems"]
        offset_items = offset_result.data["offsetExecItems"]
        assert offset_items[0]["seq"] == all_items[5]["seq"]

    @pytest.mark.asyncio
    async def test_limit_and_offset_together(
        self, clear_registry, db_connection, gql_context,
        setup_graphql_table, seed_graphql_data
    ):
        """limit and offset should work together."""
        await setup_graphql_table("page_exec_items")
        await seed_graphql_data("tb_page_exec_items", [
            {"seq": i} for i in range(20)
        ])

        @fraiseql_type(sql_source="v_page_exec_items", jsonb_column="data")
        @dataclass
        class PageExecItem:
            id: str
            seq: int

        @query
        async def page_exec_items(
            info, order_by=None, limit=None, offset=None
        ) -> list[PageExecItem]:
            db = info.context["db"]
            return await db.find(
                "v_page_exec_items", info=info,
                order_by=order_by, limit=limit, offset=offset
            )

        schema = build_fraiseql_schema(query_types=[PageExecItem, page_exec_items])
        result = await execute_graphql(
            schema,
            '{ pageExecItems(orderBy: [{seq: ASC}], limit: 5, offset: 10) { seq } }',
            context_value=gql_context,
        )

        assert result.errors is None
        items = result.data["pageExecItems"]
        assert len(items) == 5
        assert items[0]["seq"] == 10

    @pytest.mark.asyncio
    async def test_negative_limit_returns_error(
        self, clear_registry, db_connection, gql_context, setup_graphql_table
    ):
        """Negative limit should return error."""
        await setup_graphql_table("neg_limit_items")

        @fraiseql_type(sql_source="v_neg_limit_items", jsonb_column="data")
        @dataclass
        class NegLimitItem:
            id: str

        @query
        async def neg_limit_items(info, limit=None) -> list[NegLimitItem]:
            db = info.context["db"]
            return await db.find("v_neg_limit_items", info=info, limit=limit)

        schema = build_fraiseql_schema(query_types=[NegLimitItem, neg_limit_items])
        result = await execute_graphql(
            schema,
            '{ negLimitItems(limit: -1) { id } }',
            context_value=gql_context,
        )

        assert result.errors is not None

    @pytest.mark.asyncio
    async def test_negative_offset_returns_error(
        self, clear_registry, db_connection, gql_context, setup_graphql_table
    ):
        """Negative offset should return error."""
        await setup_graphql_table("neg_offset_items")

        @fraiseql_type(sql_source="v_neg_offset_items", jsonb_column="data")
        @dataclass
        class NegOffsetItem:
            id: str

        @query
        async def neg_offset_items(info, offset=None) -> list[NegOffsetItem]:
            db = info.context["db"]
            return await db.find("v_neg_offset_items", info=info, offset=offset)

        schema = build_fraiseql_schema(query_types=[NegOffsetItem, neg_offset_items])
        result = await execute_graphql(
            schema,
            '{ negOffsetItems(offset: -1) { id } }',
            context_value=gql_context,
        )

        assert result.errors is not None


class TestAllParametersCombined:
    """Test all parameters work together."""

    @pytest.mark.asyncio
    async def test_where_order_by_limit_offset_combined(
        self, clear_registry, db_connection, gql_context,
        setup_graphql_table, seed_graphql_data
    ):
        """All parameters should work together."""
        await setup_graphql_table("combined_items")
        await seed_graphql_data("tb_combined_items", [
            {"name": f"Item {i}", "category": "A" if i % 2 == 0 else "B", "seq": i}
            for i in range(30)
        ])

        @fraiseql_type(sql_source="v_combined_items", jsonb_column="data")
        @dataclass
        class CombinedItem:
            id: str
            name: str
            category: str
            seq: int

        @query
        async def combined_items(
            info, where=None, order_by=None, limit=None, offset=None
        ) -> list[CombinedItem]:
            db = info.context["db"]
            return await db.find(
                "v_combined_items", info=info,
                where=where, order_by=order_by, limit=limit, offset=offset
            )

        schema = build_fraiseql_schema(query_types=[CombinedItem, combined_items])
        result = await execute_graphql(
            schema,
            '''{ combinedItems(
                where: {category: {eq: "A"}},
                orderBy: [{seq: DESC}],
                limit: 5,
                offset: 2
            ) { category seq } }''',
            context_value=gql_context,
        )

        assert result.errors is None
        items = result.data["combinedItems"]

        assert all(item["category"] == "A" for item in items)
        assert len(items) == 5
        seqs = [item["seq"] for item in items]
        assert seqs == sorted(seqs, reverse=True)
```

### Phase 1.3: GREEN - Implement OrderBy Auto-Wiring

**File: `src/fraiseql/gql/builders/query_builder.py`**

Add duplicate check to `_add_where_parameter_if_needed()`:

```python
def _add_where_parameter_if_needed(
    self, gql_args: dict[str, GraphQLArgument], return_type: Any
) -> None:
    """Add where parameter to GraphQL args if query returns list of Fraise types."""
    # Don't add if already present
    if "where" in gql_args:
        return

    should_add, element_type = self._should_add_where_parameter(return_type)
    if should_add and element_type:
        from fraiseql.sql.graphql_where_generator import create_graphql_where_input

        where_input_type = create_graphql_where_input(element_type)
        self.registry.register_type(where_input_type)
        gql_where_type = convert_type_to_graphql_input(where_input_type)
        gql_args["where"] = GraphQLArgument(gql_where_type)
```

Add new method after `_add_where_parameter_if_needed()`:

```python
def _add_order_by_parameter_if_needed(
    self, gql_args: dict[str, GraphQLArgument], return_type: Any
) -> None:
    """Add orderBy parameter to GraphQL args if query returns list of Fraise types."""
    from graphql import GraphQLList
    from fraiseql.sql.graphql_order_by_generator import create_graphql_order_by_input

    if "orderBy" in gql_args:
        return

    should_add, element_type = self._should_add_where_parameter(return_type)
    if should_add and element_type:
        order_by_input_type = create_graphql_order_by_input(element_type)
        self.registry.register_type(order_by_input_type)
        gql_order_by_type = convert_type_to_graphql_input(order_by_input_type)
        gql_args["orderBy"] = GraphQLArgument(GraphQLList(gql_order_by_type))


def _add_pagination_parameters_if_needed(
    self, gql_args: dict[str, GraphQLArgument], return_type: Any
) -> None:
    """Add limit/offset parameters if query returns list of Fraise types."""
    from graphql import GraphQLInt

    should_add, _ = self._should_add_where_parameter(return_type)
    if should_add:
        if "limit" not in gql_args:
            gql_args["limit"] = GraphQLArgument(GraphQLInt)
        if "offset" not in gql_args:
            gql_args["offset"] = GraphQLArgument(GraphQLInt)
```

Update `_add_query_functions()` (around line 156):

```python
# Automatically add parameters for list[FraiseType] queries
self._add_where_parameter_if_needed(gql_args, hints["return"])
self._add_order_by_parameter_if_needed(gql_args, hints["return"])
self._add_pagination_parameters_if_needed(gql_args, hints["return"])
```

Add validation in `_create_gql_resolver()`:

```python
def _validate_pagination_params(kwargs: dict[str, Any]) -> None:
    """Validate pagination parameters are non-negative."""
    from graphql import GraphQLError

    for param in ("limit", "offset", "first", "last"):
        if param in kwargs and kwargs[param] is not None:
            if kwargs[param] < 0:
                raise GraphQLError(f"{param} must be non-negative")

# In async_resolver, after WHERE validation:
_validate_pagination_params(kwargs)
```

### Verification Commands

```bash
uv run pytest tests/integration/graphql/queries/test_query_parameters_schema.py -v
uv run pytest tests/integration/graphql/queries/test_query_parameters_execution.py -v
```

---

## Phase 2: Relay Pagination Auto-Wiring

### Phase 2.1: RED - Write Failing Tests

**File: `tests/integration/graphql/queries/test_query_parameters_relay.py`**

```python
"""Relay pagination tests for auto-wired Connection query parameters.

These tests verify that queries returning Connection[FraiseType] automatically
get first/after/last/before parameters.
"""

from dataclasses import dataclass
from uuid import UUID

import pytest
from graphql import GraphQLInt, GraphQLString

from fraiseql import query
from fraiseql import type as fraiseql_type
from fraiseql.gql.schema_builder import build_fraiseql_schema

pytestmark = pytest.mark.integration


# Check if Connection type exists
try:
    from fraiseql.types.generic import Connection
    HAS_CONNECTION = True
except ImportError:
    HAS_CONNECTION = False


@pytest.mark.skipif(not HAS_CONNECTION, reason="Connection type not available")
class TestRelaySchemaGeneration:
    """Test that Relay parameters are auto-added to schema."""

    @pytest.fixture(autouse=True)
    def auto_clear(self, clear_registry):
        yield

    def test_connection_query_has_first_parameter(self):
        """Connection queries should have 'first' parameter."""
        from fraiseql.types.generic import Connection

        @fraiseql_type(sql_source="v_relay_posts")
        @dataclass
        class RelayPost:
            id: UUID
            title: str

        @query
        async def relay_posts(info) -> Connection[RelayPost]:
            return None

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("relayPosts")

        assert field is not None
        assert "first" in field.args

    def test_connection_query_has_after_parameter(self):
        """Connection queries should have 'after' parameter."""
        from fraiseql.types.generic import Connection

        @fraiseql_type(sql_source="v_relay_after_posts")
        @dataclass
        class RelayAfterPost:
            id: UUID

        @query
        async def relay_after_posts(info) -> Connection[RelayAfterPost]:
            return None

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("relayAfterPosts")

        assert "after" in field.args

    def test_connection_query_has_last_and_before(self):
        """Connection queries should have 'last' and 'before' parameters."""
        from fraiseql.types.generic import Connection

        @fraiseql_type(sql_source="v_relay_back_posts")
        @dataclass
        class RelayBackPost:
            id: UUID

        @query
        async def relay_back_posts(info) -> Connection[RelayBackPost]:
            return None

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("relayBackPosts")

        assert "last" in field.args
        assert "before" in field.args

    def test_first_is_int_after_is_string(self):
        """first should be Int, after should be String."""
        from fraiseql.types.generic import Connection

        @fraiseql_type(sql_source="v_relay_types")
        @dataclass
        class RelayTypesPost:
            id: UUID

        @query
        async def relay_types(info) -> Connection[RelayTypesPost]:
            return None

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("relayTypes")

        assert field.args["first"].type == GraphQLInt
        assert field.args["after"].type == GraphQLString

    def test_connection_also_has_where_and_order_by(self):
        """Connection queries should have where and orderBy."""
        from fraiseql.types.generic import Connection

        @fraiseql_type(sql_source="v_relay_full_posts")
        @dataclass
        class RelayFullPost:
            id: UUID
            title: str

        @query
        async def relay_full_posts(info) -> Connection[RelayFullPost]:
            return None

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("relayFullPosts")

        assert "where" in field.args
        assert "orderBy" in field.args

    def test_list_query_no_relay_params(self):
        """Regular list queries should NOT get Relay parameters."""
        @fraiseql_type(sql_source="v_list_no_relay")
        @dataclass
        class ListNoRelay:
            id: UUID

        @query
        async def list_no_relay(info) -> list[ListNoRelay]:
            return []

        schema = build_fraiseql_schema()
        field = schema.query_type.fields.get("listNoRelay")

        assert "first" not in field.args
        assert "after" not in field.args
        assert "last" not in field.args
        assert "before" not in field.args


@pytest.mark.skipif(not HAS_CONNECTION, reason="Connection type not available")
class TestRelayValidation:
    """Test validation for Relay parameters."""

    @pytest.mark.asyncio
    async def test_negative_first_returns_error(
        self, clear_registry, db_connection, gql_context, setup_graphql_table
    ):
        """Negative first should return error."""
        from fraiseql.types.generic import Connection
        from fraiseql.graphql.execute import execute_graphql

        await setup_graphql_table("neg_first_posts")

        @fraiseql_type(sql_source="v_neg_first_posts", jsonb_column="data")
        @dataclass
        class NegFirstPost:
            id: str

        @query
        async def neg_first_posts(info, first=None) -> Connection[NegFirstPost]:
            return {"edges": [], "pageInfo": {}, "totalCount": 0}

        schema = build_fraiseql_schema(query_types=[NegFirstPost, neg_first_posts])
        result = await execute_graphql(
            schema,
            '{ negFirstPosts(first: -1) { edges { cursor } } }',
            context_value=gql_context,
        )

        assert result.errors is not None

    @pytest.mark.asyncio
    async def test_negative_last_returns_error(
        self, clear_registry, db_connection, gql_context, setup_graphql_table
    ):
        """Negative last should return error."""
        from fraiseql.types.generic import Connection
        from fraiseql.graphql.execute import execute_graphql

        await setup_graphql_table("neg_last_posts")

        @fraiseql_type(sql_source="v_neg_last_posts", jsonb_column="data")
        @dataclass
        class NegLastPost:
            id: str

        @query
        async def neg_last_posts(info, last=None) -> Connection[NegLastPost]:
            return {"edges": [], "pageInfo": {}, "totalCount": 0}

        schema = build_fraiseql_schema(query_types=[NegLastPost, neg_last_posts])
        result = await execute_graphql(
            schema,
            '{ negLastPosts(last: -1) { edges { cursor } } }',
            context_value=gql_context,
        )

        assert result.errors is not None
```

### Phase 2.2: GREEN - Implement Relay Auto-Wiring

**File: `src/fraiseql/gql/builders/query_builder.py`**

```python
def _should_add_relay_parameters(self, return_type: Any) -> tuple[bool, Any | None]:
    """Check if query should get Relay pagination parameters."""
    try:
        from fraiseql.types.generic import Connection
    except ImportError:
        return False, None

    origin = get_origin(return_type)
    if origin is Connection:
        args = get_args(return_type)
        if args and self._is_fraise_type(args[0]):
            return True, args[0]

    return False, None


def _add_relay_parameters_if_needed(
    self, gql_args: dict[str, GraphQLArgument], return_type: Any
) -> None:
    """Add Relay pagination parameters if query returns Connection[T]."""
    from graphql import GraphQLInt, GraphQLList, GraphQLString
    from fraiseql.sql.graphql_order_by_generator import create_graphql_order_by_input
    from fraiseql.sql.graphql_where_generator import create_graphql_where_input

    should_add, element_type = self._should_add_relay_parameters(return_type)
    if not should_add or not element_type:
        return

    # Forward pagination
    if "first" not in gql_args:
        gql_args["first"] = GraphQLArgument(GraphQLInt)
    if "after" not in gql_args:
        gql_args["after"] = GraphQLArgument(GraphQLString)

    # Backward pagination
    if "last" not in gql_args:
        gql_args["last"] = GraphQLArgument(GraphQLInt)
    if "before" not in gql_args:
        gql_args["before"] = GraphQLArgument(GraphQLString)

    # Also add where
    if "where" not in gql_args:
        where_input_type = create_graphql_where_input(element_type)
        self.registry.register_type(where_input_type)
        gql_args["where"] = GraphQLArgument(
            convert_type_to_graphql_input(where_input_type)
        )

    # Also add orderBy
    if "orderBy" not in gql_args:
        order_by_input_type = create_graphql_order_by_input(element_type)
        self.registry.register_type(order_by_input_type)
        gql_args["orderBy"] = GraphQLArgument(
            GraphQLList(convert_type_to_graphql_input(order_by_input_type))
        )
```

Update `_add_query_functions()`:

```python
# Check for Connection[T] first (Relay pagination)
is_relay, _ = self._should_add_relay_parameters(hints["return"])
if is_relay:
    self._add_relay_parameters_if_needed(gql_args, hints["return"])
else:
    # Standard list[T] - add where, orderBy, limit, offset
    self._add_where_parameter_if_needed(gql_args, hints["return"])
    self._add_order_by_parameter_if_needed(gql_args, hints["return"])
    self._add_pagination_parameters_if_needed(gql_args, hints["return"])
```

### Verification Commands

```bash
uv run pytest tests/integration/graphql/queries/test_query_parameters_relay.py -v
```

---

## Summary

### Test Files (QA-Recommended Architecture)

| File | Purpose | DB Required |
|------|---------|-------------|
| `tests/fixtures/graphql/conftest_graphql.py` | Shared fixtures | N/A |
| `test_query_parameters_fixtures.py` | Smoke tests | Yes |
| `test_query_parameters_schema.py` | Schema generation | No |
| `test_query_parameters_execution.py` | Query execution | Yes |
| `test_query_parameters_relay.py` | Relay pagination | Yes |

### Implementation Files

| File | Changes |
|------|---------|
| `src/fraiseql/gql/builders/query_builder.py` | Add `_add_order_by_parameter_if_needed()`, `_add_pagination_parameters_if_needed()`, `_add_relay_parameters_if_needed()`, `_should_add_relay_parameters()`, `_validate_pagination_params()`. Update `_add_where_parameter_if_needed()` with duplicate check. |

### Test Counts

- Fixtures smoke tests: 3
- Schema tests: 10
- Execution tests: 9
- Relay tests: 8
- **Total: ~30 tests**

### Pre-Existing Functionality (No Changes Needed)

- `FraiseQLRepository.find()` - handles kwargs (db.py:1657-1742)
- `CQRSRepository._convert_order_by_to_tuples()` - handles GraphQL format (repository.py:604-636)
- `CQRSRepository.paginate()` - Relay pagination (repository.py:469-529)

### Execution Order

1. Phase 0: Create fixtures module, update root conftest, verify
2. Phase 1: OrderBy + limit/offset (schema tests → execution tests → implementation)
3. Phase 2: Relay (schema tests → validation tests → implementation)

### Final Verification

```bash
# All query parameter tests
uv run pytest tests/integration/graphql/queries/test_query_parameters*.py -v

# Full regression
uv run pytest --tb=short -q
```
