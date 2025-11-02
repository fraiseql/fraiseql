# Extracted from: docs/reference/config.md
# Block number: 18
from fraiseql.routing.config import EntityRoutingConfig

config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    entity_routing=EntityRoutingConfig(
        enabled=True,
        default_schema="public",
        entity_mapping={"User": "users_schema", "Post": "content_schema"},
    ),
)

# Or using dict
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    entity_routing={"enabled": True, "default_schema": "public"},
)
