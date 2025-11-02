# Extracted from: docs/production/monitoring.md
# Block number: 19
from fraiseql.analysis.complexity import analyze_query_complexity


async def track_query_complexity(query: str, operation_name: str):
    """Track query complexity metrics."""
    complexity = analyze_query_complexity(query)

    graphql_query_complexity.observe(complexity.score)

    if complexity.score > 500:
        logger.warning(
            "High complexity query",
            extra={
                "operation": operation_name,
                "complexity": complexity.score,
                "depth": complexity.depth,
                "fields": complexity.field_count,
            },
        )
