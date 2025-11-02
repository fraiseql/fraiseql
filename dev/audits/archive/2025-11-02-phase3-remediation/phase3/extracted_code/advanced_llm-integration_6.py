# Extracted from: docs/advanced/llm-integration.md
# Block number: 6
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    complexity_enabled=True,
    complexity_max_score=100,  # Lower for LLM queries
    complexity_max_depth=3,  # Prevent deep nesting
    complexity_default_list_size=10,
)
