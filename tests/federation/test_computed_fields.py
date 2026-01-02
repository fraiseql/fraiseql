"""Tests for computed fields with @requires and @provides directives."""


from fraiseql.federation.computed_fields import (
    ComputedField,
    ComputedFieldValidator,
    extract_computed_fields,
    get_all_field_dependencies,
    validate_all_computed_fields,
)
from fraiseql.federation.decorators import extend_entity, external
from fraiseql.federation.directives import requires, provides


class TestComputedField:
    """Tests for ComputedField metadata class."""

    def test_create_computed_field_requires(self):
        """Test creating computed field with requirements."""
        field = ComputedField(
            "discounted_price",
            requires=["price"],
            is_async=False,
        )
        assert field.method_name == "discounted_price"
        assert field.requires == ["price"]
        assert field.has_requirements() is True
        assert field.has_provisions() is False

    def test_create_computed_field_provides(self):
        """Test creating computed field with provisions."""
        field = ComputedField(
            "summary",
            provides=["id", "title"],
            is_async=True,
        )
        assert field.method_name == "summary"
        assert field.provides == ["id", "title"]
        assert field.has_provisions() is True
        assert field.has_requirements() is False
        assert field.is_async is True

    def test_computed_field_both_directives(self):
        """Test computed field with both requires and provides."""
        field = ComputedField(
            "transform",
            requires=["input"],
            provides=["output"],
            is_async=False,
        )
        assert field.has_requirements() is True
        assert field.has_provisions() is True
        assert field.get_required_fields() == ["input"]
        assert field.get_provided_fields() == ["output"]

    def test_computed_field_no_directives(self):
        """Test computed field with no directives."""
        field = ComputedField("plain_method")
        assert field.has_requirements() is False
        assert field.has_provisions() is False

    def test_computed_field_repr(self):
        """Test repr of computed field."""
        field = ComputedField(
            "method",
            requires=["a", "b"],
            provides=["c"],
            is_async=True,
        )
        repr_str = repr(field)
        assert "method" in repr_str
        assert "requires=" in repr_str
        assert "provides=" in repr_str
        assert "async=True" in repr_str


class TestComputedFieldValidator:
    """Tests for ComputedFieldValidator class."""

    def test_validator_init(self):
        """Test validator initialization."""
        validator = ComputedFieldValidator()
        assert validator.get_errors() == []

    def test_validate_requires_all_exist(self):
        """Test validation when all required fields exist."""
        validator = ComputedFieldValidator()
        all_fields = {"id", "price", "name"}
        required = ["price", "name"]

        valid = validator.validate_requires("method", required, all_fields)
        assert valid is True
        assert validator.get_errors() == []

    def test_validate_requires_missing_fields(self):
        """Test validation with missing required fields."""
        validator = ComputedFieldValidator()
        all_fields = {"id", "price"}
        required = ["price", "discount", "tax"]

        valid = validator.validate_requires("method", required, all_fields)
        assert valid is False
        errors = validator.get_errors()
        assert len(errors) > 0
        assert "discount" in errors[0]
        assert "tax" in errors[0]

    def test_validate_provides_valid(self):
        """Test validation of provisions."""
        validator = ComputedFieldValidator()
        all_fields = {"id", "price", "name"}
        provided = ["summary"]

        valid = validator.validate_provides("method", provided, all_fields)
        assert valid is True

    def test_validate_provides_empty(self):
        """Test validation with empty provisions."""
        validator = ComputedFieldValidator()
        all_fields = {"id", "price"}
        provided = []

        valid = validator.validate_provides("method", provided, all_fields)
        assert valid is False
        assert len(validator.get_errors()) > 0

    def test_validate_computed_field_complete(self):
        """Test complete validation of a computed field."""
        validator = ComputedFieldValidator()
        field = ComputedField(
            "method",
            requires=["price"],
            provides=["discount"],
        )
        all_fields = {"id", "price", "name"}

        valid = validator.validate_computed_field(field, all_fields)
        assert valid is True
        assert validator.get_errors() == []

    def test_clear_errors(self):
        """Test clearing error list."""
        validator = ComputedFieldValidator()
        validator.errors.append("error 1")
        validator.errors.append("error 2")

        validator.clear_errors()
        assert validator.get_errors() == []


class TestExtractComputedFields:
    """Tests for extract_computed_fields helper function."""

    def test_extract_no_computed_fields(self):
        """Test extracting from class with no computed fields."""

        class User:
            id: str
            name: str

        fields = extract_computed_fields(User)
        assert fields == {}

    def test_extract_with_requires(self):
        """Test extracting method with @requires."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            price: float = external()

            @requires("price")
            def discounted_price(self) -> float:
                return self.price * 0.9

        fields = extract_computed_fields(Product)
        assert "discounted_price" in fields
        assert fields["discounted_price"].requires == ["price"]
        assert fields["discounted_price"].has_requirements() is True

    def test_extract_with_provides(self):
        """Test extracting method with @provides."""

        @extend_entity(key="id")
        class Article:
            id: str = external()
            title: str = external()

            @provides("id title")
            async def summary(self):
                return f"{self.id}: {self.title}"

        fields = extract_computed_fields(Article)
        assert "summary" in fields
        assert set(fields["summary"].provides) == {"id", "title"}
        assert fields["summary"].is_async is True

    def test_extract_multiple_computed_fields(self):
        """Test extracting multiple computed fields."""

        @extend_entity(key="id")
        class Post:
            id: str = external()
            content: str = external()
            comments: list = None

            @requires("content")
            def word_count(self) -> int:
                return len(self.content.split())

            @requires("comments")
            def comment_count(self) -> int:
                return len(self.comments) if self.comments else 0

            @provides("id")
            async def metadata(self):
                return {"id": self.id}

        fields = extract_computed_fields(Post)
        assert len(fields) == 3
        assert "word_count" in fields
        assert "comment_count" in fields
        assert "metadata" in fields

    def test_extract_with_both_directives(self):
        """Test method with both @requires and @provides."""

        @extend_entity(key="id")
        class Document:
            id: str = external()
            text: str = external()

            @requires("text")
            @provides("id summary")
            async def analyze(self) -> dict:
                return {"id": self.id, "summary": self.text[:50]}

        fields = extract_computed_fields(Document)
        assert "analyze" in fields
        field = fields["analyze"]
        assert field.requires == ["text"]
        assert set(field.provides) == {"id", "summary"}
        assert field.is_async is True


class TestFieldDependencies:
    """Tests for field dependency analysis."""

    def test_simple_dependency(self):
        """Test simple field dependency."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            price: float = external()

            @requires("price")
            def discounted(self) -> float:
                return self.price * 0.9

        deps = get_all_field_dependencies(Product)
        assert "discounted" in deps
        assert deps["discounted"] == {"price"}

    def test_multiple_dependencies(self):
        """Test field with multiple dependencies."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            price: float = external()
            discount: float = external()

            @requires("price discount")
            def final_price(self) -> float:
                return self.price * (1 - self.discount)

        deps = get_all_field_dependencies(Product)
        assert deps["final_price"] == {"price", "discount"}

    def test_multiple_computed_fields_dependencies(self):
        """Test dependencies of multiple computed fields."""

        @extend_entity(key="id")
        class Order:
            id: str = external()
            subtotal: float = external()
            tax_rate: float = external()
            discount: float = external()

            @requires("subtotal tax_rate")
            def total_with_tax(self) -> float:
                return self.subtotal + (self.subtotal * self.tax_rate)

            @requires("subtotal discount")
            def discounted_subtotal(self) -> float:
                return self.subtotal * (1 - self.discount)

        deps = get_all_field_dependencies(Order)
        assert deps["total_with_tax"] == {"subtotal", "tax_rate"}
        assert deps["discounted_subtotal"] == {"subtotal", "discount"}


class TestValidateComputedFields:
    """Tests for validate_all_computed_fields function."""

    def test_validate_all_valid(self):
        """Test validation when all computed fields are valid."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            price: float = external()

            @requires("price")
            def discounted(self) -> float:
                return self.price * 0.9

        all_fields = {"id", "price", "discounted"}
        valid, errors = validate_all_computed_fields(Product, all_fields)
        assert valid is True
        assert errors == []

    def test_validate_missing_requirement(self):
        """Test validation with missing required field."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            price: float = external()

            @requires("price discount")  # discount doesn't exist
            def final_price(self) -> float:
                return self.price * 0.9

        all_fields = {"id", "price", "final_price"}
        valid, errors = validate_all_computed_fields(Product, all_fields)
        assert valid is False
        assert len(errors) > 0
        assert "discount" in errors[0]

    def test_validate_multiple_errors(self):
        """Test validation with multiple errors."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            price: float = external()

            @requires("price tax")  # tax missing
            def with_tax(self) -> float:
                return self.price

            @requires("price currency")  # currency missing
            def formatted(self) -> str:
                return f"${self.price}"

        all_fields = {"id", "price", "with_tax", "formatted"}
        valid, errors = validate_all_computed_fields(Product, all_fields)
        assert valid is False
        assert len(errors) >= 2


class TestComputedFieldsIntegration:
    """Integration tests for computed fields with external fields."""

    def test_product_review_computed_fields(self):
        """Test computed fields in product review extension."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            name: str = external()
            price: float = external()
            reviews: list = None

            @requires("price")
            def discount_10_percent(self) -> float:
                return self.price * 0.9

            @requires("reviews")
            def average_rating(self) -> float:
                if not self.reviews:
                    return 0
                return sum(r.get("rating", 0) for r in self.reviews) / len(self.reviews)

            @requires("name price")
            @provides("id name price")
            async def product_summary(self) -> dict:
                return {
                    "id": self.id,
                    "name": self.name,
                    "price": self.price,
                }

        fields = extract_computed_fields(Product)
        assert len(fields) == 3
        assert "discount_10_percent" in fields
        assert "average_rating" in fields
        assert "product_summary" in fields

    def test_user_posts_computed_fields(self):
        """Test computed fields in user posts extension."""

        @extend_entity(key="id")
        class User:
            id: str = external()
            username: str = external()
            posts: list = None

            @requires("posts")
            def post_count(self) -> int:
                return len(self.posts) if self.posts else 0

            @requires("posts")
            def most_recent_post(self):
                if not self.posts:
                    return None
                return self.posts[0]

        deps = get_all_field_dependencies(User)
        assert deps["post_count"] == {"posts"}
        assert deps["most_recent_post"] == {"posts"}

    def test_validate_real_world_scenario(self):
        """Test validation with real-world scenario."""

        @extend_entity(key="id")
        class Article:
            id: str = external()
            title: str = external()
            content: str = external()
            comments: list = None

            @requires("content")
            def word_count(self) -> int:
                return len(self.content.split())

            @requires("comments")
            def comment_count(self) -> int:
                return len(self.comments) if self.comments else 0

            @provides("id title")
            async def headline(self) -> str:
                return f"{self.id}: {self.title}"

        all_fields = {
            "id",
            "title",
            "content",
            "comments",
            "word_count",
            "comment_count",
            "headline",
        }
        valid, errors = validate_all_computed_fields(Article, all_fields)
        assert valid is True
        assert errors == []
