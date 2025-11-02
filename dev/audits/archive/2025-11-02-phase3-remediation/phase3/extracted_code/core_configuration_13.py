# Extracted from: docs/core/configuration.md
# Block number: 13
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    # Rust pipeline is always active
    field_projection=True,  # Optional: disable for debugging
)
