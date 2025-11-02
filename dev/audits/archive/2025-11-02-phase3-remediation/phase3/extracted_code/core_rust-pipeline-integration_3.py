# Extracted from: docs/core/rust-pipeline-integration.md
# Block number: 3
import json

from fraiseql.core.rust_pipeline import RustResponseBytes

result = await repo.find("v_user")
if isinstance(result, RustResponseBytes):
    # Convert bytes to string for inspection
    json_str = result.bytes.decode("utf-8")
    print(json_str)  # See what Rust produced

    # Parse to verify structure
    data = json.loads(json_str)
    print(json.dumps(data, indent=2))
