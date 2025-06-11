"""Missing docstring."""

import itertools
from collections.abc import Iterator

_field_counter: Iterator[int] = itertools.count(start=1)


def next_field_index() -> int:
    """Returns a globally increasing index for field definition order."""
    return next(_field_counter)
