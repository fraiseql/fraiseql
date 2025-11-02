# Extracted from: docs/reference/config.md
# Block number: 4
# Direct
config = FraiseQLConfig(database_url="postgresql://localhost/mydb")

# From environment
export FRAISEQL_DATABASE_URL="postgresql://localhost/mydb"
config = FraiseQLConfig()

# .env file
FRAISEQL_DATABASE_URL=postgresql://localhost/mydb
