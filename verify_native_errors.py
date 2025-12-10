#!/usr/bin/env python3
"""Verification script for WP-034: Native Error Arrays

Tests that error arrays are automatically populated on all mutation error responses.
"""

import json

from fraiseql_rs import build_mutation_response


def test_auto_generated_errors() -> None:
    """Test that errors array is auto-generated from status string."""
    print("\n" + "=" * 70)
    print("TEST 1: Auto-generated errors from status string")
    print("=" * 70)

    mutation_json = json.dumps(
        {
            "status": "failed:validation",
            "message": "Validation failed",
            "entity": None,
            "entity_type": "User",
            "entity_id": None,
            "updated_fields": None,
            "cascade": None,
            "metadata": None,
        }
    )

    result = build_mutation_response(
        mutation_json=mutation_json,
        field_name="createUser",
        success_type="CreateUserSuccess",
        error_type="CreateUserError",
        entity_field_name="user",
        entity_type="User",
        cascade_selections=None,
        auto_camel_case=False,
        success_type_fields=None,
    )

    response = json.loads(result)
    print(f"\nResponse: {json.dumps(response, indent=2)}")

    data = response["data"]["createUser"]

    # Verify error response structure
    assert data["__typename"] == "CreateUserError", "Wrong typename"
    assert data["code"] == 400, f"Expected code 400, got {data['code']}"
    assert data["status"] == "failed:validation", (
        f"Expected status 'failed:validation', got {data['status']}"
    )
    assert data["message"] == "Validation failed", (
        f"Expected message 'Validation failed', got {data['message']}"
    )

    # ✅ KEY ASSERTION - errors array should exist and be populated
    assert "errors" in data, "❌ FAIL: errors field missing from response"
    assert data["errors"] is not None, "❌ FAIL: errors field is None"
    assert len(data["errors"]) == 1, f"❌ FAIL: Expected 1 error, got {len(data['errors'])}"

    error = data["errors"][0]
    assert error["code"] == 400, f"Expected error code 400, got {error['code']}"
    assert error["identifier"] == "validation", (
        f"Expected identifier 'validation', got {error['identifier']}"
    )
    assert error["message"] == "Validation failed", (
        f"Expected message 'Validation failed', got {error['message']}"
    )
    assert error["details"] is None, f"Expected details None, got {error['details']}"

    print("✅ PASS: Errors array auto-generated correctly!")


def test_explicit_errors_override() -> None:
    """Test that explicit errors in metadata.errors override auto-generation."""
    print("\n" + "=" * 70)
    print("TEST 2: Explicit errors override auto-generation")
    print("=" * 70)

    mutation_json = json.dumps(
        {
            "status": "failed:validation",
            "message": "Multiple validation errors",
            "entity": None,
            "entity_type": "User",
            "entity_id": None,
            "updated_fields": None,
            "cascade": None,
            "metadata": {
                "errors": [
                    {
                        "code": 400,
                        "identifier": "email_invalid",
                        "message": "Email format is invalid",
                        "details": {"field": "email"},
                    },
                    {
                        "code": 400,
                        "identifier": "password_weak",
                        "message": "Password must be at least 8 characters",
                        "details": {"field": "password"},
                    },
                ]
            },
        }
    )

    result = build_mutation_response(
        mutation_json=mutation_json,
        field_name="createUser",
        success_type="CreateUserSuccess",
        error_type="CreateUserError",
        entity_field_name="user",
        entity_type="User",
        cascade_selections=None,
        auto_camel_case=False,
        success_type_fields=None,
    )

    response = json.loads(result)
    print(f"\nResponse: {json.dumps(response, indent=2)}")

    data = response["data"]["createUser"]

    # Should have 2 explicit errors, NOT auto-generated single error
    assert "errors" in data, "❌ FAIL: errors field missing"
    assert len(data["errors"]) == 2, (
        f"❌ FAIL: Expected 2 explicit errors, got {len(data['errors'])}"
    )

    # Verify first error
    assert data["errors"][0]["identifier"] == "email_invalid", (
        f"Expected first error identifier 'email_invalid', got {data['errors'][0]['identifier']}"
    )
    assert data["errors"][0]["details"]["field"] == "email", (
        f"Expected first error field 'email', got {data['errors'][0]['details']['field']}"
    )

    # Verify second error
    assert data["errors"][1]["identifier"] == "password_weak", (
        f"Expected second error identifier 'password_weak', got {data['errors'][1]['identifier']}"
    )
    assert data["errors"][1]["details"]["field"] == "password", (
        f"Expected second error field 'password', got {data['errors'][1]['details']['field']}"
    )

    print("✅ PASS: Explicit errors override working correctly!")


def test_noop_status() -> None:
    """Test error generation from noop status (e.g., not_found)."""
    print("\n" + "=" * 70)
    print("TEST 3: Noop status generates errors")
    print("=" * 70)

    mutation_json = json.dumps(
        {
            "status": "noop:not_found",
            "message": "User not found",
            "entity": None,
            "entity_type": "User",
            "entity_id": None,
            "updated_fields": None,
            "cascade": None,
            "metadata": None,
        }
    )

    result = build_mutation_response(
        mutation_json=mutation_json,
        field_name="updateUser",
        success_type="UpdateUserSuccess",
        error_type="UpdateUserError",
        entity_field_name="user",
        entity_type="User",
        cascade_selections=None,
        auto_camel_case=False,
        success_type_fields=None,
    )

    response = json.loads(result)
    print(f"\nResponse: {json.dumps(response, indent=2)}")

    data = response["data"]["updateUser"]

    # Verify noop treated as error with 404 code
    assert data["code"] == 404, f"Expected code 404 for not_found, got {data['code']}"
    assert "errors" in data, "❌ FAIL: errors field missing"
    assert len(data["errors"]) == 1, f"Expected 1 error, got {len(data['errors'])}"

    error = data["errors"][0]
    assert error["code"] == 404, f"Expected error code 404, got {error['code']}"
    assert error["identifier"] == "not_found", (
        f"Expected identifier 'not_found', got {error['identifier']}"
    )
    assert error["message"] == "User not found", (
        f"Expected message 'User not found', got {error['message']}"
    )

    print("✅ PASS: Noop status generates errors correctly!")


def test_multiple_status_formats() -> None:
    """Test identifier extraction from various status formats."""
    print("\n" + "=" * 70)
    print("TEST 4: Multiple status string formats")
    print("=" * 70)

    test_cases = [
        ("failed:validation", "validation", 400),
        ("noop:not_found", "not_found", 404),
        ("failed:authorization", "authorization", 403),
        ("failed", "general_error", 500),
    ]

    for status, expected_id, expected_code in test_cases:
        mutation_json = json.dumps(
            {
                "status": status,
                "message": "Test message",
                "entity": None,
                "entity_type": "Test",
                "entity_id": None,
                "updated_fields": None,
                "cascade": None,
                "metadata": None,
            }
        )

        result = build_mutation_response(
            mutation_json=mutation_json,
            field_name="testOp",
            success_type="TestSuccess",
            error_type="TestError",
            entity_field_name=None,
            entity_type="Test",
            cascade_selections=None,
            auto_camel_case=False,
            success_type_fields=None,
        )

        response = json.loads(result)
        data = response["data"]["testOp"]

        assert data["errors"][0]["identifier"] == expected_id, (
            f"For status '{status}', expected identifier '{expected_id}', got '{data['errors'][0]['identifier']}'"
        )
        assert data["code"] == expected_code, (
            f"For status '{status}', expected code {expected_code}, got {data['code']}"
        )

        print(f"  ✓ Status '{status}' → identifier '{expected_id}', code {expected_code}")

    print("✅ PASS: All status formats handled correctly!")


def main() -> int:
    """Run all verification tests."""
    print("\n" + "=" * 70)
    print("WP-034: Native Error Arrays Verification")
    print("=" * 70)

    try:
        test_auto_generated_errors()
        test_explicit_errors_override()
        test_noop_status()
        test_multiple_status_formats()

        print("\n" + "=" * 70)
        print("✅ ALL TESTS PASSED!")
        print("=" * 70)
        print("\nNative error arrays feature is working correctly:")
        print("  ✓ Errors array auto-populates from status strings")
        print("  ✓ Explicit metadata.errors override works")
        print("  ✓ Noop statuses generate errors")
        print("  ✓ Multiple status formats handled")
        print("\nWP-034 implementation verified! 🎉")

    except AssertionError as e:
        print(f"\n❌ TEST FAILED: {e}")
        return 1
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        import traceback

        traceback.print_exc()
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
