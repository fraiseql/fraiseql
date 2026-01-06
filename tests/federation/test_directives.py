"""Tests for Federation Standard directives (@requires, @provides)."""

from fraiseql.federation.directives import (
    DirectiveMetadata,
    get_directives,
    get_method_directives,
    provides,
    requires,
)


class TestRequiresMarker:
    """Tests for @requires directive marker."""

    def test_requires_with_space_separated_fields(self) -> None:
        """Test parsing space-separated field list."""

        @requires("price currency")
        def formatted_price(self):
            pass

        metadata = get_directives(formatted_price)
        assert metadata.has_requires()
        assert metadata.get_required_fields() == ["price", "currency"]

    def test_requires_with_comma_separated_fields(self) -> None:
        """Test parsing comma-separated field list."""

        @requires("latitude, longitude")
        def distance(self):
            pass

        metadata = get_directives(distance)
        assert metadata.has_requires()
        assert metadata.get_required_fields() == ["latitude", "longitude"]

    def test_requires_with_list_fields(self) -> None:
        """Test passing fields as list."""

        @requires(["field1", "field2", "field3"])
        def compute(self):
            pass

        metadata = get_directives(compute)
        assert metadata.has_requires()
        assert metadata.get_required_fields() == ["field1", "field2", "field3"]

    def test_requires_single_field(self) -> None:
        """Test with single field."""

        @requires("id")
        def get_id(self):
            pass

        metadata = get_directives(get_id)
        assert metadata.has_requires()
        assert metadata.get_required_fields() == ["id"]

    def test_requires_field_set(self) -> None:
        """Test that field_set is properly populated."""

        @requires("a b c")
        def method(self):
            pass

        metadata = get_directives(method)
        assert metadata.requires is not None
        assert metadata.requires.field_set == {"a", "b", "c"}


class TestProvidesMarker:
    """Tests for @provides directive marker."""

    def test_provides_with_space_separated_fields(self) -> None:
        """Test parsing space-separated field list."""

        @provides("id name")
        async def posts(self):
            pass

        metadata = get_directives(posts)
        assert metadata.has_provides()
        assert metadata.get_provided_fields() == ["id", "name"]

    def test_provides_with_comma_separated_fields(self) -> None:
        """Test parsing comma-separated field list."""

        @provides("user_id, created_at")
        async def comments(self):
            pass

        metadata = get_directives(comments)
        assert metadata.has_provides()
        assert metadata.get_provided_fields() == ["user_id", "created_at"]

    def test_provides_with_list_fields(self) -> None:
        """Test passing fields as list."""

        @provides(["id", "title", "author_id"])
        async def articles(self):
            pass

        metadata = get_directives(articles)
        assert metadata.has_provides()
        assert metadata.get_provided_fields() == ["id", "title", "author_id"]

    def test_provides_single_field(self) -> None:
        """Test with single field."""

        @provides("id")
        async def get_id(self):
            pass

        metadata = get_directives(get_id)
        assert metadata.has_provides()
        assert metadata.get_provided_fields() == ["id"]


class TestCombinedDirectives:
    """Tests for combining @requires and @provides on same method."""

    def test_both_requires_and_provides(self) -> None:
        """Test method with both directives."""

        @requires("price")
        @provides("formatted_price")
        def format_price(self):
            pass

        metadata = get_directives(format_price)
        assert metadata.has_requires()
        assert metadata.has_provides()
        assert metadata.get_required_fields() == ["price"]
        assert metadata.get_provided_fields() == ["formatted_price"]

    def test_provides_then_requires(self) -> None:
        """Test decorator order: @provides then @requires."""

        @provides("result")
        @requires("input")
        def transform(self):
            pass

        metadata = get_directives(transform)
        assert metadata.has_requires()
        assert metadata.has_provides()
        assert metadata.get_required_fields() == ["input"]
        assert metadata.get_provided_fields() == ["result"]


class TestGetDirectives:
    """Tests for get_directives helper function."""

    def test_no_directives(self) -> None:
        """Test function without directives."""

        def plain_method(self):
            pass

        metadata = get_directives(plain_method)
        assert not metadata.has_requires()
        assert not metadata.has_provides()
        assert metadata.get_required_fields() == []
        assert metadata.get_provided_fields() == []

    def test_get_directives_repr(self) -> None:
        """Test DirectiveMetadata repr."""

        @requires("a b")
        def method1(self):
            pass

        metadata = get_directives(method1)
        assert "requires" in repr(metadata)
        assert "a" in repr(metadata)
        assert "b" in repr(metadata)


class TestGetMethodDirectives:
    """Tests for get_method_directives class introspection."""

    def test_get_method_directives_from_class(self) -> None:
        """Test extracting all method directives from a class."""

        class Product:
            id: str
            price: float

            @requires("price")
            def formatted_price(self) -> str:
                return f"${self.price}"

            @provides("id")
            async def reviews(self):
                pass

            def plain_method(self):
                pass

        directives = get_method_directives(Product)

        # Should have 2 methods with directives
        assert len(directives) == 2
        assert "formatted_price" in directives
        assert "reviews" in directives
        assert "plain_method" not in directives

        # Check formatted_price directives
        assert directives["formatted_price"].has_requires()
        assert directives["formatted_price"].get_required_fields() == ["price"]

        # Check reviews directives
        assert directives["reviews"].has_provides()
        assert directives["reviews"].get_provided_fields() == ["id"]

    def test_get_method_directives_empty_class(self) -> None:
        """Test class with no directive-marked methods."""

        class User:
            id: str
            name: str

            def get_name(self):
                return self.name

        directives = get_method_directives(User)
        assert len(directives) == 0

    def test_get_method_directives_multiple_methods(self) -> None:
        """Test class with multiple directive-marked methods."""

        class Document:
            id: str
            title: str
            content: str

            @requires("title")
            def slug(self) -> str:
                return self.title.lower().replace(" ", "-")

            @requires("content")
            def word_count(self) -> int:
                return len(self.content.split())

            @requires("title content")
            def summary(self) -> str:
                return f"{self.title}: {len(self.content)} chars"

            @provides("id title")
            async def related_documents(self):
                pass

        directives = get_method_directives(Document)

        assert len(directives) == 4
        assert set(directives.keys()) == {
            "slug",
            "word_count",
            "summary",
            "related_documents",
        }

        # Check each has expected directives
        assert directives["slug"].get_required_fields() == ["title"]
        assert directives["word_count"].get_required_fields() == ["content"]
        assert directives["summary"].get_required_fields() == ["title", "content"]
        assert directives["related_documents"].get_provided_fields() == ["id", "title"]

    def test_get_method_directives_ignores_private_methods(self) -> None:
        """Test that private methods are ignored."""

        class Service:
            @requires("id")
            def public_method(self):
                pass

            @requires("id")
            def _private_method(self):
                pass

            @requires("id")
            def __dunder_method__(self):
                pass

        directives = get_method_directives(Service)

        # Only public_method should be included
        assert len(directives) == 1
        assert "public_method" in directives
        assert "_private_method" not in directives
        assert "__dunder_method__" not in directives


class TestDirectiveMetadata:
    """Tests for DirectiveMetadata class."""

    def test_both_directives(self) -> None:
        """Test metadata with both directives."""
        from fraiseql.federation.directives import _ProvidesMarker, _RequiresMarker

        requires_marker = _RequiresMarker("a b")
        provides_marker = _ProvidesMarker("x y")

        metadata = DirectiveMetadata(requires=requires_marker, provides=provides_marker)

        assert metadata.has_requires()
        assert metadata.has_provides()
        assert metadata.get_required_fields() == ["a", "b"]
        assert metadata.get_provided_fields() == ["x", "y"]

    def test_only_requires(self) -> None:
        """Test metadata with only requires."""
        from fraiseql.federation.directives import _RequiresMarker

        requires_marker = _RequiresMarker("field1 field2")
        metadata = DirectiveMetadata(requires=requires_marker)

        assert metadata.has_requires()
        assert not metadata.has_provides()
        assert metadata.get_required_fields() == ["field1", "field2"]
        assert metadata.get_provided_fields() == []

    def test_only_provides(self) -> None:
        """Test metadata with only provides."""
        from fraiseql.federation.directives import _ProvidesMarker

        provides_marker = _ProvidesMarker("field1 field2")
        metadata = DirectiveMetadata(provides=provides_marker)

        assert not metadata.has_requires()
        assert metadata.has_provides()
        assert metadata.get_required_fields() == []
        assert metadata.get_provided_fields() == ["field1", "field2"]

    def test_no_directives(self) -> None:
        """Test metadata with no directives."""
        metadata = DirectiveMetadata()

        assert not metadata.has_requires()
        assert not metadata.has_provides()
        assert metadata.get_required_fields() == []
        assert metadata.get_provided_fields() == []


class TestEdgeCases:
    """Tests for edge cases and special scenarios."""

    def test_empty_field_string(self) -> None:
        """Test with empty field string."""

        @requires("")
        def method(self):
            pass

        metadata = get_directives(method)
        assert metadata.has_requires()
        assert metadata.get_required_fields() == []

    def test_whitespace_only_fields(self) -> None:
        """Test with whitespace-only field string."""

        @requires("   ")
        def method(self):
            pass

        metadata = get_directives(method)
        assert metadata.has_requires()
        assert metadata.get_required_fields() == []

    def test_field_with_trailing_comma(self) -> None:
        """Test field names with trailing commas."""

        @requires("field1, field2, field3,")
        def method(self):
            pass

        metadata = get_directives(method)
        assert metadata.get_required_fields() == ["field1", "field2", "field3"]

    def test_underscore_field_names(self) -> None:
        """Test with underscore in field names."""

        @requires("field_one field_two")
        def method(self):
            pass

        metadata = get_directives(method)
        assert metadata.get_required_fields() == ["field_one", "field_two"]

    def test_numeric_field_names(self) -> None:
        """Test with numeric field names."""

        @requires("field1 field2")
        def method(self):
            pass

        metadata = get_directives(method)
        assert metadata.get_required_fields() == ["field1", "field2"]

    def test_directive_on_async_method(self) -> None:
        """Test directives work on async methods."""

        @requires("id")
        async def async_method(self):
            pass

        metadata = get_directives(async_method)
        assert metadata.has_requires()
        assert metadata.get_required_fields() == ["id"]

    def test_directive_on_classmethod(self) -> None:
        """Test directives work on class methods."""

        @requires("field")
        @classmethod
        def class_method(cls):
            pass

        metadata = get_directives(class_method)
        assert metadata.has_requires()
        assert metadata.get_required_fields() == ["field"]

    def test_directive_on_staticmethod(self) -> None:
        """Test directives work on static methods."""

        @requires("field")
        @staticmethod
        def static_method():
            pass

        metadata = get_directives(static_method)
        assert metadata.has_requires()
        assert metadata.get_required_fields() == ["field"]
