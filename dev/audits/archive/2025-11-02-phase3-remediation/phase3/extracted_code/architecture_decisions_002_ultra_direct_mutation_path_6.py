# Extracted from: docs/architecture/decisions/002_ultra_direct_mutation_path.md
# Block number: 6
async def test_delete_customer_ultra_direct(db):
    """Test ultra-direct mutation path."""
    result = await db.execute_function_raw_json(
        "app.delete_customer", {"customer_id": "uuid-123"}, type_name="DeleteCustomerSuccess"
    )

    # Verify it's a RawJSONResult
    assert isinstance(result, RawJSONResult)

    # Verify transformation happened
    assert result._transformed is True

    # Parse JSON to verify structure
    data = json.loads(result.json_string)
    assert data["__typename"] == "DeleteCustomerSuccess"
    assert data["customer"]["__typename"] == "Customer"
    assert "firstName" in data["customer"]  # camelCase
    assert "first_name" not in data["customer"]  # no snake_case
