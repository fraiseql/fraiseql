"""Test FraiseQL type serialization issue and fix."""

import json
import uuid
from dataclasses import dataclass
from datetime import date, datetime

import pytest

import fraiseql
from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder, FraiseQLJSONResponse


@pytest.mark.unit
class TestFraiseTypeJSONSerialization:
    """Test that @fraiseql.type decorated classes are JSON serializable."""

    def test_fraiseql_type_direct_serialization_fails(self) -> None:
        """Test that direct JSON serialization currently fails (reproducing bug)."""

        @fraiseql.type(sql_source="tv_allocation")
        @dataclass
        class Allocation:
            """Test allocation type."""

            id: uuid.UUID
            identifier: str
            start_date: date | None

        allocation = Allocation(
            id=uuid.uuid4(), identifier="TEST-001", start_date=date(2024, 1, 15)
        )

        # This should fail with current implementation
        with pytest.raises(TypeError, match="Object of type Allocation is not JSON serializable"):
            json.dumps(allocation)

    def test_fraiseql_type_in_graphql_response_fails(self) -> None:
        """Test that FraiseQL types in GraphQL responses fail to serialize (reproducing bug)."""

        @fraiseql.type(sql_source="tv_allocation")
        @dataclass
        class Allocation:
            """Test allocation type."""

            id: uuid.UUID
            identifier: str
            start_date: date | None

        allocation = Allocation(
            id=uuid.uuid4(), identifier="TEST-001", start_date=date(2024, 1, 15)
        )

        # Simulate GraphQL response structure
        response_data = {"data": {"allocations": [allocation]}}

        # This should fail with current implementation
        with pytest.raises(TypeError, match="Object of type Allocation is not JSON serializable"):
            json.dumps(response_data)

    def test_fraiseql_json_encoder_handles_fraiseql_types(self) -> None:
        """Test that FraiseQLJSONEncoder should handle FraiseQL types (this will fail initially)."""

        @fraiseql.type(sql_source="tv_user")
        @dataclass
        class User:
            """Test user type with various field types."""

            id: uuid.UUID
            name: str
            email: str | None
            created_at: datetime
            is_active: bool = True

        user = User(
            id=uuid.uuid4(),
            name="John Doe",
            email="john@example.com",
            created_at=datetime(2024, 1, 15, 10, 30, 0),
            is_active=True,
        )

        encoder = FraiseQLJSONEncoder()

        # This should work once we implement the fix
        json_str = encoder.encode(user)
        result = json.loads(json_str)

        # Should serialize as a dictionary with the object's fields
        assert isinstance(result, dict)
        assert str(result["id"]) == str(user.id)
        assert result["name"] == "John Doe"
        assert result["email"] == "john@example.com"
        assert result["created_at"] == "2024-01-15T10:30:00"
        assert result["is_active"] is True

    def test_fraiseql_json_encoder_handles_nested_fraiseql_types(self) -> None:
        """Test that FraiseQLJSONEncoder handles nested FraiseQL types."""

        @fraiseql.type(sql_source="tv_department")
        @dataclass
        class Department:
            """Test department type."""

            id: uuid.UUID
            name: str

        @fraiseql.type(sql_source="tv_user")
        @dataclass
        class User:
            """Test user type with nested department."""

            id: uuid.UUID
            name: str
            department: Department | None

        dept = Department(id=uuid.uuid4(), name="Engineering")

        user = User(id=uuid.uuid4(), name="John Doe", department=dept)

        encoder = FraiseQLJSONEncoder()

        # This should work once we implement the fix
        json_str = encoder.encode(user)
        result = json.loads(json_str)

        assert isinstance(result, dict)
        assert result["name"] == "John Doe"
        assert isinstance(result["department"], dict)
        assert result["department"]["name"] == "Engineering"

    def test_fraiseql_json_encoder_handles_lists_of_fraiseql_types(self) -> None:
        """Test that FraiseQLJSONEncoder handles lists of FraiseQL types."""

        @fraiseql.type(sql_source="tv_product")
        @dataclass
        class Product:
            """Test product type."""

            id: uuid.UUID
            name: str
            price: float

        products = [
            Product(id=uuid.uuid4(), name="Product 1", price=10.99),
            Product(id=uuid.uuid4(), name="Product 2", price=25.50),
        ]

        response_data = {"data": {"products": products}}

        encoder = FraiseQLJSONEncoder()

        # This should work once we implement the fix
        json_str = encoder.encode(response_data)
        result = json.loads(json_str)

        assert "data" in result
        assert "products" in result["data"]
        assert len(result["data"]["products"]) == 2
        assert result["data"]["products"][0]["name"] == "Product 1"
        assert result["data"]["products"][1]["price"] == 25.50

    def test_fraiseql_json_response_with_fraiseql_types(self) -> None:
        """Test that FraiseQLJSONResponse works with FraiseQL types."""

        @fraiseql.type(sql_source="tv_order")
        @dataclass
        class Order:
            """Test order type."""

            id: uuid.UUID
            total: float
            created_at: datetime

        order = Order(id=uuid.uuid4(), total=99.99, created_at=datetime(2024, 1, 15, 14, 30, 0))

        content = {"data": {"order": order}, "errors": None}

        # This should work once we implement the fix
        response = FraiseQLJSONResponse(content=content)
        rendered = response.render(content)
        result = json.loads(rendered.decode("utf-8"))

        assert "data" in result
        assert "order" in result["data"]
        assert result["data"]["order"]["total"] == 99.99
        assert result["data"]["order"]["created_at"] == "2024-01-15T14:30:00"

    def test_fraiseql_type_without_dataclass_decorator(self) -> None:
        """Test that FraiseQL types work without @dataclass decorator."""

        @fraiseql.type(sql_source="tv_simple")
        class SimpleType:
            """Test type without @dataclass but with __init__ created by FraiseQL."""

            id: uuid.UUID
            name: str

        # The @fraiseql.type decorator should create __init__ for us
        simple = SimpleType(id=uuid.uuid4(), name="Test")

        encoder = FraiseQLJSONEncoder()

        # This should work once we implement the fix
        json_str = encoder.encode(simple)
        result = json.loads(json_str)

        assert isinstance(result, dict)
        assert result["name"] == "Test"
        assert "id" in result

    def test_from_dict_method_exists_and_works(self) -> None:
        """Test that @fraiseql.type adds from_dict method correctly."""

        @fraiseql.type(sql_source="tv_test")
        @dataclass
        class TestType:
            """Test type for from_dict functionality."""

            id: uuid.UUID
            name: str
            created_at: datetime | None = None

        # Should have from_dict class method
        assert hasattr(TestType, "from_dict")
        assert callable(TestType.from_dict)

        # Test from_dict with camelCase data (as would come from GraphQL)
        data = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "name": "Test Name",
            "createdAt": "2024-01-15T10:30:00",  # camelCase
        }

        instance = TestType.from_dict(data)

        assert isinstance(instance, TestType)
        assert str(instance.id) == "123e4567-e89b-12d3-a456-426614174000"
        assert instance.name == "Test Name"
        assert instance.created_at == "2024-01-15T10:30:00"  # Type conversion handled

    def test_fraiseql_type_has_required_attributes(self) -> None:
        """Test that @fraiseql.type adds required FraiseQL attributes."""

        @fraiseql.type(sql_source="tv_test")
        @dataclass
        class TestType:
            """Test type for attribute checking."""

            id: uuid.UUID
            name: str

        # Should have FraiseQL definition
        assert hasattr(TestType, "__fraiseql_definition__")
        assert TestType.__fraiseql_definition__ is not None

        # Should have SQL source
        assert hasattr(TestType, "__gql_table__")
        assert TestType.__gql_table__ == "tv_test"

    def test_fraiseql_json_encoder_includes_cascade_attribute(self) -> None:
        """Test that FraiseQLJSONEncoder includes __cascade__ attribute as 'cascade'."""

        # Create a mock object that mimics a FraiseQL type
        class MockFraiseQLType:
            def __init__(self, id_val: str, name: str):
                self.id = id_val
                self.name = name
                self.__cascade__ = {
                    "updated": [{"__typename": "TestType", "id": id_val, "operation": "CREATED"}],
                    "deleted": [],
                    "invalidations": [{"queryName": "tests", "strategy": "INVALIDATE"}],
                }
                # Add the FraiseQL definition attribute to make it a "FraiseQL type"
                self.__fraiseql_definition__ = True

        # Create instance
        test_instance = MockFraiseQLType("test-id-123", "test")

        # Serialize using FraiseQLJSONEncoder
        result = json.loads(json.dumps(test_instance, cls=FraiseQLJSONEncoder))

        # Should include cascade data
        assert "cascade" in result
        assert result["cascade"]["updated"][0]["__typename"] == "TestType"
        assert result["cascade"]["invalidations"][0]["queryName"] == "tests"

        # Should not include __cascade__
        assert "__cascade__" not in result
