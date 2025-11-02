# Extracted from: docs/core/configuration.md
# Block number: 4
from fraiseql.fastapi.config import IntrospectionPolicy

# Production configuration (introspection disabled)
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="production",
    introspection_policy=IntrospectionPolicy.DISABLED,
    enable_playground=False,
    max_query_depth=10,
    query_timeout=15,
)

# Development configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="development",
    introspection_policy=IntrospectionPolicy.PUBLIC,
    enable_playground=True,
    playground_tool="graphiql",
    database_echo=True,  # Log all SQL queries
)
