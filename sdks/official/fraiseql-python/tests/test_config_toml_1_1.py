"""``load_config`` parses ``fraiseql.toml`` as TOML 1.1, as ``fraiseql-cli compile`` does."""

from __future__ import annotations

import datetime
from typing import TYPE_CHECKING

from fraiseql import config

if TYPE_CHECKING:
    from pathlib import Path

# No [inject_defaults]: loading applies nothing to the global registry.
TOML_1_1 = """\
[deployment]
window = { opens = 07:30, closes = 18:00, }
"""


def test_a_toml_1_1_file_loads(tmp_path: Path) -> None:
    path = tmp_path / "fraiseql.toml"
    path.write_text(TOML_1_1)
    doc = config.load_config(path)
    assert doc["deployment"]["window"] == {
        "opens": datetime.time(7, 30),
        "closes": datetime.time(18, 0),
    }
