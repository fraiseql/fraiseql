# Extracted from: docs/advanced/llm-integration.md
# Block number: 3
QUERY_GENERATION_PROMPT = """
You are a GraphQL query generator. Given a natural language request and a GraphQL schema,
generate a valid GraphQL query.

Schema:
{schema}

Rules:
1. Use only fields that exist in the schema
2. Include only requested fields in the selection set
3. Use proper argument types
4. Limit queries to reasonable depth (max 3 levels)
5. Add __typename for debugging if needed

User Request: {user_request}

Generate ONLY the GraphQL query, no explanation:
"""


async def generate_query_with_llm(user_request: str, llm_client) -> str:
    """Generate GraphQL query using LLM."""
    # Get schema
    schema = await get_schema_for_llm(None)
    schema_text = schema_to_llm_prompt(schema)

    # Build prompt
    prompt = QUERY_GENERATION_PROMPT.format(schema=schema_text, user_request=user_request)

    # Call LLM
    response = await llm_client.complete(prompt)

    # Extract query
    query_text = extract_graphql_query(response)

    return query_text


def extract_graphql_query(llm_response: str) -> str:
    """Extract GraphQL query from LLM response."""
    # Remove markdown code blocks
    if "```graphql" in llm_response:
        query = llm_response.split("```graphql")[1].split("```")[0].strip()
    elif "```" in llm_response:
        query = llm_response.split("```")[1].split("```")[0].strip()
    else:
        query = llm_response.strip()

    return query
