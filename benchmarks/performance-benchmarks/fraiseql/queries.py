"""FraiseQL benchmark queries using automatic WhereType generation."""

from models import Category, Order, PopularProduct, Product, ProductsByCategory, User, UserStats

from fraiseql import fraise_field as field
from fraiseql import fraise_type


@fraise_type
class Query:
    """Root query type for the benchmark API.

    FraiseQL automatically generates WhereType filters for each field,
    so we don't need individual resolvers for different query patterns.
    """

    # Health check field
    ping: str = field(default="pong", description="Health check endpoint")

    # User queries with automatic filtering
    users: list[User] = field(
        default_factory=list, description="Query users with automatic filters"
    )

    # Product queries with automatic filtering
    products: list[Product] = field(
        default_factory=list, description="Query products with automatic filters"
    )

    # Order queries with automatic filtering
    orders: list[Order] = field(
        default_factory=list, description="Query orders with automatic filters"
    )

    # Category queries
    categories: list[Category] = field(default_factory=list, description="Query categories")

    # Table views for performance
    popular_products: list[PopularProduct] = field(
        default_factory=list, description="Popular products from table view"
    )

    products_by_category: list[ProductsByCategory] = field(
        default_factory=list, description="Products grouped by category from table view"
    )

    user_stats: list[UserStats] = field(
        default_factory=list, description="User statistics from table view"
    )


# Example GraphQL queries that work automatically:
"""
# Get user by ID
query {
    users(where: {id: {eq: "123e4567-e89b-12d3-a456-426614174000"}}) {
        id
        username
        email
        fullName
        orderCount
        totalSpent
    }
}

# Get products with price filtering
query {
    products(
        where: {
            price: {gte: 50.0, lte: 200.0}
            stockQuantity: {gt: 0}
        }
        orderBy: {price: ASC}
        limit: 20
    ) {
        id
        name
        price
        stockQuantity
        averageRating
        categories {
            name
        }
    }
}

# Get orders by status and user
query {
    orders(
        where: {
            status: {eq: "completed"}
            userId: {eq: "123e4567-e89b-12d3-a456-426614174000"}
        }
        orderBy: {createdAt: DESC}
        limit: 10
    ) {
        id
        totalAmount
        status
        createdAt
        items {
            productName
            quantity
            unitPrice
        }
    }
}

# Complex query with nested filtering
query {
    products(
        where: {
            price: {lt: 100}
            categories: {name: {in: ["Electronics", "Books"]}}
            stockQuantity: {gt: 0}
            averageRating: {gte: 4.0}
        }
        orderBy: {averageRating: DESC}
        limit: 50
    ) {
        id
        name
        price
        averageRating
        reviewCount
        categories {
            name
        }
        reviews {
            rating
            title
            user {
                username
            }
        }
    }
}

# Get popular products
query {
    popularProducts(
        where: {totalRevenue: {gte: 1000}}
        orderBy: {totalRevenue: DESC}
        limit: 10
    ) {
        name
        price
        reviewCount
        averageRating
        totalRevenue
    }
}

# Get user statistics
query {
    userStats(
        where: {orderCount: {gte: 5}}
        orderBy: {totalSpent: DESC}
        limit: 20
    ) {
        username
        orderCount
        totalSpent
        reviewCount
        averageRating
    }
}
"""
