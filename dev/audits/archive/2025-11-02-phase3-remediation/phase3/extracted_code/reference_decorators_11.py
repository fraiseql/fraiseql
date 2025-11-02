# Extracted from: docs/reference/decorators.md
# Block number: 11
from fraiseql import mutation


# Function-based
@mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    return await db.create_one("v_user", data=input.__dict__)


# Class-based
@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError


# With custom function
@mutation(function="register_new_user", schema="auth")
class RegisterUser:
    input: RegistrationInput
    success: RegistrationSuccess
    failure: RegistrationError


# With context parameters - maps context to PostgreSQL function params
@mutation(
    function="create_location",
    context_params={"tenant_id": "input_pk_organization", "user_id": "input_created_by"},
)
class CreateLocation:
    input: CreateLocationInput
    success: CreateLocationSuccess
    failure: CreateLocationError
