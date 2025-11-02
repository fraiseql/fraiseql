# Extracted from: docs/development/style-guide.md
# Block number: 7
from fraiseql import mutation, query


# Queries: camelCase
@query
def getUserById(id: UUID) -> User:
    pass


# Mutations: camelCase
@mutation
def createUser(input: CreateUserInput) -> User:
    pass


# Fields: camelCase
class User:
    firstName: str  # not first_name
    lastName: str  # not last_name
