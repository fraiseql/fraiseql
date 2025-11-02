# Extracted from: docs/reference/config.md
# Block number: 16
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    field_projection=True,  # Enable field filtering
    schema_registry=True,  # Enable schema-based transformation
)
