# Extracted from: docs/core/postgresql-extensions.md
# Block number: 3
# test_extensions.py
from fraiseql.ivm import IVMAnalyzer


async def test_extensions():
    analyzer = IVMAnalyzer(db_pool)

    # Check jsonb_ivm
    has_ivm = await analyzer.check_extension()
    print(f"jsonb_ivm available: {has_ivm}")
    print(f"Version: {analyzer.extension_version}")


test_extensions()
