"""Field ordering utilities for FraiseQL type system.

This module provides a global field index counter that ensures consistent field
definition order across FraiseQL types. Field ordering is important for:

- GraphQL schema stability (field order affects schema fingerprinting)
- Serialization consistency (field order affects JSON output)
- Debugging predictability (fields appear in same order)

The counter starts at 1 and increments globally, ensuring that fields defined
later in the codebase always have higher index values than fields defined earlier.
This provides a stable, deterministic ordering that doesn't depend on Python's
dict ordering or class attribute iteration order.

Usage:
    Field indices are automatically assigned when FraiseQLField instances are created.
    The index is used internally for field ordering but is not exposed in the GraphQL schema.
"""

import itertools
from collections.abc import Iterator

_field_counter: Iterator[int] = itertools.count(start=1)


def next_field_index() -> int:
    """Returns a globally increasing index for field definition order.

    This function provides a thread-safe, monotonically increasing counter
    that ensures fields are assigned consistent indices regardless of the
    order in which they are processed during schema construction.

    Returns:
        The next available field index (starting from 1)
    """
    return next(_field_counter)
