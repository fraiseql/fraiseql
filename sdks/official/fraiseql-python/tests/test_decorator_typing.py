"""What a type checker sees of a decorated class or function.

``type``, ``query`` and ``mutation`` are each used bare (``@fraiseql.type``) and called
(``@fraiseql.query(sql_source=...)``). A checker must see the class or the function the
decorator returns, not a union with a decorator factory: otherwise every annotation that
names a decorated type is refused (``invalid-type-form``), and ``Order | None`` too
(``unsupported-operator``).

The checker is ``ty``, the one the SDK's suite runs. The project config ignores
``invalid-type-form`` for ``src``; these samples are checked with it, and the rest of
``ty``'s rules, as errors.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

SDK = Path(__file__).resolve().parent.parent
SAMPLES = Path(__file__).resolve().parent / "typing_samples"


def check(sample: str) -> tuple[int, list[str]]:
    """``ty``'s exit code over ``sample``, and its diagnostics, one per line."""
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "ty",
            "check",
            "--output-format",
            "concise",
            "--error",
            "invalid-type-form",
            "--error",
            "unsupported-operator",
            # An operation's body is its docstring, which the SDK documents; whether a
            # checker accepts that body is another question than what the decorator returns.
            "--ignore",
            "invalid-implicit-return-in-function-with-stub-body",
            str(SAMPLES / sample),
        ],
        cwd=SDK,
        capture_output=True,
        text=True,
        check=False,
        timeout=300,
    )
    lines = (result.stdout + result.stderr).splitlines()
    return result.returncode, [line for line in lines if f"{sample}:" in line]


def test_every_decorator_form_types_what_it_decorates() -> None:
    code, diagnostics = check("decorated.py")

    assert diagnostics == [], "\n".join(diagnostics)
    assert code == 0


def test_a_decorated_class_used_wrongly_is_still_reported() -> None:
    """The checker reads the class through the decorator, so it can refuse a misuse of it:
    the test above is not passing because nothing is checked."""
    code, diagnostics = check("misused.py")

    assert code != 0
    assert any("expected `int`, found `str`" in line for line in diagnostics), "\n".join(
        diagnostics
    )


if __name__ == "__main__":
    pytest.main([__file__])
