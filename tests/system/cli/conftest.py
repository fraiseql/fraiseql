"""Shared fixtures for CLI tests."""

import tempfile
from collections.abc import Generator
from pathlib import Path

import pytest
from click.testing import CliRunner

from fraiseql.gql.schema_builder import SchemaRegistry


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear the schema registry before and after each test."""
    # Clear before test
    registry = SchemaRegistry.get_instance()
    registry.clear()

    yield

    # Clear after test
    registry.clear()


@pytest.fixture
def cli_runner() -> CliRunner:
    """Create a Click test runner."""
    return CliRunner()


@pytest.fixture
def temp_project_dir(monkeypatch) -> Generator[Path]:
    """Create a temporary directory for project testing."""
    with tempfile.TemporaryDirectory() as tmpdir:
        monkeypatch.chdir(tmpdir)
        yield Path(tmpdir)


@pytest.fixture
def mock_database_url(monkeypatch) -> None:
    """Mock DATABASE_URL environment variable."""
    monkeypatch.setenv("DATABASE_URL", "postgresql://test:test@localhost/testdb")


@pytest.fixture
def sample_pyproject_toml() -> str:
    """Sample pyproject.toml content."""
    return """[project]
name = "test-project"
version = "0.1.0"
dependencies = ["fraiseql>=0.1.0"]
"""


@pytest.fixture
def sample_main_py() -> str:
    """Sample main.py content for testing."""
    return '''"""Test app."""
import fraiseql

@fraiseql.type
class User:
    id: int
    name: str

app = fraiseql.create_fraiseql_app(types=[User])
'''


@pytest.fixture
def sample_type_file() -> str:
    """Sample type file content."""
    return '''"""User type."""
import fraiseql
from fraiseql import fraise_field

@fraiseql.type
class User:
    id: int
    name: str = fraise_field(description="User name")
    email: str = fraise_field(description="Email address")
'''
