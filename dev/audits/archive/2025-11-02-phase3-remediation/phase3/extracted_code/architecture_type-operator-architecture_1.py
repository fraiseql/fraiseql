# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 1
class FieldType(ScalarMarker):
    """Base class for all custom scalar types."""

    __slots__ = ()

    def __repr__(self) -> str:
        return "FieldType"
