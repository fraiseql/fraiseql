# Extracted from: docs/advanced/llm-integration.md
# Block number: 5
FEW_SHOT_EXAMPLES = """
Example 1:
Request: "Get all users"
Query:
query {
  users {
    id
    name
    email
  }
}

Example 2:
Request: "Get user with ID 123 and their orders"
Query:
query {
  user(id: "123") {
    id
    name
    orders {
      id
      total
      status
    }
  }
}

Example 3:
Request: "Find orders created in the last week"
Query:
query {
  orders(
    filter: { createdAt: { gte: "2024-01-01" } }
    orderBy: { createdAt: DESC }
    limit: 100
  ) {
    id
    total
    status
    createdAt
  }
}

Now generate a query for:
Request: {user_request}
"""
