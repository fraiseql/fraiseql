# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 4
# ❌ OLD: Expected Python objects
result = await repo.find("v_user")
assert isinstance(result, list)  # Fails - now RustResponseBytes
assert result[0].name == "John"  # Fails - no longer Product instances

# Handle RustResponseBytes
import json

from fraiseql.core.rust_pipeline import RustResponseBytes

result = await repo.find("v_user")
if isinstance(result, RustResponseBytes):
    data = json.loads(bytes(result.bytes))
    users = data["data"]["v_user"]  # Note: field name matches query
else:
    users = result  # Fallback for compatibility

# For assertions:
assert isinstance(users, list)
assert users[0]["firstName"] == "John"  # Note: camelCase field names
