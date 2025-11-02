# Extracted from: docs/advanced/llm-integration.md
# Block number: 2
def schema_to_llm_prompt(schema: dict) -> str:
    """Convert GraphQL schema to compact prompt format."""
    prompt = "# GraphQL Schema\n\n"

    # Queries
    prompt += "## Queries\n\n"
    query_type = next(t for t in schema["types"] if t["name"] == "Query")
    for field in query_type["fields"]:
        args = ", ".join(f"{a['name']}: {a['type']}" for a in field["args"])
        prompt += f"- {field['name']}({args}): {field['type']}\n"
        if field.get("description"):
            prompt += f"  {field['description']}\n"

    # Mutations
    prompt += "\n## Mutations\n\n"
    mutation_type = next((t for t in schema["types"] if t["name"] == "Mutation"), None)
    if mutation_type:
        for field in mutation_type["fields"]:
            args = ", ".join(f"{a['name']}: {a['type']}" for a in field["args"])
            prompt += f"- {field['name']}({args}): {field['type']}\n"
            if field.get("description"):
                prompt += f"  {field['description']}\n"

    # Types
    prompt += "\n## Types\n\n"
    for type_def in schema["types"]:
        if type_def["kind"] == "OBJECT" and type_def["name"] not in ["Query", "Mutation"]:
            prompt += f"### {type_def['name']}\n"
            for field in type_def.get("fields", []):
                prompt += f"- {field['name']}: {field['type']}\n"
            prompt += "\n"

    return prompt
