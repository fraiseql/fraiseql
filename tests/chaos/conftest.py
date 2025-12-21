# Chaos Engineering Test Configuration
#
# This module provides fixtures and configuration for chaos engineering tests.
# Chaos tests inject failures into FraiseQL to validate resilience and recovery.

import sys
import os
import pytest

# Add tests directory to Python path for chaos module imports
tests_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if tests_dir not in sys.path:
    sys.path.insert(0, tests_dir)

from chaos.base import ChaosTestCase
from chaos.fixtures import ToxiproxyManager
from chaos.base import ChaosMetrics

# Import chaos database fixtures
pytest_plugins = ["chaos.database_fixtures"]


@pytest.fixture
def chaos_test_case():
    """Base test case for chaos engineering tests."""
    return ChaosTestCase()


@pytest.fixture
def toxiproxy():
    """Toxiproxy manager for network chaos injection."""
    manager = ToxiproxyManager()
    yield manager
    # Cleanup all proxies after test
    for proxy_name in list(manager.proxies.keys()):
        try:
            manager.delete_proxy(proxy_name)
        except:
            pass

# For unittest-style tests, provide a default toxiproxy instance
_default_toxiproxy = ToxiproxyManager()


@pytest.fixture
def chaos_metrics():
    """Chaos metrics collector."""
    return ChaosMetrics()


# Register chaos test markers
def pytest_configure(config):
    config.addinivalue_line("markers", "chaos: marks tests as chaos engineering tests")
    config.addinivalue_line("markers", "chaos_network: network-related chaos tests")
    config.addinivalue_line("markers", "chaos_database: database-related chaos tests")
    config.addinivalue_line("markers", "chaos_cache: cache-related chaos tests")
    config.addinivalue_line("markers", "chaos_auth: authentication-related chaos tests")
    config.addinivalue_line("markers", "chaos_resources: resource-related chaos tests")
    config.addinivalue_line("markers", "chaos_concurrency: concurrency-related chaos tests")
