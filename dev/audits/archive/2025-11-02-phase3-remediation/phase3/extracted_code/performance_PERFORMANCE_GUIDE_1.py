# Extracted from: docs/performance/PERFORMANCE_GUIDE.md
# Block number: 1
# Standard production setup
config = FraiseQLConfig(
    apq_enabled=True,
    apq_storage_backend="postgresql",
    field_projection=True,
    complexity_max_score=1000,
)
