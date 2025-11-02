# Extracted from: docs/patterns/README.md
# Block number: 1
@type(sql_source="v_post")
class Post:
    pk_post: int  # Internal: Fast joins, never exposed to API
    id: UUID  # Public: Stable API identifier
    identifier: str  # Human: "how-to-use-fraiseql" (SEO-friendly)
