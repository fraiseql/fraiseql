"""Unit tests for type-safe GraphQL context."""

import pytest

from fraiseql import GraphQLContext, build_context
from fraiseql.auth.base import UserContext
from fraiseql.cqrs.repository import CQRSRepository


class FakeRepository(CQRSRepository):
    """Minimal fake repository for testing."""

    def __init__(self):
        """Initialize with minimal setup."""
        self.pool = None
        self.context = {}


class TestGraphQLContext:
    """Tests for GraphQLContext dataclass."""

    def test_create_with_defaults(self) -> None:
        """Test creating context with minimal required fields."""
        db = FakeRepository()
        context = GraphQLContext(db=db)

        assert context.db is db
        assert context.user is None
        assert context.request is None
        assert context.response is None
        assert context.loader_registry is None
        assert context.config is None
        assert context.authenticated is False
        assert context._extras == {}

    def test_create_with_user(self) -> None:
        """Test creating context with authenticated user."""
        db = FakeRepository()
        user = UserContext(user_id="user_123", roles=["admin"])

        context = GraphQLContext(db=db, user=user, authenticated=True)

        assert context.db is db
        assert context.user is user
        assert context.user.user_id == "user_123"
        assert context.authenticated is True

    def test_create_with_all_fields(self) -> None:
        """Test creating context with all fields."""
        db = FakeRepository()
        user = UserContext(user_id="user_123")
        request = {"path": "/graphql"}
        response = {"status": 200}

        context = GraphQLContext(
            db=db,
            user=user,
            request=request,
            response=response,
            config=None,
            loader_registry=None,
            authenticated=True,
        )

        assert context.db is db
        assert context.user is user
        assert context.request == request
        assert context.response == response
        assert context.config is None
        assert context.authenticated is True

    def test_from_dict_basic(self) -> None:
        """Test creating context from dictionary."""
        db = FakeRepository()
        context_dict = {"db": db}

        context = GraphQLContext.from_dict(context_dict)

        assert context.db is db
        assert context.user is None
        assert context.authenticated is False

    def test_from_dict_with_user(self) -> None:
        """Test creating context from dictionary with user."""
        db = FakeRepository()
        user = UserContext(user_id="user_123")

        context_dict = {"db": db, "user": user, "authenticated": True}
        context = GraphQLContext.from_dict(context_dict)

        assert context.db is db
        assert context.user is user
        assert context.authenticated is True

    def test_from_dict_with_extras(self) -> None:
        """Test creating context from dictionary with extra fields."""
        db = FakeRepository()
        context_dict = {"db": db, "custom_field": "value1", "another_field": 42}

        context = GraphQLContext.from_dict(context_dict)

        assert context.db is db
        assert context.get_extra("custom_field") == "value1"
        assert context.get_extra("another_field") == 42

    def test_from_dict_missing_db_raises(self) -> None:
        """Test from_dict raises if db is missing."""
        context_dict = {"user": None}

        with pytest.raises(KeyError, match="must contain 'db' key"):
            GraphQLContext.from_dict(context_dict)

    def test_to_dict(self) -> None:
        """Test converting context back to dictionary."""
        db = FakeRepository()
        user = UserContext(user_id="user_123")
        context = GraphQLContext(db=db, user=user, authenticated=True, _extras={"custom": "value"})

        context_dict = context.to_dict()

        assert context_dict["db"] is db
        assert context_dict["user"] is user
        assert context_dict["authenticated"] is True
        assert context_dict["custom"] == "value"

    def test_get_extra_default(self) -> None:
        """Test getting extra field with default."""
        db = FakeRepository()
        context = GraphQLContext(db=db, _extras={"key1": "value1"})

        assert context.get_extra("key1") == "value1"
        assert context.get_extra("nonexistent") is None
        assert context.get_extra("nonexistent", "default") == "default"

    def test_set_extra(self) -> None:
        """Test setting extra field."""
        db = FakeRepository()
        context = GraphQLContext(db=db)

        context.set_extra("request_id", "req_123")
        assert context.get_extra("request_id") == "req_123"

        context.set_extra("request_id", "req_456")
        assert context.get_extra("request_id") == "req_456"


class TestBuildContext:
    """Tests for build_context helper function."""

    def test_build_context_minimal(self) -> None:
        """Test building context with minimal parameters."""
        db = FakeRepository()
        context = build_context(db=db)

        assert context.db is db
        assert context.user is None
        assert context.authenticated is False

    def test_build_context_with_user(self) -> None:
        """Test building context with user."""
        db = FakeRepository()
        user = UserContext(user_id="user_123", roles=["admin"])

        context = build_context(db=db, user=user)

        assert context.db is db
        assert context.user is user
        # authenticated should be inferred from user
        assert context.authenticated is True

    def test_build_context_with_all_parameters(self) -> None:
        """Test building context with all parameters."""
        db = FakeRepository()
        user = UserContext(user_id="user_123")
        request = {"path": "/graphql"}
        response = {"status": 200}

        context = build_context(
            db=db,
            user=user,
            request=request,
            response=response,
            config=None,
            authenticated=True,
        )

        assert context.db is db
        assert context.user is user
        assert context.request == request
        assert context.response == response
        assert context.config is None
        assert context.authenticated is True

    def test_build_context_with_extras(self) -> None:
        """Test building context with extra keyword arguments."""
        db = FakeRepository()
        context = build_context(
            db=db, request_id="req_123", tenant_id="tenant_abc", custom_data={"key": "value"}
        )

        assert context.db is db
        assert context.get_extra("request_id") == "req_123"
        assert context.get_extra("tenant_id") == "tenant_abc"
        assert context.get_extra("custom_data") == {"key": "value"}

    def test_build_context_authenticated_inference(self) -> None:
        """Test automatic authentication status inference."""
        db = FakeRepository()
        user = UserContext(user_id="user_123")

        # Without explicit authenticated parameter, should infer from user
        context1 = build_context(db=db, user=user)
        assert context1.authenticated is True

        # Without user, should be not authenticated
        context2 = build_context(db=db)
        assert context2.authenticated is False

        # Can override inference with explicit value
        context3 = build_context(db=db, user=user, authenticated=False)
        assert context3.authenticated is False

    def test_build_context_round_trip(self) -> None:
        """Test converting context to dict and back."""
        db = FakeRepository()
        user = UserContext(user_id="user_123")

        context1 = build_context(db=db, user=user, request_id="req_123")

        # Convert to dict
        context_dict = context1.to_dict()

        # Convert back from dict
        context2 = GraphQLContext.from_dict(context_dict)

        # Both should be equivalent
        assert context2.db is context1.db
        assert context2.user.user_id == context1.user.user_id
        assert context2.authenticated == context1.authenticated
        assert context2.get_extra("request_id") == "req_123"


class TestContextIntegration:
    """Integration tests for context usage patterns."""

    def test_resolver_pattern_with_type_safety(self) -> None:
        """Test typical resolver usage with type-safe context."""
        # Simulate a resolver
        db = FakeRepository()
        user = UserContext(user_id="user_123", email="user@example.com")

        context: GraphQLContext = build_context(db=db, user=user)

        # Resolver can now access context with full type safety
        # No need for unsafe info.context["user"] patterns
        assert context.user.user_id == "user_123"
        assert context.user.email == "user@example.com"

    def test_context_with_custom_extensions(self) -> None:
        """Test extending context with custom fields."""
        db = FakeRepository()
        context = build_context(
            db=db,
            trace_id="trace_abc",
            span_id="span_xyz",
            tenant_id="tenant_123",
        )

        # Custom fields are accessible via get_extra
        assert context.get_extra("trace_id") == "trace_abc"
        assert context.get_extra("span_id") == "span_xyz"
        assert context.get_extra("tenant_id") == "tenant_123"

    def test_fastapi_context_building(self) -> None:
        """Test building context similar to FastAPI integration."""
        # Simulate FastAPI context building
        db = FakeRepository()
        request = {"headers": {"authorization": "Bearer token"}}
        response = {"headers": {}}
        user = UserContext(user_id="user_123")

        # This mimics build_graphql_context from fastapi integration
        context = build_context(
            db=db,
            user=user,
            request=request,
            response=response,
            authenticated=user is not None,
        )

        # Verify context structure
        assert context.db is db
        assert context.user is user
        assert context.request == request
        assert context.response == response
        assert context.authenticated is True
