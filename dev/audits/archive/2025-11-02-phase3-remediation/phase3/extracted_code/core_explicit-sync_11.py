# Extracted from: docs/core/explicit-sync.md
# Block number: 11
# ✅ Only denormalize what GraphQL queries need
jsonb_data = {
    "id": str(post["id"]),
    "title": post["title"],  # Queried often
    "author": {
        "username": author["username"]  # Queried often
    },
    # Don't include: post["content"] if GraphQL doesn't query it in lists
}
