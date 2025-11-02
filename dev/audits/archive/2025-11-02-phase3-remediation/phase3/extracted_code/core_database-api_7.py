# Extracted from: docs/core/database-api.md
# Block number: 7
from psycopg.sql import SQL, Identifier, Placeholder

query = SQL("SELECT json_data FROM {} WHERE id = {}").format(Identifier("v_user"), Placeholder())

user = await repo.fetch_one(query, (user_id,))
