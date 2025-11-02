# Extracted from: docs/production/security.md
# Block number: 4
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    # Query complexity limits
    complexity_enabled=True,
    complexity_max_score=1000,
    complexity_max_depth=10,
    complexity_default_list_size=10,
    # Field-specific multipliers
    complexity_field_multipliers={
        "users": 2,  # Expensive field
        "orders": 3,
        "analytics": 10,
    },
)
