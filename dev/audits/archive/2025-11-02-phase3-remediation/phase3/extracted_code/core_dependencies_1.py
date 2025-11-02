# Extracted from: docs/core/dependencies.md
# Block number: 1
from fraiseql.ivm import setup_auto_ivm

recommendation = await setup_auto_ivm(db_pool, verbose=True)
# ✓ Detected jsonb_ivm v1.1
# IVM Analysis: 5/8 tables benefit from incremental updates
