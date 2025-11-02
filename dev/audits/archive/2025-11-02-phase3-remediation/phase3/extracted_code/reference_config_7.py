# Extracted from: docs/reference/config.md
# Block number: 7
from fraiseql.fastapi.config import IntrospectionPolicy

# Disable introspection in production
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="production",
    introspection_policy=IntrospectionPolicy.DISABLED,
)

# Require auth for introspection
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    introspection_policy=IntrospectionPolicy.AUTHENTICATED,
)
