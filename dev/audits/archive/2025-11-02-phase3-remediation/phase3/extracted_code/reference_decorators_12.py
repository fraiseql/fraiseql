# Extracted from: docs/reference/decorators.md
# Block number: 12
from fraiseql import mutation


# GraphQL mutation
@mutation(
    function="create_location",
    context_params={
        "tenant_id": "input_pk_organization",  # info.context["tenant_id"] → p_pk_organization
        "user_id": "input_created_by",  # info.context["user_id"] → p_created_by
    },
)
class CreateLocation:
    input: CreateLocationInput
    success: CreateLocationSuccess
    failure: CreateLocationError


# PostgreSQL function signature
# CREATE FUNCTION create_location(
#     p_pk_organization uuid,   -- From info.context["tenant_id"]
#     p_created_by uuid,         -- From info.context["user_id"]
#     input jsonb                -- From mutation input
# ) RETURNS jsonb
