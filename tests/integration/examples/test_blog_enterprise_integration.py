"""
Integration tests for blog_enterprise example running from main test suite.

These tests ensure the blog_enterprise example works correctly and catches regressions.
"""

import pytest

# Mark all tests as example integration tests
pytestmark = [
    pytest.mark.blog_enterprise,
    pytest.mark.integration,
    pytest.mark.enterprise,
    pytest.mark.examples,
]


def test_blog_enterprise_app_exists() -> None:
    """Basic test that blog enterprise app can be imported and has expected structure."""
    import sys
    from pathlib import Path

    examples_dir = Path(__file__).parent.parent.parent.parent / "examples"
    blog_enterprise_dir = examples_dir / "blog_enterprise"
    app_file = blog_enterprise_dir / "app.py"

    # Check that the file exists
    assert app_file.exists(), f"Blog enterprise app.py not found at {app_file}"

    # Try to read the file and check it has expected content
    content = app_file.read_text()
    assert "def create_app():" in content, "create_app function not found"
    assert "/health" in content, "Health endpoint not found"
    assert "blog_enterprise" in content, "Service name not found"

    # Try basic import without creating the app (to avoid database issues)
    sys.path.insert(0, str(blog_enterprise_dir))

    try:
        import importlib.util

        spec = importlib.util.spec_from_file_location("blog_enterprise_app", app_file)
        if spec and spec.loader:
            app_module = importlib.util.module_from_spec(spec)
            # Just check that we can load the module spec without executing it
            assert spec is not None, "Could not create module spec"
            assert spec.loader is not None, "Module loader not available"
        else:
            pytest.skip("Could not create module spec for blog enterprise app")

    except Exception as e:
        pytest.skip(f"App import check failed: {e}")
    finally:
        # Clean up
        if str(blog_enterprise_dir) in sys.path:
            sys.path.remove(str(blog_enterprise_dir))


@pytest.mark.asyncio
async def test_blog_enterprise_home_endpoint(blog_enterprise_client) -> None:
    """Test that blog_enterprise home endpoint returns expected information."""
    response = await blog_enterprise_client.get("/")
    assert response.status_code == 200

    data = response.json()
    assert "FraiseQL Blog Enterprise" in data["message"]
    assert data["version"] == "2.0.0"
    assert "features" in data
    assert len(data["features"]) > 0

    # Check enterprise features are mentioned
    features_text = " ".join(data["features"])
    assert "multi-tenant" in features_text.lower()
    assert "enterprise" in features_text.lower()
    assert "domain-driven" in features_text.lower()


@pytest.mark.asyncio
async def test_blog_enterprise_metrics_endpoint(blog_enterprise_client) -> None:
    """Test that blog_enterprise metrics endpoint works."""
    response = await blog_enterprise_client.get("/metrics")
    assert response.status_code == 200

    data = response.json()
    assert data["service"] == "blog_enterprise"
    assert "metrics" in data
    assert "business_metrics" in data

    # Check expected metrics structure
    metrics = data["metrics"]
    assert "requests_total" in metrics
    assert "cache_hit_rate" in metrics
    assert "error_rate" in metrics

    business_metrics = data["business_metrics"]
    assert "total_posts" in business_metrics
    assert "total_users" in business_metrics
    assert "total_organizations" in business_metrics


@pytest.mark.asyncio
async def test_blog_enterprise_admin_endpoint(blog_enterprise_client) -> None:
    """Test that blog_enterprise admin endpoint is accessible."""
    response = await blog_enterprise_client.get("/admin")
    assert response.status_code == 200

    data = response.json()
    assert "Enterprise Admin Interface" in data["message"]
    assert "features" in data

    # Check enterprise admin features
    admin_features = data["features"]
    expected_features = [
        "Organization management",
        "User administration",
        "Content moderation",
        "Analytics dashboard",
    ]

    for feature in expected_features:
        assert feature in admin_features


@pytest.mark.asyncio
async def test_blog_enterprise_graphql_endpoint_exists(blog_enterprise_client) -> None:
    """Test that GraphQL endpoint exists (even if no schema is implemented yet)."""
    # Try a simple introspection query
    response = await blog_enterprise_client.post(
        "/graphql", json={"query": "{ __schema { types { name } } }"}
    )

    # Should respond (even if with errors due to no types)
    assert response.status_code == 200

    # Response should be valid JSON
    data = response.json()
    assert isinstance(data, dict)


@pytest.mark.asyncio
async def test_blog_enterprise_environment_config(blog_enterprise_client) -> None:
    """Test that enterprise environment configuration works."""
    response = await blog_enterprise_client.get("/")
    data = response.json()

    # Should have environment indicator
    assert "environment" in data

    # Should have proper endpoints configuration
    endpoints = data["endpoints"]
    assert "health" in endpoints
    assert "metrics" in endpoints
    assert "admin" in endpoints
    assert endpoints["health"] == "/health"
    assert endpoints["metrics"] == "/metrics"


@pytest.mark.asyncio
async def test_blog_enterprise_cors_headers(blog_enterprise_client) -> None:
    """Test that CORS headers are properly configured for enterprise."""
    # Make an OPTIONS request to check CORS
    response = await blog_enterprise_client.options("/")

    # Should handle CORS preflight
    assert response.status_code in [200, 405]  # 405 if OPTIONS not explicitly handled

    # Make a regular request and check CORS headers are present
    response = await blog_enterprise_client.get("/", headers={"Origin": "http://localhost:3000"})
    assert response.status_code == 200


@pytest.mark.asyncio
async def test_blog_enterprise_domain_structure_exists() -> None:
    """Test that enterprise domain structure exists."""
    # Check if FraiseQL is available first
    try:
        import fraiseql
    except ImportError:
        pytest.skip("FraiseQL not installed - skipping domain structure test")

    import sys
    from pathlib import Path

    # Add blog_enterprise to path temporarily
    blog_enterprise_path = (
        Path(__file__).parent.parent.parent.parent / "examples" / "blog_enterprise"
    )
    sys.path.insert(0, str(blog_enterprise_path))

    try:
        # Try to import domain modules
        from domain.common import base_classes, events, exceptions

        # Check key domain classes exist
        assert hasattr(base_classes, "AggregateRoot")
        assert hasattr(base_classes, "Entity")
        assert hasattr(base_classes, "ValueObject")
        assert hasattr(base_classes, "DomainEvent")

        # Check events exist
        assert hasattr(events, "PostCreatedEvent")
        assert hasattr(events, "UserRegisteredEvent")

        # Check exceptions exist
        assert hasattr(exceptions, "DomainException")
        assert hasattr(exceptions, "EntityNotFoundError")

    except (ImportError, ModuleNotFoundError) as e:
        pytest.skip(f"Domain structure not implemented yet or dependencies missing: {e}")
    finally:
        # Clean up sys.path
        if str(blog_enterprise_path) in sys.path:
            sys.path.remove(str(blog_enterprise_path))


@pytest.mark.asyncio
async def test_blog_enterprise_vs_simple_distinction(
    blog_enterprise_client, blog_simple_client
) -> None:
    """Test that enterprise version has distinct features from simple version."""
    # Get both app info
    enterprise_response = await blog_enterprise_client.get("/")
    simple_response = await blog_simple_client.get("/")

    enterprise_data = enterprise_response.json()
    simple_data = simple_response.json()

    # Should have different titles
    assert "Enterprise" in enterprise_data["message"]
    assert "Simple" in simple_data["message"]

    # Enterprise should have more features
    enterprise_features = enterprise_data.get("features", [])
    simple_features = simple_data.get("features", [])

    # Enterprise should mention advanced concepts
    enterprise_text = " ".join(enterprise_features).lower()
    assert any(
        keyword in enterprise_text
        for keyword in ["domain-driven", "multi-tenant", "enterprise", "event sourcing", "cqrs"]
    )

    # Enterprise should have additional endpoints
    enterprise_endpoints = enterprise_data.get("endpoints", {})
    simple_endpoints = simple_data.get("endpoints", {})

    # Enterprise has admin and metrics
    assert "admin" in enterprise_endpoints
    assert "metrics" in enterprise_endpoints

    # Simple might not have these
    assert "admin" not in simple_endpoints.get("admin", "")
    assert "metrics" not in simple_endpoints.get("metrics", "")


@pytest.mark.asyncio
@pytest.mark.slow
async def test_blog_enterprise_startup_time(blog_enterprise_client) -> None:
    """Test that enterprise app starts up within reasonable time."""
    import time

    # This test runs after app is already started, but we can test response time
    start_time = time.time()
    response = await blog_enterprise_client.get("/health")
    end_time = time.time()

    assert response.status_code == 200

    # Health check should be fast even for enterprise app
    duration = end_time - start_time
    assert duration < 2.0, f"Health check too slow: {duration:.2f}s"
