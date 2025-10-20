import pytest

"""Test for unified Rust-first architecture - methods return RustResponseBytes instead of RawJSONResult."""

from fraiseql.db import FraiseQLRepository
from fraiseql.core.rust_pipeline import RustResponseBytes


@pytest.mark.unit
def test_unified_rust_methods_available():
    """Test that unified Rust-first methods are available."""
    context = {"mode": "production"}
    db = FraiseQLRepository(None, context)

    # Check unified methods exist (no more find_raw_json/find_rust split)
    assert hasattr(db, "find")
    assert hasattr(db, "find_one")

    # Check they're documented as unified Rust-first methods
    assert "Rust-first" in db.find.__doc__
    assert "Rust-first" in db.find_one.__doc__


def test_find_methods_return_rust_response_bytes():
    """Test that find methods return RustResponseBytes for unified architecture."""
    from typing import get_type_hints

    # Get type hints for methods
    find_hints = get_type_hints(FraiseQLRepository.find)
    find_one_hints = get_type_hints(FraiseQLRepository.find_one)

    # Check return types - should be RustResponseBytes, not RawJSONResult
    assert "return" in find_hints
    assert "return" in find_one_hints

    # The return type should be RustResponseBytes
    find_return_type_str = str(find_hints["return"])
    find_one_return_type_str = str(find_one_hints["return"])

    assert "RustResponseBytes" in find_return_type_str
    assert "RustResponseBytes" in find_one_return_type_str

    # Verify the actual type
    assert find_hints["return"] == RustResponseBytes
    assert find_one_hints["return"] == RustResponseBytes
