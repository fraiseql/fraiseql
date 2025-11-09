"""Test FraiseQLJSONEncoder integration with Error __json__ method.

This module tests that the FraiseQLJSONEncoder properly uses the Error.__json__()
method for serialization, which is essential for GraphQL response serialization.
"""

import json

from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder
from fraiseql.types.errors import Error


class TestFraiseQLJSONEncoderIntegration:
    """Test FraiseQLJSONEncoder properly handles Error objects with __json__ method."""

    def test_fraiseql_json_encoder_uses_error_json_method(self) -> None:
        """Test that FraiseQLJSONEncoder uses Error.__json__() method for serialization."""
        error = Error(
            message="Test encoder integration",
            code=400,
            identifier="encoder_test",
            details={"source": "json_encoder", "test": True},
        )

        # Create encoder instance
        encoder = FraiseQLJSONEncoder()

        # Test that the encoder's default method uses our __json__ method
        result = encoder.default(error)

        expected = {
            "message": "Test encoder integration",
            "code": 400,
            "identifier": "encoder_test",
            "details": {"source": "json_encoder", "test": True},
        }

        assert result == expected
        assert isinstance(result, dict)

    def test_fraiseql_json_encoder_full_serialization(self) -> None:
        """Test full JSON serialization using FraiseQLJSONEncoder with Error objects."""
        error = Error(
            message="Full serialization test",
            code=422,
            identifier="full_test",
            details={"nested": {"key": "value"}, "array": [1, 2, 3]},
        )

        # Test full serialization path
        json_string = json.dumps(error, cls=FraiseQLJSONEncoder)

        # Parse back and verify
        parsed = json.loads(json_string)

        assert parsed["message"] == "Full serialization test"
        assert parsed["code"] == 422
        assert parsed["identifier"] == "full_test"
        assert parsed["details"]["nested"]["key"] == "value"
        assert parsed["details"]["array"] == [1, 2, 3]

    def test_fraiseql_json_encoder_error_list_serialization(self) -> None:
        """Test serialization of lists containing Error objects."""
        errors = [
            Error(message="Error 1", code=400, identifier="error_1"),
            Error(message="Error 2", code=500, identifier="error_2", details={"info": "test"}),
        ]

        # Test serialization of list with Error objects
        json_string = json.dumps(errors, cls=FraiseQLJSONEncoder)
        parsed = json.loads(json_string)

        assert len(parsed) == 2
        assert parsed[0]["message"] == "Error 1"
        assert parsed[0]["code"] == 400
        assert parsed[1]["details"]["info"] == "test"

    def test_fraiseql_json_encoder_complex_graphql_response(self) -> None:
        """Test serialization of complex GraphQL-style response with Error objects."""
        # Simulate a GraphQL mutation error response
        error_response = {
            "data": {
                "createUser": {
                    "__typename": "CreateUserError",
                    "message": "Validation failed",
                    "errors": [
                        Error(
                            message="Email already exists",
                            code=409,
                            identifier="duplicate_email",
                            details={"field": "email", "value": "test@example.com"},
                        ),
                        Error(
                            message="Password too weak",
                            code=400,
                            identifier="weak_password",
                            details={"requirements": ["uppercase", "numbers", "symbols"]},
                        ),
                    ],
                }
            }
        }

        # This should now work with our fix
        json_string = json.dumps(error_response, cls=FraiseQLJSONEncoder)
        parsed = json.loads(json_string)

        # Verify the structure
        create_user_data = parsed["data"]["createUser"]
        assert create_user_data["__typename"] == "CreateUserError"
        assert create_user_data["message"] == "Validation failed"
        assert len(create_user_data["errors"]) == 2

        # Verify first error
        error1 = create_user_data["errors"][0]
        assert error1["message"] == "Email already exists"
        assert error1["code"] == 409
        assert error1["identifier"] == "duplicate_email"
        assert error1["details"]["field"] == "email"

        # Verify second error
        error2 = create_user_data["errors"][1]
        assert error2["message"] == "Password too weak"
        assert error2["details"]["requirements"] == ["uppercase", "numbers", "symbols"]

    def test_fraiseql_json_encoder_error_none_details(self) -> None:
        """Test Error serialization when details is None."""
        error = Error(
            message="No details error",
            code=500,
            identifier="no_details",
            # details defaults to None
        )

        result = json.dumps(error, cls=FraiseQLJSONEncoder)
        parsed = json.loads(result)

        assert parsed["message"] == "No details error"
        assert parsed["code"] == 500
        assert parsed["identifier"] == "no_details"
        assert parsed["details"] is None

    def test_fraiseql_json_encoder_respects_error_json_method(self) -> None:
        """Test that encoder specifically calls Error.__json__() not generic attribute extraction."""
        error = Error(
            message="Method validation",
            code=400,
            identifier="method_test",
            details={"check": "specific"},
        )

        # Mock the __json__ method to verify it's being called
        original_json = error.__json__
        call_count = 0

        def mock_json() -> None:
            nonlocal call_count
            call_count += 1
            return original_json()

        error.__json__ = mock_json

        # Serialize using FraiseQLJSONEncoder
        json.dumps(error, cls=FraiseQLJSONEncoder)

        # Verify __json__ method was called
        assert call_count == 1
