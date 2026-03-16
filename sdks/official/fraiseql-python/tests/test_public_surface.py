"""Verifies the public API surface of the fraiseql SDK.

This test file documents the stable public interface. Any removal from __all__
is a breaking change that requires a major version bump.
"""

import fraiseql


def test_all_defined() -> None:
    """__all__ must be defined."""
    assert hasattr(fraiseql, "__all__"), "fraiseql must define __all__"


def test_all_names_importable() -> None:
    """Every name in __all__ must be importable from fraiseql directly."""
    for name in fraiseql.__all__:
        assert hasattr(fraiseql, name), f"fraiseql.{name} is in __all__ but not importable"


def test_no_private_names_in_all() -> None:
    """__all__ must not expose private names."""
    for name in fraiseql.__all__:
        assert not name.startswith("_"), f"Private name in __all__: {name}"


def test_core_decorators_present() -> None:
    """Core authoring decorators must be importable."""
    assert callable(fraiseql.type)
    assert callable(fraiseql.query)
    assert callable(fraiseql.mutation)
    assert callable(fraiseql.subscription)
    assert callable(fraiseql.field)
    assert callable(fraiseql.enum)
    assert callable(fraiseql.input)
    assert callable(fraiseql.interface)
    assert callable(fraiseql.union)
    assert callable(fraiseql.scalar)


def test_client_present() -> None:
    """Client classes must be importable."""
    assert fraiseql.FraiseQLClient is not None
    assert issubclass(fraiseql.FraiseQLError, Exception)
    assert issubclass(fraiseql.FraiseQLAuthError, fraiseql.FraiseQLError)
    assert issubclass(fraiseql.FraiseQLUnsupportedError, fraiseql.FraiseQLError)
    assert issubclass(fraiseql.FraiseQLRateLimitError, fraiseql.FraiseQLError)
    assert issubclass(fraiseql.FraiseQLDatabaseError, fraiseql.FraiseQLError)


def test_schema_utilities_present() -> None:
    """Schema utilities must be importable."""
    assert callable(fraiseql.export_schema)
    assert callable(fraiseql.export_types)
    assert callable(fraiseql.generate_schema_json)
    assert fraiseql.SchemaRegistry is not None


def test_scalar_types_present() -> None:
    """Built-in scalar types must be importable."""
    assert fraiseql.ID is not None
    assert fraiseql.UUID is not None
    assert fraiseql.DateTime is not None
    assert fraiseql.Date is not None
    assert fraiseql.Time is not None
    assert fraiseql.Json is not None
    assert fraiseql.Decimal is not None
    assert fraiseql.Vector is not None
    assert fraiseql.CustomScalar is not None


def test_scope_validation_present() -> None:
    """Scope validation utilities must be importable."""
    assert callable(fraiseql.validate_scope)
    assert callable(fraiseql.describe_scope_format)
    assert issubclass(fraiseql.ScopeValidationError, Exception)


def test_version_present() -> None:
    """__version__ must be defined (though not in __all__ as it is a dunder)."""
    assert hasattr(fraiseql, "__version__")
    assert isinstance(fraiseql.__version__, str)
    assert fraiseql.__version__
