# Extracted from: docs/development/style-guide.md
# Block number: 10
from fraiseql import query


# In documentation examples, show both the code and expected GraphQL usage
@query
def get_user(id: UUID) -> User:
    """Get user by ID."""


# GraphQL usage:
# query {
#   getUser(id: "123e4567-e89b-12d3-a456-426614174000") {
#     id
#     name
#     email
#   }
# }
