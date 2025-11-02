# Extracted from: docs/reference/cli.md
# Block number: 1
from fraiseql import input, mutation


@input
class CreateUserInput:
    name: str


@input
class UpdateUserInput:
    id: UUID
    name: str | None


@success
class UserSuccess:
    user: User
    message: str


@failure
class UserError:
    message: str
    code: str


@result
class UserResult:
    pass


@mutation
async def create_user(input: CreateUserInput, repository: CQRSRepository) -> UserResult:
    # TODO: Implement creation logic
    ...
