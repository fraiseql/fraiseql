# Extracted from: docs/rust/RUST_FIRST_PIPELINE.md
# Block number: 10
# Step 7: Python list operations
json_items = []
for row in rows:
    json_items.append(row[0])  # 150μs per 100 rows

# Step 8: Python string formatting
json_array = f"[{','.join(json_items)}]"  # 50μs
json_response = f'{{"data":{{"{field_name}":{json_array}}}}}'  # 30μs

# Step 9: Python → Rust FFI call
transformed = rust_transformer.transform(json_response, type_name)  # 10μs + 50μs FFI

# Step 10: Python string → bytes
response_bytes = transformed.encode('utf-8')  # 20μs

TOTAL: 310μs per 100 rows
