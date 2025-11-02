# Extracted from: docs/reference/config.md
# Block number: 14
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    complexity_enabled=True,
    complexity_max_score=500,
    complexity_max_depth=8,
    complexity_field_multipliers={"users": 2, "posts": 1, "comments": 3},
)
