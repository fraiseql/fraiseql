#!/usr/bin/env python3
"""Verify that vector modules are properly installed and accessible.

This script tests that all vector-related modules can be imported
from an installed fraiseql package.

Usage:
    python scripts/verify_vector_modules.py
"""

import sys


def test_vector_scalar_import() -> bool:
    """Test importing VectorScalar and related types."""
    try:
        from fraiseql.types.scalars.vector import (
            VectorScalar,
            HalfVectorScalar,
            SparseVectorScalar,
        )

        print("✓ Vector scalars imported successfully")
        print(f"  VectorScalar: {VectorScalar}")
        print(f"  HalfVectorScalar: {HalfVectorScalar}")
        print(f"  SparseVectorScalar: {SparseVectorScalar}")
        return True
    except ImportError as e:
        print(f"✗ Failed to import vector scalars: {e}")
        return False


def test_vector_operators_import() -> bool:
    """Test importing vector operators."""
    try:
        from fraiseql.sql.where.operators import vectors

        distance_funcs = [f for f in dir(vectors) if "distance" in f.lower()]
        print("✓ Vector operators imported successfully")
        print(f"  Distance functions: {distance_funcs[:5]}...")
        return True
    except ImportError as e:
        print(f"✗ Failed to import vector operators: {e}")
        return False


def test_scalar_exports() -> bool:
    """Test that VectorScalar is exported from fraiseql.types.scalars."""
    try:
        from fraiseql.types.scalars import VectorScalar

        print("✓ VectorScalar exported from fraiseql.types.scalars")
        return True
    except ImportError as e:
        print(f"✗ VectorScalar not exported: {e}")
        return False


def main() -> int:
    """Run all verification tests."""
    print("=" * 60)
    print("FraiseQL Vector Module Verification")
    print("=" * 60)
    print()

    results = [
        test_vector_scalar_import(),
        test_vector_operators_import(),
        test_scalar_exports(),
    ]

    print()
    print("=" * 60)

    if all(results):
        print("✅ All vector modules are properly installed and accessible!")
        print()
        print("You can now use vector features like:")
        print('  from fraiseql.types.scalars import VectorScalar')
        print('  from fraiseql.sql.where.operators import vectors')
        return 0
    else:
        print("❌ Some vector modules are missing or not accessible.")
        print()
        print("Troubleshooting:")
        print("  1. Ensure you have fraiseql >= 1.5.0 installed")
        print("  2. Try reinstalling: pip install --force-reinstall fraiseql")
        print("  3. Check installation: pip show fraiseql")
        return 1


if __name__ == "__main__":
    sys.exit(main())
