# Extracted from: docs/core/queries-and-mutations.md
# Block number: 23
from fraiseql import mutation


@mutation(
    function="create_location",
    schema="app",
    context_params={"tenant_id": "input_pk_organization", "user": "input_created_by"},
)
class CreateLocation:
    input: CreateLocationInput
    success: CreateLocationSuccess
    failure: CreateLocationError


# Calls: app.create_location(tenant_id, user_id, input)
# Where tenant_id comes from info.context["tenant_id"]
# And user_id comes from info.context["user"].user_id
