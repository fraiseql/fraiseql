#!/usr/bin/env python3
"""Rename example directories to use underscores instead of hyphens,
following Python naming conventions and the DDL organization documentation.
"""

import shutil
from pathlib import Path


def rename_directories_with_hyphens(examples_dir: str = "examples"):
    """Rename directories containing hyphens to use underscores.

    Args:
        examples_dir: Path to the examples directory
    """
    examples_path = Path(examples_dir)

    if not examples_path.exists():
        print(f"Directory {examples_dir} does not exist")
        return

    renamed_count = 0

    for item in examples_path.iterdir():
        if item.is_dir() and "-" in item.name:
            new_name = item.name.replace("-", "_")
            new_path = item.parent / new_name

            print(f"Renaming: {item.name} -> {new_name}")
            shutil.move(str(item), str(new_path))
            renamed_count += 1

    print(f"Renamed {renamed_count} directories")


if __name__ == "__main__":
    rename_directories_with_hyphens()
