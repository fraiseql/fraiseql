# Extracted from: docs/core/rust-pipeline-integration.md
# Block number: 2
# In FraiseQLRepository.find():
async def find(self, source: str):
    # 1. Execute PostgreSQL query
    rows = await conn.fetch(f"SELECT data FROM {source}")

    # 2. Extract JSONB strings
    json_strings = [row["data"] for row in rows]

    # 3. Call Rust pipeline
    import fraiseql_rs

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=json_strings,
        field_name="users",
        type_name="User",
        field_paths=None,
    )

    # 4. Return RustResponseBytes (FastAPI sends as HTTP response)
    return RustResponseBytes(response_bytes)
