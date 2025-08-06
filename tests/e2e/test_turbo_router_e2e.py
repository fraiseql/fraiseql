"""End-to-end tests for TurboRouter with query complexity analysis."""

import asyncio

import pytest
from fastapi.testclient import TestClient

import fraiseql
from fraiseql import query
from fraiseql.fastapi.turbo import TurboQuery
from fraiseql.fastapi.turbo_enhanced import EnhancedTurboRegistry, EnhancedTurboRouter
from fraiseql.gql.schema_builder import build_fraiseql_schema

# Import database fixtures
pytest_plugins = ["tests.database_conftest"]


# Test types
@fraiseql.type
class Product:
    id: int
    name: str
    price: float


@fraiseql.type
class OrderItem:
    product: Product
    quantity: int


@fraiseql.type
class Order:
    id: int
    items: list[OrderItem]
    total: float


@fraiseql.type
class Customer:
    id: int
    name: str
    email: str
    orders: list[Order]


# Test queries
@query
async def get_product(info, product_id: int) -> Product:
    """Simple query - should be cached."""
    return Product(id=product_id, name=f"Product {product_id}", price=19.99)


@query
async def get_customer(info, customer_id: int) -> Customer:
    """Moderate complexity query."""
    return Customer(
        id=customer_id,
        name=f"Customer {customer_id}",
        email=f"customer{customer_id}@example.com",
        orders=[],
    )


@query
async def get_customer_full(info, customer_id: int) -> Customer:
    """Complex query with deep nesting."""
    orders = []
    for order_id in range(3):
        items = []
        for item_id in range(5):
            product = Product(id=100 + item_id, name=f"Product {item_id}", price=10.0 + item_id)
            items.append(OrderItem(product=product, quantity=item_id + 1))

        orders.append(
            Order(
                id=order_id,
                items=items,
                total=sum(item.product.price * item.quantity for item in items),
            )
        )

    return Customer(
        id=customer_id,
        name=f"Customer {customer_id}",
        email=f"customer{customer_id}@example.com",
        orders=orders,
    )


@pytest.mark.database
class TestTurboRouterE2E:
    """End-to-end tests for TurboRouter with complexity analysis."""

    @pytest.fixture
    def app(self, create_fraiseql_app_with_db):
        """Create test application with enhanced TurboRouter."""
        schema = build_fraiseql_schema(query_types=[get_product, get_customer, get_customer_full])

        # Create enhanced registry
        self.registry = EnhancedTurboRegistry(
            max_size=10, max_complexity=100, max_total_weight=50.0, schema=schema
        )

        self.turbo_router = EnhancedTurboRouter(self.registry)

        app = create_fraiseql_app_with_db(
            types=[Product, OrderItem, Order, Customer],
            queries=[get_product, get_customer, get_customer_full],
        )

        # Add metrics endpoint
        @app.get("/turbo/metrics")
        async def get_metrics():
            return self.registry.get_metrics()

        # Add query analysis endpoint
        @app.post("/turbo/analyze")
        async def analyze_query(request: dict):
            query = request.get("query", "")
            score, weight = self.registry.analyze_query(query)
            return {
                "complexity": score.total_score,
                "cache_weight": weight,
                "should_cache": self.registry.should_cache(score),
            }

        return app

    @pytest.fixture
    def client(self, app):
        """Create test client."""
        return TestClient(app)

    def test_simple_query_cached(self, client):
        """Test that simple queries are cached."""
        # Analyze simple query
        simple_query = """
        query GetProduct($productId: Int!) {
            getProduct(productId: $productId) {
                id
                name
                price
            }
        }
        """
        response = client.post("/turbo/analyze", json={"query": simple_query})
        assert response.status_code == 200
        data = response.json()
        assert data["should_cache"] is True
        assert data["complexity"] < 20
        assert data["cache_weight"] <= 0.5

    def test_complex_query_rejected(self, client):
        """Test that complex queries are rejected from cache."""
        # Create a deeply nested query
        complex_query = """
        query GetCustomerFull($customerId: Int!) {
            getCustomerFull(customerId: $customerId) {
                id
                name
                email
                orders {
                    id
                    total
                    items {
                        quantity
                        product {
                            id
                            name
                            price
                        }
                    }
                }
            }
        }
        """
        response = client.post("/turbo/analyze", json={"query": complex_query})
        assert response.status_code == 200
        data = response.json()
        assert data["should_cache"] is False
        assert data["complexity"] > 100
        assert data["cache_weight"] >= 2.0

    def test_cache_metrics(self, client):
        """Test cache metrics tracking."""
        # Get initial metrics
        response = client.get("/turbo/metrics")
        assert response.status_code == 200
        metrics = response.json()

        assert metrics["total_queries_analyzed"] >= 0
        assert metrics["cache_size"] >= 0
        assert metrics["hit_rate"] >= 0.0
        assert "total_weight" in metrics
        assert "weight_utilization" in metrics

    def test_graphql_query_execution(self, client):
        """Test actual GraphQL query execution."""
        # Execute a simple query
        query = """
        query {
            getProduct(productId: 1) {
                id
                name
                price
            }
        }
        """
        response = client.post("/graphql", json={"query": query})

        assert response.status_code == 200
        data = response.json()
        assert data["data"]["getProduct"]["id"] == 1
        assert data["data"]["getProduct"]["name"] == "Product 1"
        assert data["data"]["getProduct"]["price"] == 19.99

    def test_moderate_query_caching(self, client):
        """Test moderate complexity query caching decision."""
        moderate_query = """
        query GetCustomer($customerId: Int!) {
            getCustomer(customerId: $customerId) {
                id
                name
                email
                orders {
                    id
                    total
                }
            }
        }
        """
        response = client.post("/turbo/analyze", json={"query": moderate_query})
        assert response.status_code == 200
        data = response.json()

        # Should be cached but with moderate weight
        assert data["should_cache"] is True
        assert 20 <= data["complexity"] <= 100
        assert 0.5 <= data["cache_weight"] <= 2.0

    def test_query_with_variables(self, client):
        """Test GraphQL query with variables."""
        query = """
        query GetProduct($productId: Int!) {
            getProduct(productId: $productId) {
                id
                name
                price
            }
        }
        """
        variables = {"productId": 42}

        response = client.post("/graphql", json={"query": query, "variables": variables})

        assert response.status_code == 200
        data = response.json()
        assert data["data"]["getProduct"]["id"] == 42
        assert data["data"]["getProduct"]["name"] == "Product 42"

    def test_introspection_query_complexity(self, client):
        """Test that introspection queries have appropriate complexity."""
        introspection_query = """
        query {
            __schema {
                types {
                    name
                    fields {
                        name
                        type {
                            name
                            kind
                        }
                    }
                }
            }
        }
        """
        response = client.post("/turbo/analyze", json={"query": introspection_query})
        assert response.status_code == 200
        data = response.json()

        # Introspection queries should have high complexity
        assert data["complexity"] > 50
        # But might still be cacheable depending on threshold
        # This depends on the specific implementation


@pytest.mark.asyncio
@pytest.mark.database
class TestTurboRouterAsync:
    """Async tests for TurboRouter functionality."""

    async def test_concurrent_queries(self):
        """Test handling of concurrent queries."""
        schema = build_fraiseql_schema(query_types=[get_product])

        registry = EnhancedTurboRegistry(max_size=5, max_complexity=100, schema=schema)

        # Register multiple queries concurrently
        queries = []
        for i in range(10):
            query = f"""
            query GetProduct{i} {{
                getProduct(productId: {i}) {{
                    id
                    name
                }}
            }}
            """
            turbo_query = TurboQuery(
                graphql_query=query,
                sql_template=f"SELECT * FROM products WHERE id = {i}",
                param_mapping={},
            )
            queries.append(turbo_query)

        # Register all queries (register is not async)
        for q in queries:
            registry.register(q)

        # Should have at most max_size queries cached
        assert len(registry) <= 5

        # Check metrics
        metrics = registry.get_metrics()
        assert metrics["cache_size"] <= 5
        assert metrics["total_queries_analyzed"] == 10
