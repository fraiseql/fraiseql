# Extracted from: docs/reference/quick-reference.md
# Block number: 19
from fraiseql.fastapi import FraiseQLRouter

# Add custom context
router = FraiseQLRouter(
    repo=repo,
    schema=fraiseql.build_schema(),
    context={"user_id": "current_user"},  # Available in resolvers
)
