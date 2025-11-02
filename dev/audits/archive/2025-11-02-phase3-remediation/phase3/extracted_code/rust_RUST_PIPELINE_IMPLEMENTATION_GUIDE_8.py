# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 8
# In your application
import fraiseql_rs

from fraiseql.core.rust_pipeline import RustResponseBytes

# Verify Rust extension loaded
print("Rust pipeline available:", hasattr(fraiseql_rs, "build_list_response"))

# Check repository methods
result = await repo.find_rust("v_user", "users", info)
print("Using Rust pipeline:", isinstance(result, RustResponseBytes))
