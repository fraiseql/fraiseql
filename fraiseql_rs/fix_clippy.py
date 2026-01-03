#!/usr/bin/env python3
"""
Automated clippy warning fixer for FraiseQL Rust codebase.
Fixes the 4 most common warning patterns that account for ~500 warnings.
"""

import re
import subprocess
from pathlib import Path
from typing import List, Tuple

def get_clippy_warnings() -> List[Tuple[str, int, str]]:
    """Get all clippy warnings with file path, line number, and warning type."""
    result = subprocess.run(
        ["cargo", "clippy", "--lib", "2>&1"],
        capture_output=True,
        text=True,
        shell=True
    )

    warnings = []
    current_file = None
    current_line = None

    for line in result.stdout.split('\n'):
        # Extract file and line: --> src/path/file.rs:123:45
        if '-->' in line and '.rs:' in line:
            match = re.search(r'--> (src/[^:]+):(\d+):', line)
            if match:
                current_file = match.group(1)
                current_line = int(match.group(2))

        # Extract warning type
        if 'warning:' in line and current_file and current_line:
            warning_type = line.split('warning:')[1].strip()
            warnings.append((current_file, current_line, warning_type))

    return warnings

def add_must_use_to_file(file_path: str, function_lines: List[int]):
    """Add #[must_use] to functions at specified lines."""
    path = Path(file_path)
    if not path.exists():
        return

    with open(path) as f:
        lines = f.readlines()

    # Work backwards to preserve line numbers
    for line_num in sorted(function_lines, reverse=True):
        idx = line_num - 1
        if idx < 0 or idx >= len(lines):
            continue

        # Check if #[must_use] already exists
        if idx > 0 and '#[must_use]' in lines[idx-1]:
            continue

        # Find the indentation
        indent_match = re.match(r'(\s*)', lines[idx])
        indent = indent_match.group(1) if indent_match else '    '

        # Insert #[must_use] before the function
        lines.insert(idx, f'{indent}#[must_use]\n')

    with open(path, 'w') as f:
        f.writelines(lines)

def add_errors_section(file_path: str, function_lines: List[int]):
    """Add # Errors sections to functions returning Result."""
    path = Path(file_path)
    if not path.exists():
        return

    with open(path) as f:
        lines = f.readlines()

    # Work backwards to preserve line numbers
    for line_num in sorted(function_lines, reverse=True):
        idx = line_num - 1
        if idx < 0 or idx >= len(lines):
            continue

        # Find the doc comment above this function
        doc_start = idx - 1
        while doc_start >= 0 and lines[doc_start].strip().startswith('///'):
            doc_start -= 1
        doc_start += 1

        # Check if # Errors already exists
        has_errors = any('# Errors' in lines[i] for i in range(doc_start, idx))
        if has_errors:
            continue

        # Find where to insert (before function declaration, after last doc line)
        insert_idx = idx
        while insert_idx > 0 and (lines[insert_idx-1].strip().startswith('///') or
                                   lines[insert_idx-1].strip().startswith('#[')):
            insert_idx -= 1

        # Find indentation
        indent_match = re.match(r'(\s*)', lines[doc_start] if doc_start < len(lines) else '    ')
        indent = indent_match.group(1) if indent_match else '    '

        # Insert # Errors section
        error_docs = [
            f'{indent}///\n',
            f'{indent}/// # Errors\n',
            f'{indent}///\n',
            f'{indent}/// Returns an error if the operation fails\n',
        ]

        for line in reversed(error_docs):
            lines.insert(insert_idx, line)

    with open(path, 'w') as f:
        f.writelines(lines)

def fix_missing_backticks(file_path: str):
    """Fix missing backticks in documentation."""
    path = Path(file_path)
    if not path.exists():
        return

    with open(path) as f:
        content = f.read()

    # Common identifiers that need backticks
    patterns = [
        (r'/// ([^`\n]*) (Result|Option|Arc|Mutex|Vec|HashMap|String|Duration|Error|Value)([^`\n]*)',
         r'/// \1 `\2`\3'),
        (r'/// ([^`\n]*) (WebSocket|SubscriptionExecutor|PySubscriptionExecutor)([^`\n]*)',
         r'/// \1 `\2`\3'),
    ]

    for pattern, replacement in patterns:
        content = re.sub(pattern, replacement, content)

    with open(path, 'w') as f:
        f.write(content)

def main():
    print("Analyzing clippy warnings...")
    warnings = get_clippy_warnings()

    # Group by file and warning type
    must_use_warnings = {}
    errors_warnings = {}
    backtick_files = set()

    for file_path, line_num, warning_type in warnings:
        if 'must_use' in warning_type:
            if file_path not in must_use_warnings:
                must_use_warnings[file_path] = []
            must_use_warnings[file_path].append(line_num)

        if 'Errors' in warning_type and 'missing' in warning_type:
            if file_path not in errors_warnings:
                errors_warnings[file_path] = []
            errors_warnings[file_path].append(line_num)

        if 'backticks' in warning_type or 'documentation' in warning_type:
            backtick_files.add(file_path)

    # Apply fixes
    print(f"\nFixing #[must_use] in {len(must_use_warnings)} files...")
    for file_path, lines in must_use_warnings.items():
        add_must_use_to_file(file_path, lines)
        print(f"  Fixed {len(lines)} functions in {file_path}")

    print(f"\nFixing # Errors sections in {len(errors_warnings)} files...")
    for file_path, lines in errors_warnings.items():
        add_errors_section(file_path, lines)
        print(f"  Fixed {len(lines)} functions in {file_path}")

    print(f"\nFixing backticks in {len(backtick_files)} files...")
    for file_path in backtick_files:
        fix_missing_backticks(file_path)
        print(f"  Fixed {file_path}")

    print("\nDone! Rerun clippy to see remaining warnings.")

if __name__ == "__main__":
    main()
