# Extracted from: docs/core/configuration.md
# Block number: 7
# v1.0.0+ - Exclusive Rust pipeline
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    # Rust pipeline always active, minimal config needed
)
