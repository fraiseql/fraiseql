# Extracted from: docs/core/concepts-glossary.md
# Block number: 9
import fraiseql


@fraiseql.mutation
class CreateUser:
    """Create a new user with explicit success/failure handling."""

    input: CreateUserInput
    success: CreateUserSuccess
    failure: ValidationError

    async def resolve(self, info):
        db = info.context["db"]
        # Call PostgreSQL function - all business logic in database
        result = await db.call_function("fn_create_user", self.input.name, self.input.email)

        # PostgreSQL function returns JSONB with success/error indicator
        if result["success"]:
            return CreateUserSuccess(
                user=User(**result["user"]), message=result.get("message", "User created")
            )
        return ValidationError(message=result["error"], code=result.get("code", "VALIDATION_ERROR"))
