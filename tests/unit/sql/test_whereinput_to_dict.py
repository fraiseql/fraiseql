"""Tests for WhereInput._to_whereinput_dict() conversion.

This module tests the `_to_whereinput_dict()` method that is dynamically
added to generated WhereInput classes. This method converts WhereInput
objects (with Filter objects) into plain dict structures suitable for
WHERE clause normalization.

Phase R3 of the industrial WHERE refactor.
"""

import uuid

from fraiseql.sql import (
    FloatFilter,
    IntFilter,
    StringFilter,
    UUIDFilter,
    create_graphql_where_input,
)
from fraiseql.sql.graphql_where_generator import ArrayFilter


class User:
    """Test user class."""

    id: uuid.UUID
    name: str
    age: int


class TestWhereInputToDict:
    """Test WhereInput to dict conversion."""

    def test_simple_filter_conversion(self):
        """Test simple filter converts to operator dict."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(name=StringFilter(eq="John"))

        result = where_input._to_whereinput_dict()

        assert result == {"name": {"eq": "John"}}

    def test_multiple_operators_on_same_field(self):
        """Test multiple operators on same field."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(age=IntFilter(gte=18, lte=65))

        result = where_input._to_whereinput_dict()

        assert result == {"age": {"gte": 18, "lte": 65}}

    def test_multiple_fields(self):
        """Test multiple fields with filters."""
        UserWhereInput = create_graphql_where_input(User)

        user_id = uuid.uuid4()
        where_input = UserWhereInput(id=UUIDFilter(eq=user_id), name=StringFilter(contains="john"))

        result = where_input._to_whereinput_dict()

        assert result == {"id": {"eq": user_id}, "name": {"contains": "john"}}

    def test_or_operator_conversion(self):
        """Test OR operator converts to list."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(
            OR=[
                UserWhereInput(name=StringFilter(eq="John")),
                UserWhereInput(name=StringFilter(eq="Jane")),
            ]
        )

        result = where_input._to_whereinput_dict()

        assert result == {"OR": [{"name": {"eq": "John"}}, {"name": {"eq": "Jane"}}]}

    def test_and_operator_conversion(self):
        """Test AND operator converts to list."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(
            AND=[
                UserWhereInput(age=IntFilter(gte=18)),
                UserWhereInput(age=IntFilter(lte=65)),
            ]
        )

        result = where_input._to_whereinput_dict()

        assert result == {"AND": [{"age": {"gte": 18}}, {"age": {"lte": 65}}]}

    def test_not_operator_conversion(self):
        """Test NOT operator converts correctly."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(
            name=StringFilter(eq="Active"), NOT=UserWhereInput(age=IntFilter(lt=18))
        )

        result = where_input._to_whereinput_dict()

        assert result == {"name": {"eq": "Active"}, "NOT": {"age": {"lt": 18}}}

    def test_nested_logical_operators(self):
        """Test nested OR within AND."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(
            AND=[
                UserWhereInput(age=IntFilter(gte=18)),
                UserWhereInput(
                    OR=[
                        UserWhereInput(name=StringFilter(eq="John")),
                        UserWhereInput(name=StringFilter(eq="Jane")),
                    ]
                ),
            ]
        )

        result = where_input._to_whereinput_dict()

        expected = {
            "AND": [
                {"age": {"gte": 18}},
                {"OR": [{"name": {"eq": "John"}}, {"name": {"eq": "Jane"}}]},
            ]
        }
        assert result == expected

    def test_none_values_ignored(self):
        """Test None values are not included in result."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(
            name=StringFilter(eq="John"),
            age=None,  # Should be ignored
        )

        result = where_input._to_whereinput_dict()

        assert result == {"name": {"eq": "John"}}
        assert "age" not in result

    def test_empty_filter_ignored(self):
        """Test filter with no operators set is ignored."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(name=StringFilter())  # No operators

        result = where_input._to_whereinput_dict()

        # Empty filter should not appear in result
        assert result == {} or "name" not in result

    def test_string_filter_operators(self):
        """Test all StringFilter operators."""
        UserWhereInput = create_graphql_where_input(User)

        # Test contains
        where = UserWhereInput(name=StringFilter(contains="John"))
        assert where._to_whereinput_dict() == {"name": {"contains": "John"}}

        # Test startswith
        where = UserWhereInput(name=StringFilter(startswith="J"))
        assert where._to_whereinput_dict() == {"name": {"startswith": "J"}}

        # Test endswith
        where = UserWhereInput(name=StringFilter(endswith="n"))
        assert where._to_whereinput_dict() == {"name": {"endswith": "n"}}

        # Test matches (regex)
        where = UserWhereInput(name=StringFilter(matches="^J.*n$"))
        assert where._to_whereinput_dict() == {"name": {"matches": "^J.*n$"}}

        # Test imatches (case-insensitive regex)
        where = UserWhereInput(name=StringFilter(imatches="^j.*n$"))
        assert where._to_whereinput_dict() == {"name": {"imatches": "^j.*n$"}}

        # Test not_matches
        where = UserWhereInput(name=StringFilter(not_matches="^Admin"))
        assert where._to_whereinput_dict() == {"name": {"not_matches": "^Admin"}}

    def test_int_filter_operators(self):
        """Test all IntFilter operators."""
        UserWhereInput = create_graphql_where_input(User)

        # Test gt
        where = UserWhereInput(age=IntFilter(gt=18))
        assert where._to_whereinput_dict() == {"age": {"gt": 18}}

        # Test gte
        where = UserWhereInput(age=IntFilter(gte=18))
        assert where._to_whereinput_dict() == {"age": {"gte": 18}}

        # Test lt
        where = UserWhereInput(age=IntFilter(lt=65))
        assert where._to_whereinput_dict() == {"age": {"lt": 65}}

        # Test lte
        where = UserWhereInput(age=IntFilter(lte=65))
        assert where._to_whereinput_dict() == {"age": {"lte": 65}}

    def test_in_operator_conversion(self):
        """Test IN operator for lists."""
        UserWhereInput = create_graphql_where_input(User)

        where = UserWhereInput(name=StringFilter(in_=["John", "Jane", "Jack"]))
        assert where._to_whereinput_dict() == {"name": {"in_": ["John", "Jane", "Jack"]}}

    def test_nin_operator_conversion(self):
        """Test NOT IN operator for lists."""
        UserWhereInput = create_graphql_where_input(User)

        where = UserWhereInput(age=IntFilter(nin=[10, 20, 30]))
        assert where._to_whereinput_dict() == {"age": {"nin": [10, 20, 30]}}

    def test_isnull_operator_conversion(self):
        """Test ISNULL operator."""
        UserWhereInput = create_graphql_where_input(User)

        # Test isnull=True
        where = UserWhereInput(name=StringFilter(isnull=True))
        assert where._to_whereinput_dict() == {"name": {"isnull": True}}

        # Test isnull=False
        where = UserWhereInput(name=StringFilter(isnull=False))
        assert where._to_whereinput_dict() == {"name": {"isnull": False}}

    def test_neq_operator_conversion(self):
        """Test not equal operator."""
        UserWhereInput = create_graphql_where_input(User)

        where = UserWhereInput(name=StringFilter(neq="Admin"))
        assert where._to_whereinput_dict() == {"name": {"neq": "Admin"}}


class TestNestedWhereInputToDict:
    """Test nested WhereInput conversion."""

    def test_nested_whereinput_conversion(self):
        """Test nested WhereInput objects."""

        class Machine:
            id: uuid.UUID
            name: str

        class Allocation:
            id: uuid.UUID
            machine: Machine | None
            status: str

        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        machine_id = uuid.uuid4()
        where = AllocationWhereInput(
            status=StringFilter(eq="active"),
            machine=MachineWhereInput(id=UUIDFilter(eq=machine_id)),
        )

        result = where._to_whereinput_dict()

        expected = {"status": {"eq": "active"}, "machine": {"id": {"eq": machine_id}}}
        assert result == expected

    def test_deeply_nested_whereinput(self):
        """Test multiple levels of nesting."""

        class Organization:
            id: uuid.UUID
            name: str

        class User:
            id: uuid.UUID
            name: str
            organization: Organization | None

        class Post:
            id: uuid.UUID
            title: str
            author: User | None

        OrgWhereInput = create_graphql_where_input(Organization)
        UserWhereInput = create_graphql_where_input(User)
        PostWhereInput = create_graphql_where_input(Post)

        org_id = uuid.uuid4()
        where = PostWhereInput(
            title=StringFilter(contains="GraphQL"),
            author=UserWhereInput(
                name=StringFilter(eq="Alice"),
                organization=OrgWhereInput(id=UUIDFilter(eq=org_id)),
            ),
        )

        result = where._to_whereinput_dict()

        expected = {
            "title": {"contains": "GraphQL"},
            "author": {
                "name": {"eq": "Alice"},
                "organization": {"id": {"eq": org_id}},
            },
        }
        assert result == expected

    def test_nested_with_logical_operators(self):
        """Test nested WhereInput with OR/AND operators."""

        class Machine:
            id: uuid.UUID
            name: str

        class Allocation:
            id: uuid.UUID
            machine: Machine | None
            status: str

        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        machine1_id = uuid.uuid4()
        machine2_id = uuid.uuid4()

        where = AllocationWhereInput(
            status=StringFilter(eq="active"),
            OR=[
                AllocationWhereInput(machine=MachineWhereInput(id=UUIDFilter(eq=machine1_id))),
                AllocationWhereInput(machine=MachineWhereInput(id=UUIDFilter(eq=machine2_id))),
            ],
        )

        result = where._to_whereinput_dict()

        expected = {
            "status": {"eq": "active"},
            "OR": [
                {"machine": {"id": {"eq": machine1_id}}},
                {"machine": {"id": {"eq": machine2_id}}},
            ],
        }
        assert result == expected


class TestArrayFilterConversion:
    """Test ArrayFilter conversion to dict."""

    def test_array_filter_basic_operators(self):
        """Test basic array filter operators."""

        class Document:
            id: uuid.UUID
            tags: list[str]

        DocumentWhereInput = create_graphql_where_input(Document)

        # Test contains (array contains value)
        where = DocumentWhereInput(tags=ArrayFilter(contains=["python", "graphql"]))
        assert where._to_whereinput_dict() == {"tags": {"contains": ["python", "graphql"]}}

        # Test overlaps
        where = DocumentWhereInput(tags=ArrayFilter(overlaps=["python", "rust"]))
        assert where._to_whereinput_dict() == {"tags": {"overlaps": ["python", "rust"]}}

        # Test contained_by
        where = DocumentWhereInput(tags=ArrayFilter(contained_by=["python", "rust", "go"]))
        assert where._to_whereinput_dict() == {"tags": {"contained_by": ["python", "rust", "go"]}}

    def test_array_filter_length_operators(self):
        """Test array length operators."""

        class Document:
            id: uuid.UUID
            tags: list[str]

        DocumentWhereInput = create_graphql_where_input(Document)

        # Test len_eq
        where = DocumentWhereInput(tags=ArrayFilter(len_eq=3))
        assert where._to_whereinput_dict() == {"tags": {"len_eq": 3}}

        # Test len_gt
        where = DocumentWhereInput(tags=ArrayFilter(len_gt=5))
        assert where._to_whereinput_dict() == {"tags": {"len_gt": 5}}

        # Test len_lte
        where = DocumentWhereInput(tags=ArrayFilter(len_lte=10))
        assert where._to_whereinput_dict() == {"tags": {"len_lte": 10}}


class TestFloatFilterConversion:
    """Test FloatFilter conversion to dict."""

    def test_float_filter_operators(self):
        """Test float filter operators."""

        class Product:
            id: uuid.UUID
            price: float

        ProductWhereInput = create_graphql_where_input(Product)

        # Test eq
        where = ProductWhereInput(price=FloatFilter(eq=99.99))
        assert where._to_whereinput_dict() == {"price": {"eq": 99.99}}

        # Test gte and lte (range)
        where = ProductWhereInput(price=FloatFilter(gte=10.0, lte=100.0))
        assert where._to_whereinput_dict() == {"price": {"gte": 10.0, "lte": 100.0}}


class TestEdgeCases:
    """Test edge cases and special scenarios."""

    def test_filter_with_all_none_operators_ignored(self):
        """Test that filter with all None values is not included."""
        UserWhereInput = create_graphql_where_input(User)

        # Create filter with all None (should be ignored)
        where = UserWhereInput(name=StringFilter(eq=None, contains=None, startswith=None))

        result = where._to_whereinput_dict()

        # Should be empty since all operators are None
        assert result == {}

    def test_mixed_none_and_valid_operators(self):
        """Test filter with some None and some valid operators."""
        UserWhereInput = create_graphql_where_input(User)

        where = UserWhereInput(name=StringFilter(eq="John", contains=None, startswith=None))

        result = where._to_whereinput_dict()

        # Should only include non-None operators
        assert result == {"name": {"eq": "John"}}

    def test_empty_or_list_ignored(self):
        """Test that empty OR list is handled correctly."""
        UserWhereInput = create_graphql_where_input(User)

        where = UserWhereInput(OR=[])

        result = where._to_whereinput_dict()

        # Empty OR should be included as empty list
        assert result == {"OR": []}

    def test_complex_real_world_scenario(self):
        """Test complex real-world scenario with multiple filters."""

        class Machine:
            id: uuid.UUID
            name: str
            status: str

        class Allocation:
            id: uuid.UUID
            machine: Machine | None
            status: str
            priority: int
            tags: list[str]

        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        machine_id = uuid.uuid4()
        where = AllocationWhereInput(
            status=StringFilter(eq="active"),
            priority=IntFilter(gte=5, lte=10),
            tags=ArrayFilter(contains=["urgent"]),
            machine=MachineWhereInput(
                id=UUIDFilter(eq=machine_id), status=StringFilter(neq="maintenance")
            ),
            NOT=AllocationWhereInput(tags=ArrayFilter(contains=["cancelled"])),
        )

        result = where._to_whereinput_dict()

        expected = {
            "status": {"eq": "active"},
            "priority": {"gte": 5, "lte": 10},
            "tags": {"contains": ["urgent"]},
            "machine": {"id": {"eq": machine_id}, "status": {"neq": "maintenance"}},
            "NOT": {"tags": {"contains": ["cancelled"]}},
        }
        assert result == expected
