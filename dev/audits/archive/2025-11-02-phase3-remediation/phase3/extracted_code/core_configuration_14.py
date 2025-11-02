# Extracted from: docs/core/configuration.md
# Block number: 14
# Custom schema configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    default_mutation_schema="app",
    default_query_schema="api",
)
