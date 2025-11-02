# Extracted from: docs/advanced/llm-integration.md
# Block number: 14
from fraiseql.security import RateLimit, RateLimitRule

llm_rate_limits = [
    RateLimitRule(
        path_pattern="/graphql/llm",
        rate_limit=RateLimit(requests=10, window=60),  # 10 per minute
        message="LLM query rate limit exceeded",
    )
]
