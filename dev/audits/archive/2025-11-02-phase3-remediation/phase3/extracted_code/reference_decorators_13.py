# Extracted from: docs/reference/decorators.md
# Block number: 13
from fraiseql import mutation


# Context from JWT
async def get_context(request: Request) -> dict:
    token = extract_jwt(request)
    return {"tenant_id": token["tenant_id"], "user_id": token["user_id"]}


# Mutation with context injection
@mutation(
    function="create_order",
    context_params={"tenant_id": "input_tenant_id", "user_id": "input_created_by"},
)
class CreateOrder:
    input: CreateOrderInput
    success: CreateOrderSuccess
    failure: CreateOrderFailure


# PostgreSQL function
# CREATE FUNCTION create_order(
#     p_tenant_id uuid,      -- Automatically from context!
#     p_created_by uuid,     -- Automatically from context!
#     input jsonb
# ) RETURNS jsonb AS $$
# BEGIN
#     -- p_tenant_id and p_created_by are available
#     -- No need to extract from input JSONB
#     INSERT INTO tb_order (tenant_id, data)
#     VALUES (p_tenant_id, jsonb_set(input, '{created_by}', to_jsonb(p_created_by)));
# END;
# $$ LANGUAGE plpgsql;
