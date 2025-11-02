# Extracted from: docs/reference/decorators.md
# Block number: 14
from fraiseql.mutations.decorators import failure, result, success


@success
class CreateUserSuccess:
    user: User
    message: str


@failure
class CreateUserError:
    code: str
    message: str
    field: str | None = None


@result
class CreateUserResult:
    success: CreateUserSuccess | None = None
    error: CreateUserError | None = None
