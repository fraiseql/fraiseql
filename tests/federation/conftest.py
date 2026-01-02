"""Pytest configuration for federation tests."""

import pytest

from fraiseql.federation import clear_entity_registry, reset_default_resolver


@pytest.fixture(autouse=True)
def reset_federation_state():
    """Reset federation state before and after each test."""
    # Clear before test
    clear_entity_registry()
    reset_default_resolver()

    yield

    # Clear after test (optional, but good for isolation)
    clear_entity_registry()
    reset_default_resolver()
