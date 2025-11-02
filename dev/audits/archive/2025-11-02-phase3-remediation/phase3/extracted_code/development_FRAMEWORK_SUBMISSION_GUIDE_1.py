# Extracted from: docs/development/FRAMEWORK_SUBMISSION_GUIDE.md
# Block number: 1
# Example test (adapt to your framework)
def test_simple_users_query():
    query = """
    query {
        users(limit: 10) {
            id
            name
            email
        }
    }
    """
    response = execute_query(query)
    assert len(response["data"]["users"]) == 10
    assert all("id" in user for user in response["data"]["users"])
