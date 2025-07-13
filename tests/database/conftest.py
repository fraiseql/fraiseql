"""Conftest for database tests only.

This file imports database fixtures to avoid loading Docker/Podman dependencies
in unit tests that don't need them.
"""

# Import all database fixtures for database tests
from tests.database_conftest import *  # noqa: F403
