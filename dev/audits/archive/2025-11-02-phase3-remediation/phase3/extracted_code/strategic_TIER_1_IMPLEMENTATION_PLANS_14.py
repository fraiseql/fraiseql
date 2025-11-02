# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 14
# tests/integration/enterprise/audit/test_interceptors.py


async def test_mutation_auto_logging():
    """Verify mutations are automatically logged to audit trail."""
    # Execute a mutation
    result = await execute_graphql(
        """
        mutation {
            createUser(input: {
                username: "testuser"
                email: "test@example.com"
            }) {
                user { id username }
            }
        }
    """,
        context={"user_id": "admin-123", "ip": "192.168.1.100"},
    )

    assert result["data"]["createUser"]["user"]["username"] == "testuser"

    # Check audit log
    events = await db_repo.run(
        DatabaseQuery(
            statement="SELECT * FROM audit_events WHERE event_type = 'mutation.createUser'",
            params={},
            fetch_result=True,
        )
    )

    assert len(events) == 1
    assert events[0]["event_data"]["input"]["username"] == "testuser"
    # Expected failure: interceptor not implemented
