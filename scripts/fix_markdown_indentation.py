#!/usr/bin/env python3
"""
Fix Python indentation in markdown code blocks.

This script fixes indentation issues in Python code blocks within markdown files.
The issue is that class and function bodies need proper Python indentation on top
of the markdown code block indentation.
"""

import re
import sys
from pathlib import Path


def fix_markdown_indentation(file_path: Path) -> int:
    """Fix indentation in Python code blocks in a markdown file."""
    with open(file_path, "r") as f:
        content = f.read()

    lines = content.split("\n")
    fixed_lines = []
    in_python_block = False
    in_class_or_function = False
    indent_level = 0

    i = 0
    while i < len(lines):
        line = lines[i]

        # Check for start of Python code block
        if line.strip() == "```python":
            in_python_block = True
            in_class_or_function = False
            indent_level = 0
            fixed_lines.append(line)
            i += 1
            continue

        # Check for end of code block
        if line.strip() == "```" and in_python_block:
            in_python_block = False
            in_class_or_function = False
            indent_level = 0
            fixed_lines.append(line)
            i += 1
            continue

        if in_python_block:
            # The line should already have proper markdown indentation (4 spaces)
            # We just need to ensure Python indentation is correct
            if line.startswith("    "):
                python_line = line[4:]  # Remove markdown indentation

                # Check if this is a class or function body line that needs more indentation
                if (
                    in_class_or_function
                    and python_line.strip()
                    and not re.match(r"^\s*(class|def)\s+\w+", python_line)
                ):
                    # This is a body line - ensure it has at least 4 spaces of Python indentation
                    current_python_indent = len(python_line) - len(python_line.lstrip())
                    if current_python_indent < 4:
                        python_line = "    " + python_line.lstrip()

                # Check for class or function definitions
                if re.match(r"^\s*(class|def)\s+\w+", python_line):
                    in_class_or_function = True
                elif python_line.strip() == "":
                    pass  # Empty line
                elif not python_line.startswith(" ") and not python_line.startswith("\t"):
                    # Line with no indentation - probably end of class/function
                    in_class_or_function = False

                # Add back markdown indentation
                fixed_line = "    " + python_line
            else:
                fixed_line = line

            fixed_lines.append(fixed_line)
        else:
            fixed_lines.append(line)

        i += 1

    new_content = "\n".join(fixed_lines)

    if new_content != content:
        with open(file_path, "w") as f:
            f.write(new_content)
        return 1  # File was modified

    return 0  # No changes


def main():
    if len(sys.argv) < 2:
        print("Usage: python fix_markdown_indentation.py <markdown_file>")
        sys.exit(1)

    file_path = Path(sys.argv[1])
    if not file_path.exists():
        print(f"File not found: {file_path}")
        sys.exit(1)

    modified = fix_markdown_indentation(file_path)
    if modified:
        print(f"Fixed indentation in {file_path}")
    else:
        print(f"No changes needed in {file_path}")


if __name__ == "__main__":
    main()
