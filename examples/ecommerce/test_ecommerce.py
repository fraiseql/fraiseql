"""Tests for e-commerce example."""

from uuid import uuid4

import pytest

from fraiseql.testing import FraiseQLTestClient

from .app import app


@pytest.fixture
def client():
    """Create test client."""
    return FraiseQLTestClient(app)


@pytest.fixture
def authenticated_client(client):
    """Create authenticated test client."""
    # Register a test user
    response = client.execute(
        """
        mutation {
            register(input: {
                email: "test@example.com",
                password: "password123",
                name: "Test User"
            }) {
                ... on AuthSuccess {
                    token
                    user {
                        id
                    }
                }
            }
        }
        """,
    )

    token = response["data"]["register"]["token"]
    user_id = response["data"]["register"]["user"]["id"]

    # Set authentication header
    client.set_auth_token(token)
    client.user_id = user_id

    return client


class TestProductQueries:
    """Test product-related queries."""

    def test_get_featured_products(self, client):
        """Test fetching featured products."""
        query = """
        query {
            featuredProducts(limit: 4) {
                id
                name
                price
                category
            }
        }
        """

        response = client.execute(query)
        assert "errors" not in response
        assert "featuredProducts" in response["data"]
        products = response["data"]["featuredProducts"]
        assert isinstance(products, list)

    def test_search_products_with_filters(self, client):
        """Test product search with filters."""
        query = """
        query SearchProducts($filters: ProductFilterInput!) {
            products(filters: $filters) {
                items {
                    id
                    name
                    price
                    category
                }
                totalCount
                hasNextPage
            }
        }
        """

        variables = {
            "filters": {
                "category": "ELECTRONICS",
                "minPrice": "100",
                "maxPrice": "1000",
                "inStock": True,
            },
        }

        response = client.execute(query, variables)
        assert "errors" not in response
        assert "products" in response["data"]
        result = response["data"]["products"]
        assert "items" in result
        assert "totalCount" in result
        assert "hasNextPage" in result

    def test_get_product_with_reviews(self, client):
        """Test fetching product with reviews."""
        # First create a product (in real test, this would be seeded)
        product_id = str(uuid4())

        query = """
        query GetProduct($id: UUID!) {
            productWithReviews(id: $id) {
                product {
                    id
                    name
                    description
                    price
                }
                averageRating
                reviewCount
                reviews(limit: 5) {
                    items {
                        rating
                        title
                        comment
                    }
                    totalCount
                }
            }
        }
        """

        variables = {"id": product_id}

        response = client.execute(query, variables)
        assert "errors" not in response
        # Product might not exist in test, but query should work
        assert "productWithReviews" in response["data"]


class TestAuthMutations:
    """Test authentication mutations."""

    def test_user_registration(self, client):
        """Test user registration."""
        mutation = """
        mutation Register($input: RegisterInput!) {
            register(input: $input) {
                ... on AuthSuccess {
                    user {
                        id
                        email
                        name
                    }
                    token
                    message
                }
                ... on AuthError {
                    message
                    code
                }
            }
        }
        """

        variables = {
            "input": {
                "email": f"user_{uuid4()}@example.com",
                "password": "SecurePass123!",
                "name": "New User",
                "phone": "+1234567890",
            },
        }

        response = client.execute(mutation, variables)
        assert "errors" not in response
        assert response["data"]["register"]["__typename"] == "AuthSuccess"
        assert "token" in response["data"]["register"]
        assert response["data"]["register"]["user"]["email"] == variables["input"]["email"]

    def test_duplicate_email_registration(self, client):
        """Test registration with duplicate email."""
        email = f"duplicate_{uuid4()}@example.com"

        # First registration
        mutation = """
        mutation Register($input: RegisterInput!) {
            register(input: $input) {
                ... on AuthSuccess {
                    user { id }
                }
                ... on AuthError {
                    message
                    code
                }
            }
        }
        """

        variables = {
            "input": {
                "email": email,
                "password": "password123",
                "name": "User",
            },
        }

        # First should succeed
        response1 = client.execute(mutation, variables)
        assert response1["data"]["register"]["__typename"] == "AuthSuccess"

        # Second should fail
        response2 = client.execute(mutation, variables)
        assert response2["data"]["register"]["__typename"] == "AuthError"
        assert response2["data"]["register"]["code"] == "EMAIL_EXISTS"

    def test_user_login(self, client):
        """Test user login."""
        # First register
        email = f"login_{uuid4()}@example.com"
        password = "password123"

        register_mutation = f"""
        mutation {{
            register(input: {{email: "{email}", password: "{password}", name: "Test"}}) {{
                ... on AuthSuccess {{ user {{ id }} }}
            }}
        }}
        """

        client.execute(register_mutation)

        # Then login
        login_mutation = """
        mutation Login($input: LoginInput!) {
            login(input: $input) {
                ... on AuthSuccess {
                    user {
                        email
                        name
                    }
                    token
                }
                ... on AuthError {
                    message
                    code
                }
            }
        }
        """

        variables = {
            "input": {
                "email": email,
                "password": password,
            },
        }

        response = client.execute(login_mutation, variables)
        assert "errors" not in response
        assert response["data"]["login"]["__typename"] == "AuthSuccess"
        assert "token" in response["data"]["login"]
        assert response["data"]["login"]["user"]["email"] == email


class TestCartOperations:
    """Test cart operations."""

    def test_add_to_cart(self, authenticated_client):
        """Test adding item to cart."""
        # In real test, we'd have a seeded product
        product_id = str(uuid4())

        mutation = """
        mutation AddToCart($input: AddToCartInput!) {
            addToCart(input: $input) {
                ... on CartSuccess {
                    cart {
                        id
                        itemsCount
                        subtotal
                    }
                    message
                }
                ... on CartError {
                    message
                    code
                }
            }
        }
        """

        variables = {
            "input": {
                "productId": product_id,
                "quantity": 2,
            },
        }

        response = authenticated_client.execute(mutation, variables)
        assert "errors" not in response
        # Would check success if product existed

    def test_update_cart_item(self, authenticated_client):
        """Test updating cart item quantity."""
        cart_item_id = str(uuid4())

        mutation = """
        mutation UpdateCart($input: UpdateCartItemInput!) {
            updateCartItem(input: $input) {
                ... on CartSuccess {
                    cart {
                        itemsCount
                        subtotal
                    }
                }
                ... on CartError {
                    message
                    code
                }
            }
        }
        """

        variables = {
            "input": {
                "cartItemId": cart_item_id,
                "quantity": 3,
            },
        }

        response = authenticated_client.execute(mutation, variables)
        assert "errors" not in response

    def test_get_cart(self, authenticated_client):
        """Test fetching user's cart."""
        query = """
        query {
            myCart {
                cart {
                    id
                    itemsCount
                    subtotal
                }
                items {
                    id
                    quantity
                    price
                }
                recommendedProducts {
                    id
                    name
                }
            }
        }
        """

        response = authenticated_client.execute(query)
        assert "errors" not in response
        assert "myCart" in response["data"]


class TestOrderOperations:
    """Test order operations."""

    def test_checkout_flow(self, authenticated_client):
        """Test complete checkout flow."""
        # Would need seeded data for full test
        address_id = str(uuid4())

        mutation = """
        mutation Checkout($input: CheckoutInput!) {
            checkout(input: $input) {
                ... on OrderSuccess {
                    order {
                        id
                        orderNumber
                        status
                        total
                    }
                    message
                }
                ... on OrderError {
                    message
                    code
                }
            }
        }
        """

        variables = {
            "input": {
                "shippingAddressId": address_id,
                "billingAddressId": address_id,
                "notes": "Please leave at door",
            },
        }

        response = authenticated_client.execute(mutation, variables)
        assert "errors" not in response

    def test_get_user_orders(self, authenticated_client):
        """Test fetching user's orders."""
        query = """
        query {
            myOrders(limit: 10) {
                items {
                    id
                    orderNumber
                    status
                    total
                    placedAt
                }
                totalCount
                hasNextPage
            }
        }
        """

        response = authenticated_client.execute(query)
        assert "errors" not in response
        assert "myOrders" in response["data"]
        assert "items" in response["data"]["myOrders"]
        assert "totalCount" in response["data"]["myOrders"]


class TestReviewOperations:
    """Test review operations."""

    def test_create_review(self, authenticated_client):
        """Test creating a product review."""
        product_id = str(uuid4())

        mutation = """
        mutation CreateReview($input: CreateReviewInput!) {
            createReview(input: $input) {
                ... on ReviewSuccess {
                    review {
                        id
                        rating
                        title
                        isVerified
                    }
                    message
                }
                ... on ReviewError {
                    message
                    code
                }
            }
        }
        """

        variables = {
            "input": {
                "productId": product_id,
                "rating": 5,
                "title": "Excellent product!",
                "comment": "Really happy with this purchase. Great quality.",
            },
        }

        response = authenticated_client.execute(mutation, variables)
        assert "errors" not in response


class TestAddressOperations:
    """Test address operations."""

    def test_create_address(self, authenticated_client):
        """Test creating an address."""
        mutation = """
        mutation CreateAddress($input: CreateAddressInput!) {
            createAddress(input: $input) {
                ... on AddressSuccess {
                    address {
                        id
                        label
                        street1
                        city
                        isDefault
                    }
                    message
                }
                ... on AddressError {
                    message
                    code
                }
            }
        }
        """

        variables = {
            "input": {
                "label": "Home",
                "street1": "123 Main St",
                "street2": "Apt 4B",
                "city": "New York",
                "state": "NY",
                "postalCode": "10001",
                "country": "US",
                "isDefault": True,
            },
        }

        response = authenticated_client.execute(mutation, variables)
        assert "errors" not in response
        assert response["data"]["createAddress"]["__typename"] == "AddressSuccess"
        assert response["data"]["createAddress"]["address"]["label"] == "Home"
        assert response["data"]["createAddress"]["address"]["isDefault"] is True

    def test_get_user_addresses(self, authenticated_client):
        """Test fetching user's addresses."""
        query = """
        query {
            myAddresses {
                id
                label
                street1
                city
                state
                isDefault
            }
        }
        """

        response = authenticated_client.execute(query)
        assert "errors" not in response
        assert "myAddresses" in response["data"]
        assert isinstance(response["data"]["myAddresses"], list)


class TestDashboard:
    """Test dashboard queries."""

    def test_user_dashboard_stats(self, authenticated_client):
        """Test fetching user dashboard statistics."""
        query = """
        query {
            myDashboard {
                totalOrders
                totalSpent
                averageOrderValue
                wishlistCount
                reviewCount
                pointsBalance
            }
        }
        """

        response = authenticated_client.execute(query)
        assert "errors" not in response
        assert "myDashboard" in response["data"]
        stats = response["data"]["myDashboard"]
        assert "totalOrders" in stats
        assert "totalSpent" in stats
        assert "averageOrderValue" in stats


class TestGraphQLSchema:
    """Test GraphQL schema generation."""

    def test_introspection_query(self, client):
        """Test schema introspection."""
        query = """
        query {
            __schema {
                types {
                    name
                    kind
                }
            }
        }
        """

        response = client.execute(query)
        assert "errors" not in response
        assert "__schema" in response["data"]

        type_names = [t["name"] for t in response["data"]["__schema"]["types"]]

        # Check core types exist
        assert "User" in type_names
        assert "Product" in type_names
        assert "Order" in type_names
        assert "Cart" in type_names
        assert "Review" in type_names

        # Check enums
        assert "OrderStatus" in type_names
        assert "PaymentStatus" in type_names
        assert "ProductCategory" in type_names

        # Check mutations
        assert "Mutation" in type_names

        # Check query type
        assert "Query" in type_names
