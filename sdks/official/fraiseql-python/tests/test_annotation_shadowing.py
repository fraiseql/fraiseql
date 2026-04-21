"""Tests for Python 3.14 annotation shadowing fix (issue #233).

On Python 3.14, accessing ``cls.__annotations__`` when a field name shadows a
type name (e.g. ``date: date | None = None``) raises ``TypeError`` because the
new ``__annotate__`` function checks the class ``__dict__`` before module
globals.  The fix in ``_get_class_annotations`` uses ``annotationlib`` on 3.14+
to evaluate annotations against module globals only.

The test classes are defined at module level so that the ``date`` import lives
in the module's global namespace — matching the real-world usage pattern.

``from __future__ import annotations`` keeps annotations as strings at class
definition time, preventing the shadowing error during class body execution.
This mirrors real-world usage where SDK users will use the same future import.

The ``date`` and ``Decimal`` imports must remain at runtime (not under
``TYPE_CHECKING``) because ``_get_class_annotations`` resolves them via
``eval`` against ``vars(module)``.
"""

from __future__ import annotations

import sys
from datetime import date  # noqa: TC003
from decimal import Decimal  # noqa: TC003

import pytest

from fraiseql.types import extract_field_info

# -- Test classes defined at module scope ------------------------------------
# The field name ``date`` shadows the imported ``date`` type.  On Python 3.14
# without the fix this would raise TypeError when extract_field_info accesses
# the annotations.

class _EventWithShadow:
    name: str
    year_end: date | None = None
    date: date | None = None  # shadows the type


class _PriceWithShadow:
    label: str
    amount: Decimal
    Decimal: Decimal | None = None  # shadows the type


class _MultiShadow:
    date: date | None = None
    str: str | None = None  # shadows built-in


# -- Tests -------------------------------------------------------------------

def test_field_name_shadows_imported_type() -> None:
    """Field named 'date' should not break 'date' type resolution (#233)."""
    fields = extract_field_info(_EventWithShadow)

    assert len(fields) == 3
    assert fields["name"] == {"type": "String", "nullable": False}
    assert fields["year_end"] == {"type": "date", "nullable": True}
    assert fields["date"] == {"type": "date", "nullable": True}


def test_field_name_shadows_decimal_type() -> None:
    """Field named 'Decimal' should not break Decimal type resolution."""
    fields = extract_field_info(_PriceWithShadow)

    assert fields["label"] == {"type": "String", "nullable": False}
    assert fields["amount"] == {"type": "Decimal", "nullable": False}
    assert fields["Decimal"] == {"type": "Decimal", "nullable": True}


def test_multiple_shadowed_fields() -> None:
    """Multiple fields shadowing types should all resolve correctly."""
    fields = extract_field_info(_MultiShadow)

    assert fields["date"] == {"type": "date", "nullable": True}
    assert fields["str"] == {"type": "String", "nullable": True}


@pytest.mark.skipif(
    sys.version_info < (3, 14),
    reason="annotationlib only available on Python 3.14+",
)
def test_annotationlib_path_is_used() -> None:
    """On 3.14+, _get_class_annotations must use annotationlib."""
    from fraiseql.types import _get_class_annotations

    # Should not raise even though 'date' field shadows the type
    annotations = _get_class_annotations(_EventWithShadow)
    assert "date" in annotations
    assert "year_end" in annotations
