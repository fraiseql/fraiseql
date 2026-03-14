"""Unit tests for mutation return type validation."""

import pytest
from graphql import build_schema

from fraiseql.mutations.validation import validate_mutation_return

pytestmark = pytest.mark.unit

SIMPLE_SCHEMA = build_schema("""
    type Query {
        _dummy: String
    }

    type Mutation {
        createUser(name: String!, email: String!): CreateUserResult!
    }

    type CreateUserSuccess {
        status: String!
        message: String
        id: String
    }

    type CreateUserError {
        status: String!
        message: String
        code: Int!
        errors: [ErrorDetail]
    }

    type ErrorDetail {
        field: String!
        message: String!
    }

    union CreateUserResult = CreateUserSuccess | CreateUserError
""")

NESTED_SCHEMA = build_schema("""
    type Query {
        _dummy: String
    }

    type Mutation {
        createOrder(item: String!): OrderResponse!
    }

    type OrderResponse {
        status: String!
        order: Order
    }

    type Order {
        id: ID!
        item: String!
        address: Address!
    }

    type Address {
        city: String!
        zip: String!
    }
""")

NON_UNION_SCHEMA = build_schema("""
    type Query {
        _dummy: String
    }

    type Mutation {
        updateName(name: String!): UpdateResult!
    }

    type UpdateResult {
        status: String!
        message: String
        updatedFields: [String!]
    }
""")


class TestBasicValidation:
    """Tests for basic validation scenarios."""

    def test_valid_success_response(self) -> None:
        result = validate_mutation_return(
            SIMPLE_SCHEMA,
            "createUser",
            {"status": "success", "message": "User created", "id": "123"},
        )
        assert result.is_valid

    def test_valid_error_response(self) -> None:
        result = validate_mutation_return(
            SIMPLE_SCHEMA,
            "createUser",
            {"status": "error", "message": "Duplicate email", "code": 409, "errors": []},
        )
        assert result.is_valid

    def test_mutation_not_found(self) -> None:
        result = validate_mutation_return(SIMPLE_SCHEMA, "nonExistent", {})
        assert not result.is_valid
        assert "not found" in result.errors[0].message

    def test_no_mutation_type(self) -> None:
        schema = build_schema("type Query { _dummy: String }")
        result = validate_mutation_return(schema, "anything", {})
        assert not result.is_valid
        assert "no Mutation type" in result.errors[0].message


class TestRequiredFields:
    """Tests for required (non-null) field validation."""

    def test_missing_required_field(self) -> None:
        result = validate_mutation_return(
            SIMPLE_SCHEMA,
            "createUser",
            {"message": "oops"},
        )
        assert not result.is_valid
        # Must report missing 'status' (required in both union members)
        error_paths = {e.field_path for e in result.errors}
        assert any("status" in p for p in error_paths)

    def test_null_required_field(self) -> None:
        result = validate_mutation_return(
            NON_UNION_SCHEMA,
            "updateName",
            {"status": None},
        )
        assert not result.is_valid
        assert any("null" in e.message.lower() for e in result.errors)

    def test_missing_nullable_field_ok(self) -> None:
        result = validate_mutation_return(
            NON_UNION_SCHEMA,
            "updateName",
            {"status": "success"},
        )
        assert result.is_valid


class TestTypeChecking:
    """Tests for field type validation."""

    def test_wrong_type_string_vs_int(self) -> None:
        result = validate_mutation_return(
            NON_UNION_SCHEMA,
            "updateName",
            {"status": 42},
        )
        assert not result.is_valid
        assert any("String" in e.expected_type for e in result.errors)

    def test_wrong_type_int_vs_string(self) -> None:
        # Use non-union schema where code type mismatch is unambiguous
        result = validate_mutation_return(
            NON_UNION_SCHEMA,
            "updateName",
            {"status": 123},
        )
        assert not result.is_valid
        assert any("String" in e.expected_type for e in result.errors)


class TestUnionTypes:
    """Tests for union type validation."""

    def test_matches_success_member(self) -> None:
        result = validate_mutation_return(
            SIMPLE_SCHEMA,
            "createUser",
            {"status": "success", "id": "42"},
        )
        assert result.is_valid
        assert result.matched_type == "CreateUserSuccess"

    def test_matches_error_member_with_typename(self) -> None:
        result = validate_mutation_return(
            SIMPLE_SCHEMA,
            "createUser",
            {
                "__typename": "CreateUserError",
                "status": "error",
                "code": 422,
                "errors": [],
            },
        )
        assert result.is_valid
        assert result.matched_type == "CreateUserError"

    def test_matches_first_valid_member_without_typename(self) -> None:
        """Without __typename, union validation matches the first valid member."""
        result = validate_mutation_return(
            SIMPLE_SCHEMA,
            "createUser",
            {"status": "error", "code": 422, "errors": []},
        )
        assert result.is_valid

    def test_typename_disambiguation(self) -> None:
        result = validate_mutation_return(
            SIMPLE_SCHEMA,
            "createUser",
            {"__typename": "CreateUserSuccess", "status": "success"},
        )
        assert result.is_valid
        assert result.matched_type == "CreateUserSuccess"

    def test_invalid_typename(self) -> None:
        result = validate_mutation_return(
            SIMPLE_SCHEMA,
            "createUser",
            {"__typename": "Bogus", "status": "ok"},
        )
        assert not result.is_valid
        assert "Bogus" in result.errors[0].message


class TestNestedObjects:
    """Tests for recursive nested object validation."""

    def test_valid_nested(self) -> None:
        result = validate_mutation_return(
            NESTED_SCHEMA,
            "createOrder",
            {
                "status": "success",
                "order": {
                    "id": "1",
                    "item": "Widget",
                    "address": {"city": "Paris", "zip": "75001"},
                },
            },
        )
        assert result.is_valid

    def test_missing_nested_required(self) -> None:
        result = validate_mutation_return(
            NESTED_SCHEMA,
            "createOrder",
            {
                "status": "success",
                "order": {
                    "id": "1",
                    "item": "Widget",
                    "address": {"city": "Paris"},
                },
            },
        )
        assert not result.is_valid
        error_paths = {e.field_path for e in result.errors}
        assert "order.address.zip" in error_paths

    def test_null_nested_nullable(self) -> None:
        result = validate_mutation_return(
            NESTED_SCHEMA,
            "createOrder",
            {"status": "success", "order": None},
        )
        assert result.is_valid


class TestListValidation:
    """Tests for list field validation."""

    def test_valid_list(self) -> None:
        result = validate_mutation_return(
            NON_UNION_SCHEMA,
            "updateName",
            {"status": "ok", "updatedFields": ["name", "email"]},
        )
        assert result.is_valid

    def test_null_item_in_non_null_list(self) -> None:
        result = validate_mutation_return(
            NON_UNION_SCHEMA,
            "updateName",
            {"status": "ok", "updatedFields": ["name", None]},
        )
        assert not result.is_valid

    def test_wrong_item_type(self) -> None:
        result = validate_mutation_return(
            NON_UNION_SCHEMA,
            "updateName",
            {"status": "ok", "updatedFields": [1, 2]},
        )
        assert not result.is_valid

    def test_not_a_list(self) -> None:
        result = validate_mutation_return(
            NON_UNION_SCHEMA,
            "updateName",
            {"status": "ok", "updatedFields": "name"},
        )
        assert not result.is_valid
        assert any("list" in e.message.lower() for e in result.errors)


class TestEnumValidation:
    """Tests for enum type validation."""

    def test_valid_enum(self) -> None:
        schema = build_schema("""
            type Query { _d: String }
            type Mutation { setStatus(s: String!): StatusResult! }
            type StatusResult { status: Status! }
            enum Status { ACTIVE INACTIVE PENDING }
        """)
        result = validate_mutation_return(schema, "setStatus", {"status": "ACTIVE"})
        assert result.is_valid

    def test_invalid_enum(self) -> None:
        schema = build_schema("""
            type Query { _d: String }
            type Mutation { setStatus(s: String!): StatusResult! }
            type StatusResult { status: Status! }
            enum Status { ACTIVE INACTIVE PENDING }
        """)
        result = validate_mutation_return(schema, "setStatus", {"status": "UNKNOWN"})
        assert not result.is_valid
        assert "UNKNOWN" in result.errors[0].message
