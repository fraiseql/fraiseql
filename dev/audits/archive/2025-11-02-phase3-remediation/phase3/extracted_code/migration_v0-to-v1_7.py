# Extracted from: docs/migration/v0-to-v1.md
# Block number: 7
# Old
users = await repo.query("SELECT * FROM users WHERE active = true")

# New
users = await repo.find("users_view", is_active=True)
