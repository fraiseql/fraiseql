"""Tests for FraiseQL decorators."""

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear registry before each test."""
    SchemaRegistry.clear()


def test_type_decorator() -> None:
    """Test @fraiseql.type decorator."""

    @fraiseql.type
    class User:
        """User type."""

        id: int
        name: str
        email: str | None

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 1

    user_type = schema["types"][0]
    assert user_type["name"] == "User"
    assert user_type["description"] == "User type."
    assert len(user_type["fields"]) == 3

    # Check id field
    id_field = next(f for f in user_type["fields"] if f["name"] == "id")
    assert id_field["type"] == "Int"
    assert id_field["nullable"] is False

    # Check name field
    name_field = next(f for f in user_type["fields"] if f["name"] == "name")
    assert name_field["type"] == "String"
    assert name_field["nullable"] is False

    # Check email field (nullable)
    email_field = next(f for f in user_type["fields"] if f["name"] == "email")
    assert email_field["type"] == "String"
    assert email_field["nullable"] is True


def test_query_decorator_simple() -> None:
    """Test @fraiseql.query decorator."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.query(sql_source="v_user")
    def users(limit: int = 10) -> list[User]:
        """Get all users."""
        pass

    schema = SchemaRegistry.get_schema()
    assert len(schema["queries"]) == 1

    users_query = schema["queries"][0]
    assert users_query["name"] == "users"
    assert users_query["return_type"] == "User"
    assert users_query["returns_list"] is True
    assert users_query["nullable"] is False
    assert users_query["description"] == "Get all users."
    assert users_query["sql_source"] == "v_user"

    # Check arguments
    assert len(users_query["arguments"]) == 1
    limit_arg = users_query["arguments"][0]
    assert limit_arg["name"] == "limit"
    assert limit_arg["type"] == "Int"
    assert limit_arg["nullable"] is False
    assert limit_arg["default"] == 10


def test_query_decorator_single_result() -> None:
    """Test @fraiseql.query with single result."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.query(sql_source="v_user")
    def user(id: int) -> User | None:
        """Get user by ID."""
        pass

    schema = SchemaRegistry.get_schema()
    users_query = schema["queries"][0]

    assert users_query["name"] == "user"
    assert users_query["return_type"] == "User"
    assert users_query["returns_list"] is False
    assert users_query["nullable"] is True


def test_mutation_decorator() -> None:
    """Test @fraiseql.mutation decorator."""

    @fraiseql.type
    class User:
        id: int
        name: str
        email: str

    @fraiseql.mutation(sql_source="fn_create_user", operation="CREATE")
    def create_user(name: str, email: str) -> User:
        """Create a new user."""
        pass

    schema = SchemaRegistry.get_schema()
    assert len(schema["mutations"]) == 1

    create_mutation = schema["mutations"][0]
    assert create_mutation["name"] == "create_user"
    assert create_mutation["return_type"] == "User"
    assert create_mutation["returns_list"] is False
    assert create_mutation["nullable"] is False
    assert create_mutation["description"] == "Create a new user."
    assert create_mutation["sql_source"] == "fn_create_user"
    assert create_mutation["operation"] == "CREATE"

    # Check arguments
    assert len(create_mutation["arguments"]) == 2
    name_arg = next(a for a in create_mutation["arguments"] if a["name"] == "name")
    assert name_arg["type"] == "String"
    assert name_arg["nullable"] is False

    email_arg = next(a for a in create_mutation["arguments"] if a["name"] == "email")
    assert email_arg["type"] == "String"
    assert email_arg["nullable"] is False


def test_multiple_types() -> None:
    """Test registering multiple types."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.type
    class Post:
        id: int
        title: str
        author_id: int

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 2

    type_names = [t["name"] for t in schema["types"]]
    assert "User" in type_names
    assert "Post" in type_names


def test_nested_types() -> None:
    """Test nested type references."""

    @fraiseql.type
    class Address:
        street: str
        city: str

    @fraiseql.type
    class User:
        id: int
        name: str
        address: Address

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 2

    user_type = next(t for t in schema["types"] if t["name"] == "User")
    address_field = next(f for f in user_type["fields"] if f["name"] == "address")
    assert address_field["type"] == "Address"


def test_list_types() -> None:
    """Test list type handling."""

    @fraiseql.type
    class User:
        id: int
        tags: list[str]

    schema = SchemaRegistry.get_schema()
    user_type = schema["types"][0]

    tags_field = next(f for f in user_type["fields"] if f["name"] == "tags")
    assert tags_field["type"] == "[String!]"


def test_export_schema(tmp_path: pytest.TempPathFactory) -> None:  # type: ignore[name-defined]
    """Test schema export to JSON file."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.query(sql_source="v_user")
    def users() -> list[User]:
        pass

    output_file = tmp_path / "schema.json"  # type: ignore[operator]
    fraiseql.export_schema(str(output_file))

    # Verify file exists and is valid JSON
    assert output_file.exists()

    import json

    with open(output_file, encoding="utf-8") as f:
        schema = json.load(f)

    assert "types" in schema
    assert "queries" in schema
    assert "mutations" in schema
    assert len(schema["types"]) == 1
    assert len(schema["queries"]) == 1


def test_decorator_without_parentheses() -> None:
    """Test decorators used without parentheses."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.query
    def users() -> list[User]:
        pass

    @fraiseql.mutation
    def create_user(name: str) -> User:
        pass

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 1
    assert len(schema["queries"]) == 1
    assert len(schema["mutations"]) == 1


def test_clear_registry() -> None:
    """Test registry clearing."""

    @fraiseql.type
    class User:
        id: int

    assert len(SchemaRegistry.get_schema()["types"]) == 1

    SchemaRegistry.clear()

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 0
    assert len(schema["queries"]) == 0
    assert len(schema["mutations"]) == 0
    assert len(schema["subscriptions"]) == 0


# =============================================================================
# Subscription Decorator Tests
# =============================================================================


def test_subscription_decorator_simple() -> None:
    """Test @fraiseql.subscription decorator with minimal options."""

    @fraiseql.type
    class Order:
        id: str
        amount: float

    @fraiseql.subscription
    def order_created() -> Order:
        """Subscribe to new orders."""
        pass

    schema = SchemaRegistry.get_schema()
    assert len(schema["subscriptions"]) == 1

    sub = schema["subscriptions"][0]
    assert sub["name"] == "order_created"
    assert sub["entity_type"] == "Order"
    assert sub["nullable"] is False
    assert sub["description"] == "Subscribe to new orders."
    assert len(sub["arguments"]) == 0


def test_subscription_decorator_with_topic() -> None:
    """Test @fraiseql.subscription with topic configuration."""

    @fraiseql.type
    class Order:
        id: str
        user_id: str
        amount: float

    @fraiseql.subscription(topic="orders_created")
    def order_created() -> Order:
        """Subscribe to new orders."""
        pass

    schema = SchemaRegistry.get_schema()
    sub = schema["subscriptions"][0]

    assert sub["name"] == "order_created"
    assert sub["entity_type"] == "Order"
    assert sub["topic"] == "orders_created"


def test_subscription_decorator_with_operation() -> None:
    """Test @fraiseql.subscription with operation filter."""

    @fraiseql.type
    class User:
        id: str
        name: str

    @fraiseql.subscription(operation="UPDATE")
    def user_updated() -> User:
        """Subscribe to user updates."""
        pass

    schema = SchemaRegistry.get_schema()
    sub = schema["subscriptions"][0]

    assert sub["name"] == "user_updated"
    assert sub["entity_type"] == "User"
    assert sub["operation"] == "UPDATE"


def test_subscription_decorator_with_arguments() -> None:
    """Test @fraiseql.subscription with filter arguments."""

    @fraiseql.type
    class Order:
        id: str
        user_id: str
        status: str

    @fraiseql.subscription(topic="order_events")
    def order_status_changed(user_id: str | None = None, status: str | None = None) -> Order:
        """Subscribe to order status changes, optionally filtered by user or status."""
        pass

    schema = SchemaRegistry.get_schema()
    sub = schema["subscriptions"][0]

    assert sub["name"] == "order_status_changed"
    assert sub["entity_type"] == "Order"
    assert len(sub["arguments"]) == 2

    user_arg = next(a for a in sub["arguments"] if a["name"] == "user_id")
    assert user_arg["type"] == "String"
    assert user_arg["nullable"] is True

    status_arg = next(a for a in sub["arguments"] if a["name"] == "status")
    assert status_arg["type"] == "String"
    assert status_arg["nullable"] is True


def test_subscription_decorator_explicit_entity_type() -> None:
    """Test @fraiseql.subscription with explicit entity_type."""

    @fraiseql.type
    class OrderEvent:
        id: str
        order_id: str
        event_type: str

    @fraiseql.subscription(entity_type="Order", topic="order_events")
    def order_event() -> OrderEvent:
        """Subscribe to order events."""
        pass

    schema = SchemaRegistry.get_schema()
    sub = schema["subscriptions"][0]

    # entity_type should be explicit "Order", not inferred "OrderEvent"
    assert sub["entity_type"] == "Order"


def test_subscription_decorator_nullable_return() -> None:
    """Test @fraiseql.subscription with nullable return type."""

    @fraiseql.type
    class User:
        id: str
        name: str

    @fraiseql.subscription
    def user_deleted() -> User | None:
        """Subscribe to user deletions."""
        pass

    schema = SchemaRegistry.get_schema()
    sub = schema["subscriptions"][0]

    assert sub["name"] == "user_deleted"
    assert sub["nullable"] is True


def test_multiple_subscriptions() -> None:
    """Test registering multiple subscriptions."""

    @fraiseql.type
    class Order:
        id: str

    @fraiseql.type
    class User:
        id: str

    @fraiseql.subscription(topic="orders")
    def order_created() -> Order:
        pass

    @fraiseql.subscription(topic="orders")
    def order_updated() -> Order:
        pass

    @fraiseql.subscription(topic="users")
    def user_created() -> User:
        pass

    schema = SchemaRegistry.get_schema()
    assert len(schema["subscriptions"]) == 3

    names = [s["name"] for s in schema["subscriptions"]]
    assert "order_created" in names
    assert "order_updated" in names
    assert "user_created" in names


def test_subscription_in_schema_export(tmp_path: pytest.TempPathFactory) -> None:  # type: ignore[name-defined]
    """Test that subscriptions are included in schema export."""

    @fraiseql.type
    class Order:
        id: str
        amount: float

    @fraiseql.subscription(topic="orders", operation="CREATE")
    def order_created() -> Order:
        """Subscribe to new orders."""
        pass

    output_file = tmp_path / "schema.json"  # type: ignore[operator]
    fraiseql.export_schema(str(output_file))

    import json

    with open(output_file, encoding="utf-8") as f:
        schema = json.load(f)

    assert "subscriptions" in schema
    assert len(schema["subscriptions"]) == 1

    sub = schema["subscriptions"][0]
    assert sub["name"] == "order_created"
    assert sub["entity_type"] == "Order"
    assert sub["topic"] == "orders"
    assert sub["operation"] == "CREATE"


# =============================================================================
# Duplicate Registration Tests
# =============================================================================


def test_duplicate_type_registration_raises_error() -> None:
    """Registering a type with a duplicate name raises ValueError."""

    @fraiseql.type
    class User:
        id: int

    with pytest.raises(ValueError, match="already registered"):

        @fraiseql.type
        class User:  # noqa: F811
            id: str


def test_duplicate_query_registration_raises_error() -> None:
    """Registering a query with a duplicate name raises ValueError."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.query(sql_source="v_user")
    def get_user() -> User:
        pass

    with pytest.raises(ValueError, match="already registered"):

        @fraiseql.query(sql_source="v_user")
        def get_user() -> User:  # noqa: F811
            pass


def test_duplicate_mutation_registration_raises_error() -> None:
    """Registering a mutation with a duplicate name raises ValueError."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.mutation(operation="CREATE")
    def create_user(name: str) -> User:
        pass

    with pytest.raises(ValueError, match="already registered"):

        @fraiseql.mutation(operation="CREATE")
        def create_user(name: str) -> User:  # noqa: F811
            pass


# ---------------------------------------------------------------------------
# inject validation tests
# ---------------------------------------------------------------------------


def test_query_inject_valid_passes_through() -> None:
    """Valid inject mapping on @fraiseql.query is accepted and forwarded."""

    @fraiseql.type
    class Item:
        id: int
        name: str

    @fraiseql.query(sql_source="v_item", inject={"org_id": "jwt:org_id"})
    def items() -> list[Item]:
        """Get items for the current org."""
        pass

    schema = SchemaRegistry.get_schema()
    q = schema["queries"][0]
    assert q["name"] == "items"
    assert q["inject"] == {"org_id": "jwt:org_id"}


def test_mutation_inject_valid_passes_through() -> None:
    """Valid inject mapping on @fraiseql.mutation is accepted and forwarded."""

    @fraiseql.type
    class Item:
        id: int

    @fraiseql.mutation(sql_source="fn_create_item", inject={"tenant_id": "jwt:tenant_id"})
    def create_item(name: str) -> Item:
        """Create an item."""
        pass

    schema = SchemaRegistry.get_schema()
    m = schema["mutations"][0]
    assert m["name"] == "create_item"
    assert m["inject"] == {"tenant_id": "jwt:tenant_id"}


def test_query_inject_invalid_source_raises() -> None:
    """inject source that doesn't match 'jwt:<claim>' raises ValueError."""

    @fraiseql.type
    class Item:
        id: int

    with pytest.raises(ValueError, match="jwt:"):

        @fraiseql.query(sql_source="v_item", inject={"org_id": "header:X-Org-Id"})
        def items() -> list[Item]:
            pass


def test_query_inject_empty_claim_raises() -> None:
    """inject source 'jwt:' with no claim name raises ValueError."""

    @fraiseql.type
    class Item:
        id: int

    with pytest.raises(ValueError, match="jwt:"):

        @fraiseql.query(sql_source="v_item", inject={"org_id": "jwt:"})
        def items() -> list[Item]:
            pass


def test_query_inject_invalid_key_raises() -> None:
    """inject key that is not a valid identifier raises ValueError."""

    @fraiseql.type
    class Item:
        id: int

    with pytest.raises(ValueError, match="valid identifier"):

        @fraiseql.query(sql_source="v_item", inject={"123bad": "jwt:org_id"})
        def items() -> list[Item]:
            pass


def test_query_inject_key_conflicts_with_arg_raises() -> None:
    """inject key that duplicates a GraphQL argument name raises ValueError."""

    @fraiseql.type
    class Item:
        id: int

    with pytest.raises(ValueError, match="conflicts"):

        @fraiseql.query(sql_source="v_item", inject={"name": "jwt:org_id"})
        def items(name: str) -> list[Item]:
            pass


def test_mutation_inject_not_dict_raises() -> None:
    """Passing a non-dict value for inject raises TypeError."""

    @fraiseql.type
    class Item:
        id: int

    with pytest.raises(TypeError, match="dict"):

        @fraiseql.mutation(sql_source="fn_item", inject="jwt:org_id")
        def create_item(name: str) -> Item:
            pass


def test_query_inject_multiple_params() -> None:
    """Multiple inject params are all validated and forwarded."""

    @fraiseql.type
    class Item:
        id: int

    @fraiseql.query(
        sql_source="v_item",
        inject={
            "org_id": "jwt:org_id",
            "user_id": "jwt:sub",
        },
    )
    def items() -> list[Item]:
        pass

    schema = SchemaRegistry.get_schema()
    q = schema["queries"][0]
    assert q["inject"] == {"org_id": "jwt:org_id", "user_id": "jwt:sub"}


def test_query_cache_ttl_valid_passes_through() -> None:
    """cache_ttl_seconds is forwarded to the schema."""

    @fraiseql.type
    class Widget:
        id: int

    @fraiseql.query(sql_source="v_widget", cache_ttl_seconds=300)
    def widgets() -> list[Widget]:
        pass

    schema = SchemaRegistry.get_schema()
    q = schema["queries"][0]
    assert q["cache_ttl_seconds"] == 300


def test_query_cache_ttl_zero_passes_through() -> None:
    """cache_ttl_seconds=0 (disable caching for this query) is allowed."""

    @fraiseql.type
    class Gadget:
        id: int

    @fraiseql.query(sql_source="v_gadget", cache_ttl_seconds=0)
    def gadgets() -> list[Gadget]:
        pass

    schema = SchemaRegistry.get_schema()
    q = schema["queries"][0]
    assert q["cache_ttl_seconds"] == 0


def test_query_cache_ttl_negative_raises() -> None:
    """Negative cache_ttl_seconds raises TypeError."""

    @fraiseql.type
    class Gizmo:
        id: int

    with pytest.raises(TypeError, match="non-negative integer"):

        @fraiseql.query(sql_source="v_gizmo", cache_ttl_seconds=-1)
        def gizmos() -> list[Gizmo]:
            pass


def test_query_cache_ttl_non_int_raises() -> None:
    """Non-integer cache_ttl_seconds raises TypeError."""

    @fraiseql.type
    class Thingo:
        id: int

    with pytest.raises(TypeError, match="non-negative integer"):

        @fraiseql.query(sql_source="v_thingo", cache_ttl_seconds="300")
        def thingos() -> list[Thingo]:
            pass


def test_query_additional_views_valid_passes_through() -> None:
    """Valid additional_views list is passed through to schema."""

    @fraiseql.type
    class Post:
        id: int

    @fraiseql.query(sql_source="v_user_with_posts", additional_views=["v_post", "v_tag"])
    def users_with_posts() -> list[Post]:
        pass

    schema = SchemaRegistry.get_schema()
    q = schema["queries"][0]
    assert q.get("additional_views") == ["v_post", "v_tag"]


def test_query_additional_views_not_list_raises() -> None:
    """Non-list additional_views raises TypeError."""

    @fraiseql.type
    class Widget:
        id: int

    with pytest.raises(TypeError, match="must be a list"):

        @fraiseql.query(sql_source="v_widget", additional_views="v_extra")
        def widgets() -> list[Widget]:
            pass


def test_query_additional_views_invalid_identifier_raises() -> None:
    """Invalid SQL identifier in additional_views raises ValueError."""

    @fraiseql.type
    class Gadget:
        id: int

    with pytest.raises(ValueError, match="not a valid SQL identifier"):

        @fraiseql.query(sql_source="v_gadget", additional_views=["v_good", "bad identifier"])
        def gadgets() -> list[Gadget]:
            pass


# ── @fraiseql.mutation invalidates_fact_tables tests ──────────────────────────


def test_mutation_invalidates_fact_tables_valid_passes_through() -> None:
    """Valid invalidates_fact_tables list is stored in schema output."""

    @fraiseql.type
    class Order:
        id: int

    @fraiseql.mutation(
        sql_source="fn_create_order",
        invalidates_fact_tables=["tf_sales", "tf_order_count"],
    )
    def create_order(amount: float) -> Order:
        pass

    schema = SchemaRegistry.get_schema()
    mut = next(m for m in schema["mutations"] if m["name"] == "create_order")
    assert mut["invalidates_fact_tables"] == ["tf_sales", "tf_order_count"]


def test_mutation_invalidates_fact_tables_empty_list_passes_through() -> None:
    """Empty invalidates_fact_tables is accepted and stored."""

    @fraiseql.type
    class Item:
        id: int

    @fraiseql.mutation(sql_source="fn_create_item", invalidates_fact_tables=[])
    def create_item(name: str) -> Item:
        pass

    schema = SchemaRegistry.get_schema()
    mut = next(m for m in schema["mutations"] if m["name"] == "create_item")
    assert mut["invalidates_fact_tables"] == []


def test_mutation_invalidates_fact_tables_not_list_raises() -> None:
    """Non-list invalidates_fact_tables raises TypeError."""

    @fraiseql.type
    class Widget:
        id: int

    with pytest.raises(TypeError, match="must be a list"):

        @fraiseql.mutation(
            sql_source="fn_create_widget",
            invalidates_fact_tables="tf_sales",
        )
        def create_widget(name: str) -> Widget:
            pass


def test_mutation_invalidates_fact_tables_invalid_identifier_raises() -> None:
    """Invalid SQL identifier in invalidates_fact_tables raises ValueError."""

    @fraiseql.type
    class Sprocket:
        id: int

    with pytest.raises(ValueError, match="not a valid SQL identifier"):

        @fraiseql.mutation(
            sql_source="fn_create_sprocket",
            invalidates_fact_tables=["tf_sales", "bad name"],
        )
        def create_sprocket(name: str) -> Sprocket:
            pass


# ── @fraiseql.mutation invalidates_views tests ────────────────────────────────


def test_mutation_invalidates_views_valid_passes_through() -> None:
    """Valid invalidates_views list is stored in schema output."""

    @fraiseql.type
    class Invoice:
        id: int

    @fraiseql.mutation(
        sql_source="fn_create_invoice",
        invalidates_views=["v_invoice", "v_invoice_summary"],
    )
    def create_invoice(amount: float) -> Invoice:
        pass

    schema = SchemaRegistry.get_schema()
    mut = next(m for m in schema["mutations"] if m["name"] == "create_invoice")
    assert mut["invalidates_views"] == ["v_invoice", "v_invoice_summary"]


def test_mutation_invalidates_views_not_list_raises() -> None:
    """Non-list invalidates_views raises TypeError."""

    @fraiseql.type
    class Gadget:
        id: int

    with pytest.raises(TypeError, match="must be a list"):

        @fraiseql.mutation(
            sql_source="fn_create_gadget",
            invalidates_views="v_gadget",
        )
        def create_gadget(name: str) -> Gadget:
            pass


def test_mutation_invalidates_views_invalid_identifier_raises() -> None:
    """Invalid SQL identifier in invalidates_views raises ValueError."""

    @fraiseql.type
    class Gizmo:
        id: int

    with pytest.raises(ValueError, match="not a valid SQL identifier"):

        @fraiseql.mutation(
            sql_source="fn_create_gizmo",
            invalidates_views=["v_gizmo", "bad view"],
        )
        def create_gizmo(name: str) -> Gizmo:
            pass


# ============================================================================
# sql_source identifier validation
# ============================================================================


def test_query_valid_sql_source_passes() -> None:
    """Valid sql_source values (simple and schema-qualified) are accepted."""

    @fraiseql.type
    class Widget:
        id: int

    @fraiseql.query(sql_source="v_widget")
    def widgets() -> list[Widget]:
        pass

    schema = SchemaRegistry.get_schema()
    q = schema["queries"][-1]
    assert q["sql_source"] == "v_widget"


def test_query_schema_qualified_sql_source_passes() -> None:
    """Schema-qualified sql_source like 'public.v_widget' is accepted."""

    @fraiseql.type
    class Gadget:
        id: int

    @fraiseql.query(sql_source="public.v_gadget")
    def gadgets() -> list[Gadget]:
        pass

    schema = SchemaRegistry.get_schema()
    q = schema["queries"][-1]
    assert q["sql_source"] == "public.v_gadget"


def test_query_injection_sql_source_raises() -> None:
    """SQL injection attempt in sql_source raises ValueError."""

    @fraiseql.type
    class Gizmo:
        id: int

    with pytest.raises(ValueError, match="not a valid SQL identifier"):

        @fraiseql.query(sql_source='v_gizmo"; DROP TABLE users; --')
        def gizmos() -> list[Gizmo]:
            pass


def test_query_sql_source_with_space_raises() -> None:
    """sql_source containing a space raises ValueError."""

    @fraiseql.type
    class Doohickey:
        id: int

    with pytest.raises(ValueError, match="not a valid SQL identifier"):

        @fraiseql.query(sql_source="v user")
        def doohickeys() -> list[Doohickey]:
            pass


def test_mutation_valid_sql_source_passes() -> None:
    """Valid mutation sql_source is accepted."""

    @fraiseql.type
    class Thingamajig:
        id: int

    @fraiseql.mutation(sql_source="fn_create_thingamajig", operation="CREATE")
    def create_thingamajig(name: str) -> Thingamajig:
        pass

    schema = SchemaRegistry.get_schema()
    m = schema["mutations"][-1]
    assert m["sql_source"] == "fn_create_thingamajig"


def test_mutation_injection_sql_source_raises() -> None:
    """SQL injection attempt in mutation sql_source raises ValueError."""

    @fraiseql.type
    class Whatchamacallit:
        id: int

    with pytest.raises(ValueError, match="not a valid SQL identifier"):

        @fraiseql.mutation(sql_source="fn_evil; DROP TABLE users; --", operation="CREATE")
        def create_whatchamacallit(name: str) -> Whatchamacallit:
            pass


# =============================================================================
# requires_role on @type and @query
# =============================================================================


def test_type_requires_role_is_set() -> None:
    """@type(requires_role='admin') emits requires_role in schema."""

    @fraiseql.type(requires_role="admin")
    class SecretReport:
        id: int
        content: str

    schema = SchemaRegistry.get_schema()
    t = next(t for t in schema["types"] if t["name"] == "SecretReport")
    assert t["requires_role"] == "admin"


def test_type_requires_role_absent_by_default() -> None:
    """@type without requires_role does not emit the key."""

    @fraiseql.type
    class PublicData:
        id: int

    schema = SchemaRegistry.get_schema()
    t = next(t for t in schema["types"] if t["name"] == "PublicData")
    assert "requires_role" not in t


def test_query_requires_role_is_set() -> None:
    """@query(requires_role='admin') emits requires_role in schema."""

    @fraiseql.type
    class AuditLog:
        id: int
        action: str

    @fraiseql.query(sql_source="v_audit_log", requires_role="admin")
    def audit_logs() -> list[AuditLog]:
        pass

    schema = SchemaRegistry.get_schema()
    q = next(q for q in schema["queries"] if q["name"] == "audit_logs")
    assert q["requires_role"] == "admin"


def test_query_requires_role_absent_by_default() -> None:
    """@query without requires_role does not emit the key."""

    @fraiseql.type
    class Widget:
        id: int

    @fraiseql.query(sql_source="v_widget")
    def widgets() -> list[Widget]:
        pass

    schema = SchemaRegistry.get_schema()
    q = next(q for q in schema["queries"] if q["name"] == "widgets")
    assert "requires_role" not in q


# =============================================================================
# Phase 03 Cycle 1: @fraiseql.type — missing fields
# =============================================================================


def test_type_decorator_sql_source_is_snake_case() -> None:
    """@fraiseql.type generates sql_source as v_ + snake_case of class name."""

    @fraiseql.type
    class OrderItem:
        id: int

    schema = SchemaRegistry.get_schema()
    t = schema["types"][0]
    assert t["sql_source"] == "v_order_item"


def test_type_decorator_sql_source_simple_name() -> None:
    """Single-word class name maps to v_<lower>."""

    @fraiseql.type
    class Product:
        id: int

    schema = SchemaRegistry.get_schema()
    t = schema["types"][0]
    assert t["sql_source"] == "v_product"


def test_type_decorator_jsonb_column_defaults_to_data() -> None:
    """@fraiseql.type sets jsonb_column to 'data' by default."""

    @fraiseql.type
    class Widget:
        id: int

    schema = SchemaRegistry.get_schema()
    t = schema["types"][0]
    assert t.get("jsonb_column", "data") == "data"


def test_type_decorator_implements() -> None:
    """@fraiseql.type(implements=[...]) emits implements list."""

    @fraiseql.interface
    class Node:
        """A globally unique node."""

        id: str

    @fraiseql.type(implements=["Node"])
    class Article:
        id: str
        title: str

    schema = SchemaRegistry.get_schema()
    t = next(t for t in schema["types"] if t["name"] == "Article")
    assert t["implements"] == ["Node"]


# =============================================================================
# Phase 03 Cycle 2: @fraiseql.error decorator
# =============================================================================


def test_error_type_decorator_sets_is_error_flag() -> None:
    """@fraiseql.error marks the type with is_error: true."""

    @fraiseql.error
    class UserNotFound:
        """Error when user lookup fails."""

        message: str
        code: str

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 1
    t = schema["types"][0]
    assert t["name"] == "UserNotFound"
    assert t["is_error"] is True
    assert len(t["fields"]) == 2


def test_error_type_fields_include_scalar_types() -> None:
    """@fraiseql.error types with int/datetime fields serialize correctly."""
    import datetime

    @fraiseql.error
    class ConflictError:
        message: str
        conflict_id: int
        occurred_at: datetime.datetime

    schema = SchemaRegistry.get_schema()
    t = schema["types"][0]
    assert t["is_error"] is True
    field_names = [f["name"] for f in t["fields"]]
    assert "message" in field_names
    assert "conflict_id" in field_names
    assert "occurred_at" in field_names


def test_error_type_not_is_error_by_default() -> None:
    """@fraiseql.type without error decorator does not emit is_error."""

    @fraiseql.type
    class User:
        id: int

    schema = SchemaRegistry.get_schema()
    t = schema["types"][0]
    assert "is_error" not in t


# =============================================================================
# Phase 03 Cycle 3: @fraiseql.query — missing fields in JSON
# =============================================================================


def test_query_inject_params_in_json() -> None:
    """inject= on @query emits inject_params dict with source/claim structure."""

    @fraiseql.type
    class Order:
        id: int

    @fraiseql.query(sql_source="v_order", inject={"tenant_id": "jwt:tenant_id"})
    def orders() -> list[Order]:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    assert q["inject_params"] == {"tenant_id": {"source": "jwt", "claim": "tenant_id"}}


def test_query_deprecation_in_json() -> None:
    """deprecated= on @query emits deprecation.reason in schema JSON."""

    @fraiseql.type
    class Legacy:
        id: int

    @fraiseql.query(sql_source="v_legacy", deprecated="Use newQuery instead")
    def old_query() -> list[Legacy]:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    assert q["deprecation"]["reason"] == "Use newQuery instead"


def test_query_auto_params_bool_true_expands_to_dict() -> None:
    """auto_params=True on @query expands to all-true dict."""

    @fraiseql.type
    class X:
        id: int

    @fraiseql.query(sql_source="v_x", auto_params=True)
    def xs() -> list[X]:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    ap = q.get("auto_params", {})
    assert ap.get("where") is True
    assert ap.get("order_by") is True
    assert ap.get("limit") is True
    assert ap.get("offset") is True


def test_query_auto_params_dict_passthrough() -> None:
    """auto_params as a dict passes through unchanged."""

    @fraiseql.type
    class Y:
        id: int

    @fraiseql.query(sql_source="v_y", auto_params={"where": True, "limit": False})
    def ys() -> list[Y]:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    assert q["auto_params"] == {"where": True, "limit": False}


def test_query_relay_cursor_type_in_json() -> None:
    """relay_cursor_type= on @query is emitted in schema JSON."""

    @fraiseql.type
    class Item:
        id: int

    @fraiseql.query(sql_source="v_item", relay=True, relay_cursor_type="Int64")
    def items() -> list[Item]:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    assert q["relay_cursor_type"] == "Int64"


# =============================================================================
# Phase 03 Cycle 4: @fraiseql.mutation — missing fields in JSON
# =============================================================================


def test_mutation_inject_params_in_json() -> None:
    """inject= on @mutation emits inject_params dict with source/claim structure."""

    @fraiseql.type
    class Order:
        id: int

    @fraiseql.mutation(sql_source="fn_create_order", inject={"user_id": "jwt:sub"})
    def create_order(name: str) -> Order:
        pass

    m = SchemaRegistry.get_schema()["mutations"][0]
    assert m["inject_params"] == {"user_id": {"source": "jwt", "claim": "sub"}}


def test_mutation_deprecation_in_json() -> None:
    """deprecated= on @mutation emits deprecation.reason in schema JSON."""

    @fraiseql.type
    class X:
        id: int

    @fraiseql.mutation(sql_source="fn_old", deprecated="Use newMutation")
    def old_mutation(x: int) -> X:
        pass

    m = SchemaRegistry.get_schema()["mutations"][0]
    assert m["deprecation"]["reason"] == "Use newMutation"


def test_mutation_invalidates_views_in_json() -> None:
    """invalidates_views emits in JSON and invalidates_fact_tables defaults to []."""

    @fraiseql.type
    class Order:
        id: int

    @fraiseql.mutation(
        sql_source="fn_create_order",
        invalidates_views=["v_order_summary"],
    )
    def create_order(name: str) -> Order:
        pass

    m = SchemaRegistry.get_schema()["mutations"][0]
    assert m["sql_source"] == "fn_create_order"
    assert m["invalidates_views"] == ["v_order_summary"]


# =============================================================================
# REST transport annotations (@fraiseql.query / @fraiseql.mutation)
# =============================================================================


def test_query_rest_path_emits_rest_block() -> None:
    """rest_path= on @query emits a 'rest' block with path and method."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.query(sql_source="v_user", rest_path="/users")
    def users() -> list[User]:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    assert q["rest"] == {"path": "/users", "method": "GET"}


def test_query_rest_path_default_method_is_get() -> None:
    """Default REST method for @query is GET."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.query(sql_source="v_user", rest_path="/users")
    def users() -> list[User]:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    assert q["rest"]["method"] == "GET"


def test_query_rest_path_custom_method() -> None:
    """rest_method= overrides the default method for @query."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.query(sql_source="v_user", rest_path="/users/search", rest_method="POST")
    def search_users(query: str) -> list[User]:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    assert q["rest"]["method"] == "POST"


def test_query_rest_path_with_path_param() -> None:
    """Path parameter {id} declared in function sig passes validation."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.query(sql_source="v_user", rest_path="/users/{id}")
    def get_user(id: int) -> User | None:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    assert q["rest"]["path"] == "/users/{id}"


def test_query_rest_path_undeclared_param_raises() -> None:
    """Path parameter not in function signature raises ValueError."""

    @fraiseql.type
    class User:
        id: int

    with pytest.raises(ValueError, match="not a declared function argument"):

        @fraiseql.query(sql_source="v_user", rest_path="/users/{user_id}")
        def get_user(id: int) -> User | None:
            pass


def test_query_rest_path_invalid_method_raises() -> None:
    """Invalid HTTP method raises ValueError."""

    @fraiseql.type
    class User:
        id: int

    with pytest.raises(ValueError, match="not a valid HTTP method"):

        @fraiseql.query(sql_source="v_user", rest_path="/users", rest_method="FETCH")
        def users() -> list[User]:
            pass


def test_query_rest_method_without_rest_path_is_discarded() -> None:
    """rest_method= without rest_path= is silently discarded (no error, no 'rest' key)."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.query(sql_source="v_user", rest_method="GET")
    def users() -> list[User]:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    assert "rest" not in q


def test_query_without_rest_path_has_no_rest_block() -> None:
    """@query without rest_path emits no 'rest' key."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.query(sql_source="v_user")
    def users() -> list[User]:
        pass

    q = SchemaRegistry.get_schema()["queries"][0]
    assert "rest" not in q


def test_mutation_rest_path_emits_rest_block() -> None:
    """rest_path= on @mutation emits a 'rest' block with path and method."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.mutation(sql_source="fn_create_user", rest_path="/users")
    def create_user(name: str) -> User:
        pass

    m = SchemaRegistry.get_schema()["mutations"][0]
    assert m["rest"] == {"path": "/users", "method": "POST"}


def test_mutation_rest_path_default_method_is_post() -> None:
    """Default REST method for @mutation is POST."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.mutation(sql_source="fn_create_user", rest_path="/users")
    def create_user(name: str) -> User:
        pass

    m = SchemaRegistry.get_schema()["mutations"][0]
    assert m["rest"]["method"] == "POST"


def test_mutation_rest_path_custom_method_put() -> None:
    """rest_method=PUT overrides POST default for @mutation."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.mutation(
        sql_source="fn_update_user",
        rest_path="/users/{id}",
        rest_method="PUT",
    )
    def update_user(id: int, name: str) -> User:
        pass

    m = SchemaRegistry.get_schema()["mutations"][0]
    assert m["rest"] == {"path": "/users/{id}", "method": "PUT"}


def test_mutation_rest_path_delete_method() -> None:
    """rest_method=DELETE works for @mutation."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.mutation(
        sql_source="fn_delete_user",
        rest_path="/users/{id}",
        rest_method="DELETE",
    )
    def delete_user(id: int) -> User:
        pass

    m = SchemaRegistry.get_schema()["mutations"][0]
    assert m["rest"]["method"] == "DELETE"


def test_mutation_rest_path_undeclared_param_raises() -> None:
    """Path parameter not in mutation signature raises ValueError."""

    @fraiseql.type
    class User:
        id: int

    with pytest.raises(ValueError, match="not a declared function argument"):

        @fraiseql.mutation(sql_source="fn_update_user", rest_path="/users/{user_id}")
        def update_user(id: int, name: str) -> User:
            pass


def test_mutation_rest_path_method_case_insensitive() -> None:
    """rest_method= is case-insensitive (lowercased input works)."""

    @fraiseql.type
    class User:
        id: int

    @fraiseql.mutation(sql_source="fn_create_user", rest_path="/users", rest_method="post")
    def create_user(name: str) -> User:
        pass

    m = SchemaRegistry.get_schema()["mutations"][0]
    assert m["rest"]["method"] == "POST"
