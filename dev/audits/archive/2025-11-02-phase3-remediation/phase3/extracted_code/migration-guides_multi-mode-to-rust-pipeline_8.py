# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 8
import json

from fraiseql.core.rust_pipeline import RustResponseBytes

result = await repo.find("users")
if isinstance(result, RustResponseBytes):
    data = json.loads(bytes(result.bytes))
    users = data["data"]["users"]
else:
    users = result
