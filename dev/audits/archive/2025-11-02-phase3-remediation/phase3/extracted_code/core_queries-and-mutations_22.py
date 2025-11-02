# Extracted from: docs/core/queries-and-mutations.md
# Block number: 22
from fraiseql import mutation


@mutation(function="register_new_user", schema="auth")
class RegisterUser:
    input: RegistrationInput
    success: RegistrationSuccess
    failure: RegistrationError


# Calls: auth.register_new_user(input) instead of default name
