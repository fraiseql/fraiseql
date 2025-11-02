# Extracted from: docs/core/configuration.md
# Block number: 1
from fraiseql import FraiseQLConfig, create_fraiseql_app

config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb", environment="production", enable_playground=False
)

app = create_fraiseql_app(types=[User, Post], config=config)
