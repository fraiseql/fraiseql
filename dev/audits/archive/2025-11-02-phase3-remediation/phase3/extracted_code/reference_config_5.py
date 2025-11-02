# Extracted from: docs/reference/config.md
# Block number: 5
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    database_pool_size=50,
    database_max_overflow=20,
    database_pool_timeout=60,
    database_echo=True,  # Development only
)
