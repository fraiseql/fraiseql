"""Test that store_response_in_cache passes context correctly."""

from unittest.mock import Mock

import pytest
from tests.integration.test_apq_context_propagation import ContextCapturingBackend

from fraiseql.middleware.apq_caching import store_response_in_cache

pytestmark = pytest.mark.integration


def test_store_response_passes_context() -> None:
    """Test that store_response_in_cache passes context to backend."""
    backend = ContextCapturingBackend()

    config = Mock()
    config.apq_cache_responses = True

    test_context = {"user": {"user_id": "test-123", "metadata": {"tenant_id": "tenant-abc"}}}

    test_response = {"data": {"result": "test"}}

    # Call store_response_in_cache with context
    store_response_in_cache("hash123", test_response, backend, config, context=test_context)

    # Verify context was passed
    assert backend.captured_store_context is not None
    assert backend.captured_store_context["user"]["metadata"]["tenant_id"] == "tenant-abc"
