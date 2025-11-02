# Extracted from: docs/architecture/decisions/002_ultra_direct_mutation_path.md
# Block number: 7
async def test_mutation_e2e_ultra_direct(graphql_client):
    """Test complete mutation flow with ultra-direct path."""
    response = await graphql_client.execute(
        """
        mutation DeleteCustomer($id: UUID!) {
            deleteCustomer(input: {customerId: $id}) {
                __typename
                success
                customer {
                    __typename
                    id
                    email
                    firstName
                }
                affectedOrders {
                    __typename
                    id
                    status
                }
            }
        }
    """,
        {"id": "uuid-123"},
    )

    result = response["data"]["deleteCustomer"]

    # Verify GraphQL-native format
    assert result["__typename"] == "DeleteCustomerSuccess"
    assert result["customer"]["__typename"] == "Customer"
    assert result["customer"]["firstName"]  # camelCase

    # Verify affected orders
    for order in result["affectedOrders"]:
        assert order["__typename"] == "Order"
