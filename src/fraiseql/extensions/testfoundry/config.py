"""Configuration for TestFoundry test generation."""

from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class FoundryConfig:
    """Configuration for TestFoundry test generation."""

    schema_name: str = "testfoundry"
    test_output_dir: Path = field(default_factory=lambda: Path("tests/generated"))
    table_prefix: str = "tb_"
    view_prefix: str = "v_"
    input_type_prefix: str = "type_"
    input_type_suffix: str = "_input"
    generate_pytest: bool = True
    debug_mode: bool = False

    naming_adapters: dict[str, str] = field(default_factory=dict)
    test_options: dict[str, bool] = field(
        default_factory=lambda: {
            "happy_path": True,
            "constraint_violations": True,
            "fk_violations": True,
            "soft_delete": True,
            "blocked_delete": False,
            "authorization": False,
        }
    )
