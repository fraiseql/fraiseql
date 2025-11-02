# Extracted from: docs/core/configuration.md
# Block number: 11
# Complexity limits
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    complexity_enabled=True,
    complexity_max_score=500,
    complexity_max_depth=8,
    complexity_default_list_size=20,
    complexity_field_multipliers={
        "users": 2,  # Users query costs 2x
        "posts": 1,  # Standard cost
        "comments": 3,  # Comments query costs 3x
    },
)
