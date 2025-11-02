# Extracted from: docs/advanced/filter-operators.md
# Block number: 1
@fraiseql.type
class Product:
    tags: list[str]  # ✅ Exposes array operators
    metadata: dict  # ✅ Exposes JSONB operators
    search_vector: str  # ❌ Needs TSVector type hint
