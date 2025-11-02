# Extracted from: docs/advanced/llm-integration.md
# Block number: 9
async def generate_and_refine_query(
    user_request: str, llm_client, schema, max_attempts: int = 3
) -> str:
    """Generate query with automatic refinement on errors."""
    for attempt in range(max_attempts):
        # Generate query
        query_text = await generate_query_with_llm(user_request, llm_client)

        # Validate
        try:
            document = parse(query_text)
            errors = validate(schema, document)

            if not errors:
                return query_text  # Success

            # Refine prompt with error feedback
            error_feedback = "\n".join(str(e) for e in errors)
            user_request += f"\n\nPrevious attempt failed with errors:\n{error_feedback}\n\nPlease fix these errors."

        except Exception as e:
            # Syntax error
            user_request += (
                f"\n\nPrevious attempt had syntax error: {e}\n\nPlease generate valid GraphQL."
            )

    raise ValueError(f"Failed to generate valid query after {max_attempts} attempts")
