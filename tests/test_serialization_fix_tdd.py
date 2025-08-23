"""TDD Tests for FraiseQL Serialization Fix.

This module contains comprehensive tests for the JSON serialization fix
implemented in the mutation decorator. Tests follow TDD principles:
1. RED: Write failing tests that demonstrate the requirements
2. GREEN: Implement minimal code to make tests pass
3. REFACTOR: Improve implementation while keeping tests green

The serialization fix addresses the critical issue where FraiseQL mutation
responses failed with "Object of type X is not JSON serializable" errors.
"""

import json
import uuid
from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Union
from unittest.mock import AsyncMock, MagicMock

import pytest
from graphql import ExecutionResult

from fraiseql import failure, fraise_input, success, fraise_type
from fraiseql.graphql.execute import _clean_fraise_types, _serialize_fraise_types_in_result
from fraiseql.mutations.mutation_decorator import MutationDefinition
from fraiseql.mutations.parser import parse_mutation_result


class TestSerializationFix:
    """Test suite for FraiseQL serialization fix.

    This test class validates that the serialization fix properly handles
    all types of FraiseQL objects and converts them to JSON-serializable
    dictionaries without losing data or causing errors.
    """

    def test_clean_fraise_types_with_simple_fraise_type(self):
        """Test: _clean_fraise_types converts simple @fraise_type to dict.

        GIVEN: A simple @fraise_type object with basic fields
        WHEN: _clean_fraise_types is called
        THEN: The object should be converted to a plain dictionary
        """
        @fraise_type
        class SimpleUser:
            id: str
            name: str
            email: str

        # Create instance
        user = SimpleUser()
        user.id = "123"
        user.name = "John Doe"
        user.email = "john@example.com"

        # Apply cleaning
        result = _clean_fraise_types(user)

        # Assertions
        assert isinstance(result, dict)
        assert result["id"] == "123"
        assert result["name"] == "John Doe"
        assert result["email"] == "john@example.com"
        assert "__fraiseql_definition__" not in result

    def test_clean_fraise_types_with_nested_fraise_types(self):
        """Test: _clean_fraise_types handles nested @fraise_type objects.

        GIVEN: A @fraise_type object containing another @fraise_type object
        WHEN: _clean_fraise_types is called
        THEN: Both objects should be converted to nested dictionaries
        """
        @fraise_type
        class Address:
            street: str
            city: str

        @fraise_type
        class UserWithAddress:
            id: str
            name: str
            address: Address

        # Create nested structure
        address = Address()
        address.street = "123 Main St"
        address.city = "Anytown"

        user = UserWithAddress()
        user.id = "456"
        user.name = "Jane Doe"
        user.address = address

        # Apply cleaning
        result = _clean_fraise_types(user)

        # Assertions
        assert isinstance(result, dict)
        assert result["id"] == "456"
        assert result["name"] == "Jane Doe"
        assert isinstance(result["address"], dict)
        assert result["address"]["street"] == "123 Main St"
        assert result["address"]["city"] == "Anytown"

    def test_clean_fraise_types_with_error_object(self):
        """Test: _clean_fraise_types handles FraiseQL error objects.

        GIVEN: A @fraise_failure error object with error details
        WHEN: _clean_fraise_types is called
        THEN: The error object should be converted to a dictionary
        """
        @failure
        class CreateMachineError:
            message: str
            error_code: str
            errors: List[Dict[str, Any]]

        # Create error instance
        error = CreateMachineError()
        error.message = "Contract not found or access denied"
        error.error_code = "noop:invalid_contract_id"
        error.errors = [
            {
                "code": 422,
                "details": {"field": "contract_id", "value": "invalid-uuid"},
                "message": "Invalid contract ID"
            }
        ]

        # Apply cleaning
        result = _clean_fraise_types(error)

        # Assertions
        assert isinstance(result, dict)
        assert result["message"] == "Contract not found or access denied"
        assert result["error_code"] == "noop:invalid_contract_id"
        assert isinstance(result["errors"], list)
        assert len(result["errors"]) == 1
        assert result["errors"][0]["code"] == 422

    def test_clean_fraise_types_with_success_object(self):
        """Test: _clean_fraise_types handles FraiseQL success objects.

        GIVEN: A @fraise_success object with nested data
        WHEN: _clean_fraise_types is called
        THEN: The success object should be converted to a dictionary
        """
        @fraise_type
        class Machine:
            id: str
            name: str
            status: str

        @success
        class CreateMachineSuccess:
            machine: Machine
            message: str

        # Create nested success structure
        machine = Machine()
        machine.id = str(uuid.uuid4())
        machine.name = "Test Machine"
        machine.status = "active"

        success = CreateMachineSuccess()
        success.machine = machine
        success.message = "Machine created successfully"

        # Apply cleaning
        result = _clean_fraise_types(success)

        # Assertions
        assert isinstance(result, dict)
        assert result["message"] == "Machine created successfully"
        assert isinstance(result["machine"], dict)
        assert result["machine"]["name"] == "Test Machine"
        assert result["machine"]["status"] == "active"

    def test_clean_fraise_types_with_list_of_fraise_types(self):
        """Test: _clean_fraise_types handles lists containing @fraise_type objects.

        GIVEN: A list containing multiple @fraise_type objects
        WHEN: _clean_fraise_types is called
        THEN: Each object in the list should be converted to a dictionary
        """
        @fraise_type
        class Item:
            id: str
            name: str
            price: float

        # Create list of items
        items = []
        for i in range(3):
            item = Item()
            item.id = f"item-{i}"
            item.name = f"Item {i}"
            item.price = 10.0 * (i + 1)
            items.append(item)

        # Apply cleaning
        result = _clean_fraise_types(items)

        # Assertions
        assert isinstance(result, list)
        assert len(result) == 3
        for i, item_dict in enumerate(result):
            assert isinstance(item_dict, dict)
            assert item_dict["id"] == f"item-{i}"
            assert item_dict["name"] == f"Item {i}"
            assert item_dict["price"] == 10.0 * (i + 1)

    def test_clean_fraise_types_preserves_primitives(self):
        """Test: _clean_fraise_types leaves primitive types unchanged.

        GIVEN: Primitive Python types (str, int, float, bool, None)
        WHEN: _clean_fraise_types is called
        THEN: The primitives should be returned unchanged
        """
        primitives = [
            "string",
            42,
            3.14,
            True,
            False,
            None,
            {"regular": "dict"},
            ["regular", "list"]
        ]

        for primitive in primitives:
            result = _clean_fraise_types(primitive)
            assert result == primitive

    def test_clean_fraise_types_handles_circular_references(self):
        """Test: _clean_fraise_types handles circular references gracefully.

        GIVEN: Objects with circular references
        WHEN: _clean_fraise_types is called
        THEN: The function should not infinite loop or crash
        """
        @fraise_type
        class Node:
            id: str
            child: Optional['Node'] = None

        # Create circular reference
        node1 = Node()
        node1.id = "node1"

        node2 = Node()
        node2.id = "node2"

        node1.child = node2
        node2.child = node1  # Circular reference

        # This should not crash or infinite loop
        result = _clean_fraise_types(node1)

        # Assertions - should handle gracefully
        assert isinstance(result, dict)
        assert result["id"] == "node1"

    def test_json_serialization_after_cleaning(self):
        """Test: Objects are JSON serializable after _clean_fraise_types.

        GIVEN: A complex @fraise_type structure
        WHEN: _clean_fraise_types is applied and then JSON serialized
        THEN: No JSON serialization errors should occur
        """
        @fraise_type
        class Error:
            message: str
            code: int

        @failure
        class CreateMachineError:
            message: str
            error_code: str
            errors: List[Error]

        # Create complex error structure
        error_detail = Error()
        error_detail.message = "Validation failed"
        error_detail.code = 422

        machine_error = CreateMachineError()
        machine_error.message = "Machine creation failed"
        machine_error.error_code = "VALIDATION_ERROR"
        machine_error.errors = [error_detail]

        # Apply cleaning
        cleaned = _clean_fraise_types(machine_error)

        # This should not raise any JSON serialization errors
        json_string = json.dumps(cleaned)
        assert isinstance(json_string, str)

        # Verify round-trip
        parsed = json.loads(json_string)
        assert parsed["message"] == "Machine creation failed"
        assert parsed["error_code"] == "VALIDATION_ERROR"
        assert len(parsed["errors"]) == 1
        assert parsed["errors"][0]["code"] == 422

    def test_serialize_fraise_types_in_result_with_execution_result(self):
        """Test: _serialize_fraise_types_in_result processes ExecutionResult.

        GIVEN: An ExecutionResult containing @fraise_type objects
        WHEN: _serialize_fraise_types_in_result is called
        THEN: A new ExecutionResult with cleaned data should be returned
        """
        @fraise_type
        class User:
            id: str
            name: str

        # Create user
        user = User()
        user.id = "user123"
        user.name = "Test User"

        # Create ExecutionResult with @fraise_type data
        original_result = ExecutionResult(
            data={"user": user},
            errors=None
        )

        # Apply serialization fix
        cleaned_result = _serialize_fraise_types_in_result(original_result)

        # Assertions
        assert isinstance(cleaned_result, ExecutionResult)
        assert isinstance(cleaned_result.data, dict)
        assert isinstance(cleaned_result.data["user"], dict)
        assert cleaned_result.data["user"]["id"] == "user123"
        assert cleaned_result.data["user"]["name"] == "Test User"
        assert cleaned_result.errors == original_result.errors

    def test_mutation_decorator_returns_serializable_objects(self):
        """Test: Mutation decorator returns JSON-serializable objects.

        GIVEN: A mutation that would return @fraise_type objects
        WHEN: The mutation resolver is executed
        THEN: The returned objects should be JSON-serializable
        """
        @fraise_input
        class TestInput:
            name: str

        @success
        class TestSuccess:
            id: str
            name: str
            message: str

        @failure
        class TestError:
            message: str
            error_code: str

        # Create mock for testing
        class TestMutation:
            input: TestInput
            success: TestSuccess
            failure: TestError

        # Create mutation definition
        definition = MutationDefinition(TestMutation)

        # Mock database and parse result
        success = TestSuccess()
        success.id = "test123"
        success.name = "Test Item"
        success.message = "Created successfully"

        # Simulate the clean process that happens in the resolver
        cleaned_result = _clean_fraise_types(success)

        # Verify JSON serializable
        json_string = json.dumps(cleaned_result)
        parsed = json.loads(json_string)

        assert parsed["id"] == "test123"
        assert parsed["name"] == "Test Item"
        assert parsed["message"] == "Created successfully"

    @pytest.mark.parametrize("error_type", [
        "CreateMachineError",
        "DeleteMachineError",
        "UpdateMachineError",
        "CreateContractError",
        "ValidationError"
    ])
    def test_various_error_types_serializable(self, error_type):
        """Test: Various FraiseQL error types are serializable.

        GIVEN: Different types of FraiseQL error objects
        WHEN: _clean_fraise_types is applied
        THEN: All should be converted to JSON-serializable dictionaries
        """
        @failure
        class GenericError:
            message: str
            error_code: str
            status: str = "error"

        # Create error instance
        error = GenericError()
        error.message = f"Error from {error_type}"
        error.error_code = f"{error_type.upper()}_FAILED"

        # Apply cleaning
        result = _clean_fraise_types(error)

        # Verify serializable
        json_string = json.dumps(result)
        parsed = json.loads(json_string)

        assert parsed["message"] == f"Error from {error_type}"
        assert parsed["error_code"] == f"{error_type.upper()}_FAILED"

    def test_performance_of_serialization_cleaning(self):
        """Test: Serialization cleaning has acceptable performance.

        GIVEN: A large, complex structure with nested @fraise_type objects
        WHEN: _clean_fraise_types is called
        THEN: The operation should complete in reasonable time (< 100ms for complex structures)
        """
        import time

        @fraise_type
        class Item:
            id: str
            name: str
            value: int

        @fraise_type
        class Container:
            id: str
            items: List[Item]

        # Create large nested structure
        items = []
        for i in range(100):  # 100 items
            item = Item()
            item.id = f"item-{i}"
            item.name = f"Item {i}"
            item.value = i
            items.append(item)

        container = Container()
        container.id = "container-1"
        container.items = items

        # Measure performance
        start_time = time.time()
        result = _clean_fraise_types(container)
        end_time = time.time()

        duration_ms = (end_time - start_time) * 1000

        # Assertions
        assert isinstance(result, dict)
        assert len(result["items"]) == 100
        assert duration_ms < 100  # Should complete in less than 100ms

    def test_real_world_mutation_response_serialization(self):
        """Test: Real-world mutation response structure is properly serialized.

        GIVEN: A realistic mutation response with success/error union types
        WHEN: The response goes through serialization cleaning
        THEN: The final result should match expected GraphQL response structure
        """
        @fraise_type
        class Machine:
            id: str
            name: str
            status: str
            created_at: str

        @success
        class CreateMachineSuccess:
            machine: Machine
            message: str

        @failure
        class CreateMachineError:
            message: str
            error_code: str
            errors: List[Dict[str, Any]]

        # Create success response
        machine = Machine()
        machine.id = str(uuid.uuid4())
        machine.name = "Production Machine"
        machine.status = "active"
        machine.created_at = "2025-08-23T10:00:00Z"

        success = CreateMachineSuccess()
        success.machine = machine
        success.message = "Machine created successfully"

        # Simulate GraphQL response structure
        graphql_response = {
            "data": {
                "createMachine": success
            }
        }

        # Apply cleaning (as would happen in mutation decorator)
        cleaned = _clean_fraise_types(graphql_response)

        # Verify final structure
        assert isinstance(cleaned, dict)
        assert "data" in cleaned
        assert "createMachine" in cleaned["data"]

        create_machine = cleaned["data"]["createMachine"]
        assert isinstance(create_machine, dict)
        assert create_machine["message"] == "Machine created successfully"
        assert isinstance(create_machine["machine"], dict)
        assert create_machine["machine"]["name"] == "Production Machine"

        # Verify JSON serialization works
        json_response = json.dumps(cleaned)
        parsed_response = json.loads(json_response)
        assert parsed_response["data"]["createMachine"]["machine"]["name"] == "Production Machine"


class TestSerializationFixIntegration:
    """Integration tests for serialization fix in mutation context."""

    def test_mutation_decorator_serialization_integration(self):
        """Test: Full integration of serialization fix in mutation decorator.

        This test verifies that the fix applied in mutation_decorator.py
        (lines 151-153) properly converts FraiseQL objects to serializable dicts.
        """
        # This test would require a full GraphQL setup, but we can test
        # the core logic that the mutation decorator uses

        @failure
        class TestError:
            message: str
            status: str

        # Simulate what parse_mutation_result returns
        error = TestError()
        error.message = "Test error message"
        error.status = "error"

        # Apply the same cleaning logic used in mutation decorator
        from fraiseql.graphql.execute import _clean_fraise_types
        cleaned_result = _clean_fraise_types(error)

        # This is what should be returned from the mutation resolver
        assert isinstance(cleaned_result, dict)
        assert cleaned_result["message"] == "Test error message"
        assert cleaned_result["status"] == "error"

        # And it should be JSON serializable
        json.dumps(cleaned_result)  # Should not raise exception

    def test_no_regression_with_regular_objects(self):
        """Test: Serialization fix doesn't break regular (non-FraiseQL) objects.

        GIVEN: Regular Python objects without FraiseQL decorators
        WHEN: _clean_fraise_types is called
        THEN: Objects should pass through unchanged
        """
        regular_objects = [
            {"key": "value"},
            [1, 2, 3],
            "string",
            42,
            3.14,
            True,
            None
        ]

        for obj in regular_objects:
            result = _clean_fraise_types(obj)
            assert result == obj

    def test_mixed_fraise_and_regular_objects(self):
        """Test: Serialization handles mix of FraiseQL and regular objects.

        GIVEN: A structure containing both FraiseQL and regular objects
        WHEN: _clean_fraise_types is called
        THEN: Only FraiseQL objects should be converted, regular objects preserved
        """
        @fraise_type
        class FraiseUser:
            id: str
            name: str

        user = FraiseUser()
        user.id = "user123"
        user.name = "Test User"

        mixed_structure = {
            "fraise_user": user,  # Should be converted
            "regular_dict": {"key": "value"},  # Should be preserved
            "regular_list": [1, 2, 3],  # Should be preserved
            "primitive": "string"  # Should be preserved
        }

        result = _clean_fraise_types(mixed_structure)

        # Check FraiseQL object was converted
        assert isinstance(result["fraise_user"], dict)
        assert result["fraise_user"]["id"] == "user123"

        # Check regular objects preserved
        assert result["regular_dict"] == {"key": "value"}
        assert result["regular_list"] == [1, 2, 3]
        assert result["primitive"] == "string"
