# Extracted from: docs/core/database-api.md
# Block number: 13
query = SQL("INSERT INTO {} (name, email) VALUES ({}, {})").format(
    Identifier("tb_users"), Placeholder(), Placeholder()
)

await repo.execute_many(
    query,
    [
        ("Alice", "alice@example.com"),
        ("Bob", "bob@example.com"),
        ("Charlie", "charlie@example.com"),
    ],
)
