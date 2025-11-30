import pytest

"""Tests for GraphQL where input type generation."""

import uuid
from dataclasses import dataclass
from datetime import UTC, date, datetime
from decimal import Decimal
from typing import Optional

import fraiseql
from fraiseql.sql import (
    BooleanFilter,
    DateFilter,
    DateTimeFilter,
    DecimalFilter,
    FloatFilter,
    IntFilter,
    StringFilter,
    UUIDFilter,
    create_graphql_where_input,
)

pytestmark = [pytest.mark.integration, pytest.mark.database]


@pytest.mark.unit
@dataclass
class SimpleModel:
    """Simple test model."""

    id: int
    name: str
    is_active: bool


@fraiseql.type
class Product:
    """Product model for testing."""

    id: uuid.UUID
    name: str
    price: Decimal
    stock: int
    weight: float
    is_active: bool
    created_at: datetime
    release_date: date
    description: Optional[str] = None


class TestFilterTypes:
    """Test the individual filter types."""

    def test_string_filter(self) -> None:
        """Test StringFilter creation and fields."""
        filter_obj = StringFilter(
            eq="test",
            contains="est",
            startswith="te",
            endswith="st",
            in_=["test1", "test2"],
            isnull=False,
        )

        assert filter_obj.eq == "test"
        assert filter_obj.contains == "est"
        assert filter_obj.startswith == "te"
        assert filter_obj.endswith == "st"
        assert filter_obj.in_ == ["test1", "test2"]
        assert filter_obj.isnull is False

    def test_int_filter(self) -> None:
        """Test IntFilter creation and fields."""
        filter_obj = IntFilter(
            eq=10, neq=5, gt=0, gte=1, lt=100, lte=99, in_=[1, 2, 3], isnull=False
        )

        assert filter_obj.eq == 10
        assert filter_obj.neq == 5
        assert filter_obj.gt == 0
        assert filter_obj.gte == 1
        assert filter_obj.lt == 100
        assert filter_obj.lte == 99
        assert filter_obj.in_ == [1, 2, 3]

    def test_uuid_filter(self) -> None:
        """Test UUIDFilter creation and fields."""
        test_uuid = uuid.uuid4()
        filter_obj = UUIDFilter(eq=test_uuid, in_=[test_uuid])

        assert filter_obj.eq == test_uuid
        assert filter_obj.in_ == [test_uuid]

    def test_boolean_filter(self) -> None:
        """Test BooleanFilter creation and fields."""
        filter_obj = BooleanFilter(eq=True, neq=False)

        assert filter_obj.eq is True
        assert filter_obj.neq is False


class TestCreateGraphQLWhereInput:
    """Test create_graphql_where_input function."""

    def test_simple_model_where_input(self) -> None:
        """Test creating where input for simple model."""
        SimpleWhereInput = create_graphql_where_input(SimpleModel)

        # Check class was created
        assert SimpleWhereInput.__name__ == "SimpleModelWhereInput"
        assert hasattr(SimpleWhereInput, "__dataclass_fields__")

        # Check fields exist
        assert "id" in SimpleWhereInput.__dataclass_fields__
        assert "name" in SimpleWhereInput.__dataclass_fields__
        assert "is_active" in SimpleWhereInput.__dataclass_fields__

        # Create instance
        where_input = SimpleWhereInput(
            id=IntFilter(eq=1), name=StringFilter(contains="test"), is_active=BooleanFilter(eq=True)
        )

        assert where_input.id.eq == 1
        assert where_input.name.contains == "test"
        assert where_input.is_active.eq is True

    def test_complex_model_where_input(self) -> None:
        """Test creating where input for complex model with various types."""
        ProductWhereInput = create_graphql_where_input(Product)

        # Check fields were mapped to correct filter types
        where_input = ProductWhereInput()

        # Set some filters
        where_input.name = StringFilter(startswith="Widget")
        where_input.price = DecimalFilter(gte=Decimal("10.00"), lte=Decimal("100.00"))
        where_input.stock = IntFilter(gt=0)
        where_input.is_active = BooleanFilter(eq=True)

        assert where_input.name.startswith == "Widget"
        assert where_input.price.gte == Decimal("10.00")
        assert where_input.price.lte == Decimal("100.00")
        assert where_input.stock.gt == 0
        assert where_input.is_active.eq is True

    def test_custom_name_where_input(self) -> None:
        """Test creating where input with custom name."""
        CustomInput = create_graphql_where_input(SimpleModel, name="CustomFilterInput")

        assert CustomInput.__name__ == "CustomFilterInput"

    def test_optional_fields_handling(self) -> None:
        """Test that optional fields are handled correctly."""

        @dataclass
        class OptionalModel:
            id: int
            name: Optional[str]
            count: int | None

        OptionalWhereInput = create_graphql_where_input(OptionalModel)
        where_input = OptionalWhereInput()

        # All fields should be optional in the where input
        assert where_input.id is None
        assert where_input.name is None
        assert where_input.count is None

    def test_conversion_to_sql_where(self) -> None:
        """Test conversion from GraphQL input to SQL where type."""
        ProductWhereInput = create_graphql_where_input(Product)

        # Create GraphQL where input
        where_input = ProductWhereInput(
            name=StringFilter(contains="test"),
            price=DecimalFilter(gt=Decimal(50)),
            is_active=BooleanFilter(eq=True),
        )

        # Convert to SQL where type
        sql_where = where_input._to_sql_where()

        # Check it's the correct type
        assert hasattr(sql_where, "to_sql")

        # Check field values were converted
        assert hasattr(sql_where, "name")
        assert hasattr(sql_where, "price")
        assert hasattr(sql_where, "is_active")

        # Verify the operator dictionaries
        assert sql_where.name == {"contains": "test"}
        assert sql_where.price == {"gt": Decimal(50)}
        assert sql_where.is_active == {"eq": True}

    def test_in_operator_field_mapping(self) -> None:
        """Test that 'in_' field is properly mapped to 'in' operator."""
        ProductWhereInput = create_graphql_where_input(Product)

        where_input = ProductWhereInput(
            id=UUIDFilter(in_=[uuid.uuid4(), uuid.uuid4()]),
            name=StringFilter(in_=["Product1", "Product2"]),
        )

        # Convert to SQL where
        sql_where = where_input._to_sql_where()

        # Check 'in_' was mapped to 'in'
        assert "in" in sql_where.id
        assert sql_where.id["in"] == where_input.id.in_
        assert "in" in sql_where.name
        assert sql_where.name["in"] == where_input.name.in_

    def test_empty_filter_handling(self) -> None:
        """Test that empty filters are handled correctly."""
        ProductWhereInput = create_graphql_where_input(Product)

        # Create with empty filters
        where_input = ProductWhereInput(
            name=StringFilter(),  # Empty filter
            price=None,  # No filter
        )

        # Convert to SQL where
        sql_where = where_input._to_sql_where()

        # Empty filter should not create any operators
        assert sql_where.name is None  # Empty filter converted to None
        assert sql_where.price == {}  # No filter provided, uses default empty dict

    def test_all_field_types(self) -> None:
        """Test that all field types get correct filter types."""

        @dataclass
        class AllTypesModel:
            string_field: str
            int_field: int
            float_field: float
            decimal_field: Decimal
            bool_field: bool
            uuid_field: uuid.UUID
            date_field: date
            datetime_field: datetime

        AllTypesWhereInput = create_graphql_where_input(AllTypesModel)
        where_input = AllTypesWhereInput()

        # Set filters for each type
        where_input.string_field = StringFilter(eq="test")
        where_input.int_field = IntFilter(gt=5)
        where_input.float_field = FloatFilter(lte=10.5)
        where_input.decimal_field = DecimalFilter(gte=Decimal(100))
        where_input.bool_field = BooleanFilter(eq=True)
        where_input.uuid_field = UUIDFilter(eq=uuid.uuid4())
        where_input.date_field = DateFilter(gt=date.today())
        where_input.datetime_field = DateTimeFilter(lt=datetime.now(UTC))

        # Verify all fields work correctly
        assert where_input.string_field.eq == "test"
        assert where_input.int_field.gt == 5
        assert where_input.float_field.lte == 10.5
        assert where_input.decimal_field.gte == Decimal(100)
        assert where_input.bool_field.eq is True
        assert isinstance(where_input.uuid_field.eq, uuid.UUID)
        assert isinstance(where_input.date_field.gt, date)
        assert isinstance(where_input.datetime_field.lt, datetime)


class TestGraphQLWhereIntegration:
    """Test integration with SQL where generation."""

    def test_sql_where_generation(self) -> None:
        """Test that converted where types generate valid SQL."""
        ProductWhereInput = create_graphql_where_input(Product)

        where_input = ProductWhereInput(
            name=StringFilter(contains="Widget", isnull=False),
            price=DecimalFilter(gte=Decimal(10), lt=Decimal(100)),
            is_active=BooleanFilter(eq=True),
        )

        # Convert to SQL where
        sql_where = where_input._to_sql_where()

        # Generate SQL
        sql = sql_where.to_sql()

        # Check SQL was generated
        assert sql is not None

    def test_complex_filter_combinations(self) -> None:
        """Test complex filter combinations."""
        ProductWhereInput = create_graphql_where_input(Product)

        where_input = ProductWhereInput(
            # Multiple operators on same field
            price=DecimalFilter(gte=Decimal(10), lte=Decimal(100), neq=Decimal(50)),
            # String operations
            name=StringFilter(startswith="Pro", contains="duct", isnull=False),
            # Array operations
            id=UUIDFilter(in_=[uuid.uuid4() for _ in range(3)], neq=uuid.uuid4()),
        )

        sql_where = where_input._to_sql_where()

        # Verify all operators were preserved
        assert len(sql_where.price) == 3  # gte, lte, neq
        assert len(sql_where.name) == 3  # startswith, contains, isnull
        assert len(sql_where.id) == 2  # in, neq

    def test_none_values_ignored(self) -> None:
        """Test that None values in filters are ignored."""
        ProductWhereInput = create_graphql_where_input(Product)

        where_input = ProductWhereInput(
            name=StringFilter(eq="test", contains=None),  # contains should be ignored
            price=DecimalFilter(gt=None, lte=Decimal(100)),  # gt should be ignored
        )

        sql_where = where_input._to_sql_where()

        # Only non-None values should be in the operator dict
        assert sql_where.name == {"eq": "test"}
        assert sql_where.price == {"lte": Decimal(100)}


class TestEdgeCases:
    """Test edge cases and error handling."""

    def test_class_without_annotations(self) -> None:
        """Test handling of class without proper annotations."""

        class NoAnnotations:
            pass

        # Should still create a where input (empty)
        WhereInput = create_graphql_where_input(NoAnnotations)
        assert WhereInput.__name__ == "NoAnnotationsWhereInput"

    def test_private_fields_ignored(self) -> None:
        """Test that private fields are ignored."""

        @dataclass
        class ModelWithPrivate:
            id: int
            name: str
            _private: str = "ignored"
            __very_private: int = 42

        WhereInput = create_graphql_where_input(ModelWithPrivate)

        # Only public fields should be included
        assert "id" in WhereInput.__dataclass_fields__
        assert "name" in WhereInput.__dataclass_fields__
        assert "_private" not in WhereInput.__dataclass_fields__
        assert "__very_private" not in WhereInput.__dataclass_fields__
