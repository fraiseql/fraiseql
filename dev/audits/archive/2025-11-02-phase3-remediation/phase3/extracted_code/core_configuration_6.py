# Extracted from: docs/core/configuration.md
# Block number: 6
# JSONB-optimized configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    jsonb_extraction_enabled=True,
    jsonb_default_columns=["data", "metadata", "json_data"],
    jsonb_auto_detect=True,
    jsonb_field_limit_threshold=30,
)
