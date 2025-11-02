# Extracted from: docs/advanced/llm-integration.md
# Block number: 15
import logging

logger = logging.getLogger(__name__)


async def execute_llm_query_with_logging(user_request: str, query_text: str, user_id: str) -> dict:
    """Execute LLM query with comprehensive logging."""
    logger.info(
        "LLM query execution",
        extra={
            "user_id": user_id,
            "natural_language": user_request,
            "generated_query": query_text,
            "timestamp": datetime.utcnow().isoformat(),
        },
    )

    try:
        result = await execute_safe_query(query_text)

        logger.info(
            "LLM query success", extra={"user_id": user_id, "result_size": len(str(result))}
        )

        return result

    except Exception as e:
        logger.error(
            "LLM query failed", extra={"user_id": user_id, "error": str(e), "query": query_text}
        )
        raise
