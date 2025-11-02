# Extracted from: docs/strategic/V1_ADVANCED_PATTERNS.md
# Block number: 6
from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    # Use database functions for all mutations (DEFAULT)
    mutations_as_functions=True,
    # Function naming convention
    mutation_function_prefix="fn_",
    sync_function_prefix="fn_sync_tv_",
    # Auto-generate missing functions? (v1.1 feature)
    auto_generate_functions=False,
)
