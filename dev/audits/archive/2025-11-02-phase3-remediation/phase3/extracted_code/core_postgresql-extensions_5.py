# Extracted from: docs/core/postgresql-extensions.md
# Block number: 5
from fraiseql.ivm import setup_auto_ivm

recommendation = await setup_auto_ivm(db_pool, verbose=True)
# ✓ Detected jsonb_ivm v1.1
