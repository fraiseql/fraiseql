"""Cells: each checks one promise and reports whether it holds."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True)
class Outcome:
    """One cell's verdict. `key` is `CONC:<group>:<cell>`."""

    key: str
    holds: bool
    detail: dict[str, Any] = field(default_factory=dict)


class VacuousError(RuntimeError):
    """A cell's precondition failed, so its verdict would mean nothing."""
