import pytest

"""Test for unified Rust-first architecture - methods return RustResponseBytes instead of RawJSONResult."""

from fraiseql.core.rust_pipeline import RustResponseBytes
from fraiseql.db import FraiseQLRepository


@pytest.mark.unit
def test_unified_rust_methods_available() -> None:
    """Test that unified Rust-first methods are available."""
    context = {"mode": "production"}
    db = FraiseQLRepository(None, context)

    # Check unified methods exist (no more find_raw_json/find_rust split)
    assert hasattr(db, "find")
    assert hasattr(db, "find_one")

    # Check they're documented as unified Rust-first methods
    assert "Rust-first" in db.find.__doc__
    assert "Rust-first" in db.find_one.__doc__


def test_find_methods_return_rust_response_bytes() -> None:
    """Test that find methods return RustResponseBytes for unified architecture."""
    from typing import get_args, get_origin, get_type_hints

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
    # find() returns RustResponseBytes
    assert find_hints["return"] == RustResponseBytes

    # find_one() returns RustResponseBytes | None (nullable for null results)
    # This was changed in v1.1.7 to handle null results properly
    find_one_return = find_one_hints["return"]
    assert (
        get_origin(find_one_return) is type(None)
        or RustResponseBytes in get_args(find_one_return)
        or find_one_return == RustResponseBytes
    )
    # More precise check: it should be a Union with RustResponseBytes and None
    args = get_args(find_one_return)
    assert RustResponseBytes in args or find_one_return == RustResponseBytes
    assert type(None) in args or find_one_return == RustResponseBytes
