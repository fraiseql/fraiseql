# Extracted from: docs/advanced/llm-integration.md
# Block number: 1
from graphql import get_introspection_query, graphql_sync

from fraiseql import query


@query
async def get_schema_for_llm(info) -> dict:
    """Get GraphQL schema formatted for LLM context."""
    schema = info.schema

    # Get full introspection
    introspection_query = get_introspection_query()
    result = graphql_sync(schema, introspection_query)

    # Simplify for LLM
    simplified = {"types": [], "queries": [], "mutations": []}

    for type_def in result.data["__schema"]["types"]:
        if type_def["name"].startswith("__"):
            continue  # Skip internal types

        simplified_type = {
            "name": type_def["name"],
            "kind": type_def["kind"],
            "description": type_def.get("description"),
            "fields": [],
        }

        if type_def.get("fields"):
            for field in type_def["fields"]:
                simplified_type["fields"].append(
                    {
                        "name": field["name"],
                        "type": _format_type(field["type"]),
                        "description": field.get("description"),
                        "args": [
                            {
                                "name": arg["name"],
                                "type": _format_type(arg["type"]),
                                "description": arg.get("description"),
                            }
                            for arg in field.get("args", [])
                        ],
                    }
                )

        simplified["types"].append(simplified_type)

    return simplified


def _format_type(type_ref: dict) -> str:
    """Format GraphQL type for LLM readability."""
    if type_ref["kind"] == "NON_NULL":
        return f"{_format_type(type_ref['ofType'])}!"
    if type_ref["kind"] == "LIST":
        return f"[{_format_type(type_ref['ofType'])}]"
    return type_ref["name"]
