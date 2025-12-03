"""Shared mock classes for FraiseQL tests.

These mocks simulate the async database connection pattern used
by the Rust-first mutation pipeline.
"""

from __future__ import annotations

import json
from typing import Any


class MockCursor:
    """Mock async cursor context manager."""

    def __init__(self, return_data: str | None = None) -> None:
        self._return_data = return_data or '{"status": "success"}'

    async def __aenter__(self) -> MockCursor:
        return self

    async def __aexit__(self, *args: object) -> None:
        pass

    async def fetchone(self) -> tuple[str]:
        return (self._return_data,)

    async def fetchall(self) -> list[tuple[str]]:
        return [(self._return_data,)]


class MockConnection:
    """Mock async connection context manager."""

    def __init__(self, cursor: MockCursor | None = None) -> None:
        self._cursor = cursor or MockCursor()

    async def __aenter__(self) -> MockConnection:
        return self

    async def __aexit__(self, *args: object) -> None:
        pass

    def cursor(self) -> MockCursor:
        return self._cursor

    async def execute(self, *args: Any, **kwargs: Any) -> None:
        pass


class MockPool:
    """Mock connection pool."""

    def __init__(self, connection: MockConnection | None = None) -> None:
        self._connection = connection or MockConnection()

    def connection(self) -> MockConnection:
        return self._connection


class MockDatabase:
    """Mock database with connection pool support.

    Tracks function calls and input data for test assertions.
    """

    def __init__(
        self,
        pool: MockPool | None = None,
        return_data: str | None = None,
    ) -> None:
        self.last_function_call: str | None = None
        self.last_input_data: dict[str, Any] | None = None
        self._return_data = return_data
        self._pool = pool or MockPool(MockConnection(MockCursor(return_data)))

    def get_pool(self) -> MockPool:
        return self._pool


class MockRustResponseBytes:
    """Mock for fraiseql.core.rust_pipeline.RustResponseBytes."""

    def __init__(self, data: bytes | dict[str, Any]) -> None:
        if isinstance(data, dict):
            self._data = json.dumps(data).encode()
        else:
            self._data = data

    def to_json(self) -> dict[str, Any]:
        return json.loads(self._data)

    @property
    def content_type(self) -> str:
        return "application/json"
