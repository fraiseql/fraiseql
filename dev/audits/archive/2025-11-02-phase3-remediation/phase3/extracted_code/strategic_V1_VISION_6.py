# Extracted from: docs/strategic/V1_VISION.md
# Block number: 6
# Transparent - user doesn't see this
result = await query_repo.find_one("tv_user", id=user_id)
# ↑ Automatically runs through Rust transformer
# Snake case DB → CamelCase GraphQL, field selection, type coercion
