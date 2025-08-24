"""
E2E Test Configuration for FraiseQL Blog Demos
Modern pytest configuration with both simple and enterprise blog demos.
"""

import pytest


@pytest.mark.e2e  
@pytest.mark.database
def pytest_configure(config):
    """Configure E2E test markers."""
    config.addinivalue_line("markers", "blog_demo: Blog demo specific tests")
    config.addinivalue_line("markers", "blog_demo_simple: Simple blog demo tests")
    config.addinivalue_line("markers", "blog_demo_enterprise: Enterprise blog demo tests")