"""FraiseQL Product Catalog Schema
Demonstrates LTREE usage for product categorization
"""

import fraiseql
from fraiseql.types.scalars import LTree


@fraiseql.fraise_type
class Product:
    """Product in the catalog with hierarchical categorization."""

    id: int = fraiseql.field(primary_key=True)
    name: str
    description: str
    price: float
    category_path: LTree  # Hierarchical category path
    sku: str
    in_stock: bool = True
    created_at: str
    updated_at: str

    # Computed fields
    @fraiseql.field
    def category_depth(self) -> int:
        """Get the depth of this product's category."""
        return fraiseql.sql.Function("nlevel", self.category_path)

    @fraiseql.field
    def parent_category(self) -> str | None:
        """Get the parent category path."""
        return fraiseql.sql.Function("subpath", self.category_path, 0, -1)

    @fraiseql.field
    def category_name(self) -> str:
        """Get the leaf category name."""
        return fraiseql.sql.Function("subpath", self.category_path, -1, 1)

    @fraiseql.field
    def breadcrumbs(self) -> list[str]:
        """Get category path as breadcrumb list."""
        # This would need custom SQL function or application logic
        # For demo purposes, we'll return the path components
        return [str(self.category_path)]  # Simplified

    # Relationships
    @fraiseql.field
    def related_products(self) -> list["Product"]:
        """Get products in the same category or subcategories."""
        return (
            Product.find_where(category_path__descendant_of=self.parent_category)
            .exclude(id=self.id)
            .limit(5)
        )

    @fraiseql.field
    def sibling_products(self) -> list["Product"]:
        """Get products at the same category level."""
        return Product.find_where(
            category_path__descendant_of=self.parent_category,
            category_path__nlevel_eq=self.category_depth,
        ).exclude(id=self.id)


@fraiseql.fraise_query
class Query:
    """Root query for product catalog."""

    @fraiseql.field
    def products(self, where: dict | None = None) -> list[Product]:
        """Get all products with optional filtering."""
        return Product.find_where(**(where or {}))

    @fraiseql.field
    def product(self, id: int) -> Product | None:
        """Get a specific product by ID."""
        return Product.find_by(id=id)

    @fraiseql.field
    def products_by_category(self, category_path: str) -> list[Product]:
        """Get all products in a category and its subcategories."""
        return Product.find_where(category_path__descendant_of=category_path)

    @fraiseql.field
    def category_products(
        self, category_path: str, include_subcategories: bool = True
    ) -> list[Product]:
        """Get products in a specific category."""
        if include_subcategories:
            return Product.find_where(category_path__descendant_of=category_path)
        return Product.find_where(
            category_path__descendant_of=category_path,
            category_path__nlevel_eq=fraiseql.sql.Function("nlevel", category_path) + 1,
        )

    @fraiseql.field
    def products_in_price_range(
        self, min_price: float, max_price: float, category: str | None = None
    ) -> list[Product]:
        """Get products in a price range, optionally filtered by category."""
        where_clause = {"price__gte": min_price, "price__lte": max_price}
        if category:
            where_clause["category_path__descendant_of"] = category

        return Product.find_where(**where_clause)

    @fraiseql.field
    def category_tree(self, root_category: str | None = None) -> list[Product]:
        """Get products organized by category hierarchy."""
        root = root_category or "electronics"
        return Product.find_where(category_path__descendant_of=root).order_by("category_path")

    @fraiseql.field
    def search_products(self, query: str, category: str | None = None) -> list[Product]:
        """Search products by name or description."""
        where_clause = {"name__contains": query, "in_stock": True}
        if category:
            where_clause["category_path__descendant_of"] = category

        return Product.find_where(**where_clause)

    @fraiseql.field
    def products_by_depth(self, depth: int) -> list[Product]:
        """Get products at a specific category depth."""
        return Product.find_where(category_path__nlevel_eq=depth)


@fraiseql.fraise_mutation
class Mutation:
    """Mutations for product catalog management."""

    @fraiseql.field
    def add_product(
        self, name: str, description: str, price: float, category_path: str, sku: str
    ) -> Product:
        """Add a new product to the catalog."""
        return Product.create(
            name=name, description=description, price=price, category_path=category_path, sku=sku
        )

    @fraiseql.field
    def update_product_category(self, product_id: int, new_category_path: str) -> Product:
        """Update a product's category."""
        product = Product.find_by(id=product_id)
        if not product:
            raise ValueError("Product not found")

        product.update(category_path=new_category_path)
        return product

    @fraiseql.field
    def update_price(self, product_id: int, new_price: float) -> Product:
        """Update a product's price."""
        product = Product.find_by(id=product_id)
        if not product:
            raise ValueError("Product not found")

        product.update(price=new_price, updated_at=fraiseql.sql.Function("NOW"))
        return product

    @fraiseql.field
    def bulk_update_category(self, old_category_path: str, new_category_path: str) -> list[Product]:
        """Move all products from one category to another."""
        products = Product.find_where(category_path__descendant_of=old_category_path)

        updated_products = []
        for product in products:
            # Replace old category prefix with new one
            old_path_str = str(product.category_path)
            new_path_str = old_path_str.replace(old_category_path, new_category_path, 1)
            product.update(category_path=new_path_str)
            updated_products.append(product)

        return updated_products


# Configure the GraphQL app
app = fraiseql.create_app(
    title="Product Catalog API",
    description="Hierarchical product catalog with LTREE categorization",
    version="1.0.0",
)
