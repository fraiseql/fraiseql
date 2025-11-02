# Extracted from: docs/core/queries-and-mutations.md
# Block number: 21
from fraiseql import input, mutation, type


@input
class CreateUserInput:
    name: str
    email: str


@type
class CreateUserSuccess:
    user: User
    message: str


@type
class CreateUserError:
    code: str
    message: str
    field: str | None = None


@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError


# Automatically calls PostgreSQL function: public.create_user(input)
# and parses result into CreateUserSuccess or CreateUserError
