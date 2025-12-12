"""Integration tests for auto-generation with database operations."""

from dataclasses import asdict, dataclass
from uuid import UUID

import pytest

# Import database fixtures
import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.types.lazy_properties import clear_auto_generated_cache

pytestmark = pytest.mark.integration


class TestAutoGenerationIntegration:
    """Test auto-generation works with actual database queries."""

    @pytest.fixture(scope="class")
    def clear_registry_fixture(self, clear_registry_class):
        """Clear registry before class tests."""
        return

    @pytest.mark.asyncio
    async def test_auto_generated_where_input_in_query(
        self, class_db_pool, test_schema, clear_registry_fixture
    ) -> None:
        """Test that auto-generated WhereInput works in db.find()."""
        clear_auto_generated_cache()

        @fraiseql.type(sql_source="tv_customers_auto_test")
        @dataclass
        class CustomerAutoTest:
            id: UUID
            name: str
            email: str

        # Setup test data
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}")
            await conn.execute("""
                CREATE TABLE tv_customers_auto_test (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    name TEXT NOT NULL,
                    email TEXT,
                    data JSONB
                );
                INSERT INTO tv_customers_auto_test (name, email, data)
                VALUES
                    ('Alice Anderson', 'alice@test.com', '{"name": "Alice Anderson", "email": "alice@test.com"}'),
                    ('Bob Brown', 'bob@test.com', '{"name": "Bob Brown", "email": "bob@test.com"}'),
                    ('Charlie Chen', 'charlie@test.com', '{"name": "Charlie Chen", "email": "charlie@test.com"}');
            """)
            await conn.commit()

        # Register type for view
        register_type_for_view(
            "tv_customers_auto_test",
            CustomerAutoTest,
            table_columns={"id", "name", "email", "data"},
            has_jsonb_data=True,
        )

        db = FraiseQLRepository(class_db_pool)

        # Use auto-generated WhereInput
        WhereInput = CustomerAutoTest.WhereInput
        assert WhereInput is not None

        # Test simple filter
        where = WhereInput(name={"eq": "Alice Anderson"})
        where_dict = {k: v for k, v in asdict(where).items() if v is not None}
        response = await db.find("tv_customers_auto_test", where=where_dict)
        results = response.to_json()["data"]["tv_customers_auto_test"]
        assert len(results) == 1
        assert results[0]["name"] == "Alice Anderson"

        # Test contains filter
        where = WhereInput(name={"contains": "Brown"})
        where_dict = {k: v for k, v in asdict(where).items() if v is not None}
        response = await db.find("tv_customers_auto_test", where=where_dict)
        results = response.to_json()["data"]["tv_customers_auto_test"]
        assert len(results) == 1
        assert results[0]["name"] == "Bob Brown"

        # Test logical operators
        where = WhereInput(
            OR=[{"name": {"eq": "Alice Anderson"}}, {"name": {"eq": "Charlie Chen"}}]
        )
        where_dict = {k: v for k, v in asdict(where).items() if v is not None}
        response = await db.find("tv_customers_auto_test", where=where_dict)
        results = response.to_json()["data"]["tv_customers_auto_test"]
        assert len(results) == 2

    @pytest.mark.asyncio
    async def test_auto_generated_order_by_in_query(
        self, class_db_pool, test_schema, clear_registry_fixture
    ) -> None:
        """Test that auto-generated OrderBy works in db.find()."""
        clear_auto_generated_cache()

        @fraiseql.type(sql_source="tv_products_auto_test")
        @dataclass
        class ProductAutoTest:
            id: UUID
            name: str
            price: float

        # Setup test data
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}")
            await conn.execute("""
                CREATE TABLE tv_products_auto_test (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    name TEXT NOT NULL,
                    price DECIMAL(10,2),
                    data JSONB
                );
                INSERT INTO tv_products_auto_test (name, price, data)
                VALUES
                    ('Widget', 9.99, '{"name": "Widget", "price": 9.99}'),
                    ('Gadget', 19.99, '{"name": "Gadget", "price": 19.99}'),
                    ('Doohickey', 5.99, '{"name": "Doohickey", "price": 5.99}');
            """)
            await conn.commit()

        # Register type for view
        register_type_for_view(
            "tv_products_auto_test",
            ProductAutoTest,
            table_columns={"id", "name", "price", "data"},
            has_jsonb_data=True,
        )

        db = FraiseQLRepository(class_db_pool)

        # Use auto-generated OrderBy
        OrderBy = ProductAutoTest.OrderBy
        assert OrderBy is not None

        # Test ascending order
        order_by = OrderBy(price="ASC")
        response = await db.find("tv_products_auto_test", order_by=order_by)

        results = response.to_json()["data"]["tv_products_auto_test"]
        assert len(results) == 3
        assert results[0]["name"] == "Doohickey"  # $5.99
        assert results[1]["name"] == "Widget"  # $9.99
        assert results[2]["name"] == "Gadget"  # $19.99

        # Test descending order
        order_by = OrderBy(price="DESC")
        response = await db.find("tv_products_auto_test", order_by=order_by)

        results = response.to_json()["data"]["tv_products_auto_test"]
        assert len(results) == 3
        assert results[0]["name"] == "Gadget"  # $19.99
        assert results[1]["name"] == "Widget"  # $9.99
        assert results[2]["name"] == "Doohickey"  # $5.99

        # Test ordering by name
        order_by = OrderBy(name="ASC")
        response = await db.find("tv_products_auto_test", order_by=order_by)

        results = response.to_json()["data"]["tv_products_auto_test"]
        assert len(results) == 3
        assert results[0]["name"] == "Doohickey"
        assert results[1]["name"] == "Gadget"
        assert results[2]["name"] == "Widget"

    @pytest.mark.asyncio
    async def test_combined_where_and_order_by_auto_generated(
        self, class_db_pool, test_schema, clear_registry_fixture
    ) -> None:
        """Test using both auto-generated WhereInput and OrderBy together."""
        clear_auto_generated_cache()

        @fraiseql.type(sql_source="tv_inventory_auto_test")
        @dataclass
        class InventoryAutoTest:
            id: UUID
            item_name: str
            quantity: int
            category: str

        # Setup test data
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}")
            await conn.execute("""
                DROP TABLE IF EXISTS tv_inventory_auto_test CASCADE;
                CREATE TABLE tv_inventory_auto_test (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    item_name TEXT NOT NULL,
                    quantity INT,
                    category TEXT,
                    data JSONB
                );
                INSERT INTO tv_inventory_auto_test (item_name, quantity, category, data)
                VALUES
                    ('Laptop', 10, 'Electronics', '{"item_name": "Laptop", "quantity": 10, "category": "Electronics"}'),
                    ('Mouse', 50, 'Electronics', '{"item_name": "Mouse", "quantity": 50, "category": "Electronics"}'),
                    ('Desk', 5, 'Furniture', '{"item_name": "Desk", "quantity": 5, "category": "Furniture"}'),
                    ('Chair', 20, 'Furniture', '{"item_name": "Chair", "quantity": 20, "category": "Furniture"}');
            """)
            await conn.commit()

        # Register type for view
        register_type_for_view(
            "tv_inventory_auto_test",
            InventoryAutoTest,
            table_columns={"id", "item_name", "quantity", "category", "data"},
            has_jsonb_data=True,
        )

        db = FraiseQLRepository(class_db_pool)

        # Use both auto-generated types
        WhereInput = InventoryAutoTest.WhereInput
        OrderBy = InventoryAutoTest.OrderBy

        # Filter by category and order by quantity
        where = WhereInput(category={"eq": "Electronics"})
        where_dict = {k: v for k, v in asdict(where).items() if v is not None}
        order_by = OrderBy(quantity="ASC")
        response = await db.find("tv_inventory_auto_test", where=where_dict, order_by=order_by)

        results = response.to_json()["data"]["tv_inventory_auto_test"]

        assert len(results) == 2
        assert results[0]["itemName"] == "Laptop"  # quantity 10
        assert results[1]["itemName"] == "Mouse"  # quantity 50

        # Filter by category and order by name descending
        where = WhereInput(category={"eq": "Furniture"})
        where_dict = {k: v for k, v in asdict(where).items() if v is not None}
        order_by = OrderBy(item_name="DESC")
        response = await db.find("tv_inventory_auto_test", where=where_dict, order_by=order_by)

        results = response.to_json()["data"]["tv_inventory_auto_test"]

        assert len(results) == 2
        assert results[0]["itemName"] == "Desk"
        assert results[1]["itemName"] == "Chair"

    @pytest.mark.asyncio
    async def test_auto_generated_with_limit_and_offset(
        self, class_db_pool, test_schema, clear_registry_fixture
    ) -> None:
        """Test auto-generated types work with pagination."""
        clear_auto_generated_cache()

        @fraiseql.type(sql_source="tv_users_auto_test")
        @dataclass
        class UserAutoTest:
            id: UUID
            username: str
            age: int

        # Setup test data
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}")
            await conn.execute("""
                DROP TABLE IF EXISTS tv_users_auto_test CASCADE;
                CREATE TABLE tv_users_auto_test (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    username TEXT NOT NULL,
                    age INT,
                    data JSONB
                );
                INSERT INTO tv_users_auto_test (username, age, data)
                VALUES
                    ('user1', 25, '{"username": "user1", "age": 25}'),
                    ('user2', 30, '{"username": "user2", "age": 30}'),
                    ('user3', 35, '{"username": "user3", "age": 35}'),
                    ('user4', 40, '{"username": "user4", "age": 40}'),
                    ('user5', 45, '{"username": "user5", "age": 45}');
            """)
            await conn.commit()

        # Register type for view
        register_type_for_view(
            "tv_users_auto_test",
            UserAutoTest,
            table_columns={"id", "username", "age", "data"},
            has_jsonb_data=True,
        )

        db = FraiseQLRepository(class_db_pool)

        OrderBy = UserAutoTest.OrderBy

        # Test pagination with order
        order_by = OrderBy(age="ASC")

        # First page
        response = await db.find("tv_users_auto_test", order_by=order_by, limit=2, offset=0)

        results = response.to_json()["data"]["tv_users_auto_test"]
        assert len(results) == 2
        assert results[0]["username"] == "user1"
        assert results[1]["username"] == "user2"

        # Second page
        response = await db.find("tv_users_auto_test", order_by=order_by, limit=2, offset=2)

        results = response.to_json()["data"]["tv_users_auto_test"]
        assert len(results) == 2
        assert results[0]["username"] == "user3"
        assert results[1]["username"] == "user4"

        # Third page
        response = await db.find("tv_users_auto_test", order_by=order_by, limit=2, offset=4)

        results = response.to_json()["data"]["tv_users_auto_test"]
        assert len(results) == 1
        assert results[0]["username"] == "user5"


@pytest.mark.asyncio
@pytest.mark.integration
async def test_nested_auto_generation_with_fk_detection(
    class_db_pool, test_schema, clear_registry
) -> None:
    """Test that nested auto-generated WhereInput works with FK detection."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="tv_customers_nested_auto")
    @dataclass
    class CustomerNestedAuto:
        id: UUID
        name: str

    @fraiseql.type(sql_source="tv_orders_nested_auto")
    @dataclass
    class OrderNestedAuto:
        id: UUID
        customer_id: UUID
        order_number: str
        customer: CustomerNestedAuto | None

    # Setup test data
    async with class_db_pool.connection() as conn:
        await conn.execute(f"SET search_path TO {test_schema}")
        await conn.execute("""
            DROP TABLE IF EXISTS tv_orders_nested_auto CASCADE;
            DROP TABLE IF EXISTS tv_customers_nested_auto CASCADE;

            CREATE TABLE tv_customers_nested_auto (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL,
                data JSONB
            );

            CREATE TABLE tv_orders_nested_auto (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                customer_id UUID REFERENCES tv_customers_nested_auto(id),
                order_number TEXT,
                data JSONB
            );

            INSERT INTO tv_customers_nested_auto (name, data)
            VALUES
                ('Customer Alpha', '{"name": "Customer Alpha"}'),
                ('Customer Beta', '{"name": "Customer Beta"}');
        """)

        result = await conn.execute(
            "SELECT id FROM tv_customers_nested_auto WHERE name = 'Customer Alpha'"
        )
        customer_alpha_id = (await result.fetchone())[0]

        result = await conn.execute(
            "SELECT id FROM tv_customers_nested_auto WHERE name = 'Customer Beta'"
        )
        customer_beta_id = (await result.fetchone())[0]

        await conn.execute(
            f"""
            INSERT INTO tv_orders_nested_auto (customer_id, order_number, data)
            VALUES
                ('{customer_alpha_id}', 'ORD-001', '{{"orderNumber": "ORD-001", "customer": {{"id": "{customer_alpha_id}", "name": "Customer Alpha"}}}}'),
                ('{customer_alpha_id}', 'ORD-002', '{{"orderNumber": "ORD-002", "customer": {{"id": "{customer_alpha_id}", "name": "Customer Alpha"}}}}'),
                ('{customer_beta_id}', 'ORD-003', '{{"orderNumber": "ORD-003", "customer": {{"id": "{customer_beta_id}", "name": "Customer Beta"}}}}');
            """
        )
        await conn.commit()

    db = FraiseQLRepository(class_db_pool)

    # Register FK metadata
    register_type_for_view(
        "tv_orders_nested_auto",
        OrderNestedAuto,
        table_columns={"id", "customer_id", "order_number", "data"},
        has_jsonb_data=True,
    )

    # Use auto-generated nested WhereInput
    OrderWhere = OrderNestedAuto.WhereInput

    # Verify nested customer field exists
    assert "customer" in OrderWhere.__annotations__

    # Test filtering by nested customer name
    where = OrderWhere(customer={"name": {"eq": "Customer Alpha"}})
    where_dict = {k: v for k, v in asdict(where).items() if v is not None}
    response = await db.find("tv_orders_nested_auto", where=where_dict)

    results = response.to_json()["data"]["tv_orders_nested_auto"]

    assert len(results) == 2
    assert results[0]["orderNumber"] in ["ORD-001", "ORD-002"]
    assert results[1]["orderNumber"] in ["ORD-001", "ORD-002"]

    # Test filtering by different customer
    where = OrderWhere(customer={"name": {"eq": "Customer Beta"}})
    where_dict = {k: v for k, v in asdict(where).items() if v is not None}
    response = await db.find("tv_orders_nested_auto", where=where_dict)

    results = response.to_json()["data"]["tv_orders_nested_auto"]

    assert len(results) == 1
    assert results[0]["orderNumber"] == "ORD-003"
