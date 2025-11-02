# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 1
# ❌ OLD: Mode-specific configuration
config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL"),
    execution_mode=ExecutionMode.TURBO,  # Remove this
    enable_turbo_mode=True,  # Remove this
    passthrough_mode=False,  # Remove this
)

# ❌ OLD: Mode selection in context
repo = FraiseQLRepository(pool, context={"mode": "turbo"})
