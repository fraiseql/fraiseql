# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 2
# Simplified configuration
config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL")
    # Rust pipeline always active, no additional config needed
)
