# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 7
# ✅ Works unchanged
where = ProductWhere(price={"gt": 50})
result = await repo.find("products", where=where)

# Extract for testing:
products = extract_graphql_data(result, "products")
expensive_products = [p for p in products if p["price"] > 50]
