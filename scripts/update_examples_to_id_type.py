#!/usr/bin/env python3
"""Update examples to use ID type for entity identifiers.

This script converts uuid.UUID and UUID type annotations to ID for entity
identifiers, following FraiseQL's Trinity pattern conventions.

Usage:
    python scripts/update_examples_to_id_type.py [directory]

If no directory is specified, defaults to 'examples'.
"""
import re
import sys
from pathlib import Path

# Patterns that should stay as UUID (not entity identifiers)
KEEP_UUID = {
    'correlation_id',
    'external_ref',
    'reference_id',
    'causation_id',
    'event_id',
    'trace_id',
    'request_id',
    'session_id',
}


def update_file(path: Path, dry_run: bool = False) -> tuple[bool, list[str]]:
    """Update a single file to use ID type.

    Returns:
        Tuple of (was_updated, list_of_changes)
    """
    content = path.read_text()
    original = content
    changes = []

    # Skip if already using ID from fraiseql.types
    if 'from fraiseql.types import ID' in content:
        return False, []

    # Check if file needs updating (has UUID type annotations for IDs)
    if not re.search(r'id: (?:uuid\.)?UUID', content):
        return False, []

    # Add import after fraiseql imports
    import_added = False
    if 'import fraiseql\n' in content:
        content = re.sub(
            r'(import fraiseql\n)',
            r'\1from fraiseql.types import ID\n',
            content
        )
        import_added = True
    elif 'from fraiseql import' in content or 'from fraiseql.' in content:
        # Find the last fraiseql import line and add after it
        lines = content.split('\n')
        new_lines = []
        last_fraiseql_import_idx = -1

        for i, line in enumerate(lines):
            if line.startswith('from fraiseql') or line.startswith('import fraiseql'):
                last_fraiseql_import_idx = i

        if last_fraiseql_import_idx >= 0:
            for i, line in enumerate(lines):
                new_lines.append(line)
                if i == last_fraiseql_import_idx:
                    new_lines.append('from fraiseql.types import ID')
                    import_added = True
            content = '\n'.join(new_lines)

    if import_added:
        changes.append('Added: from fraiseql.types import ID')

    # Replace id: UUID with id: ID (primary identifiers)
    def count_and_replace(pattern, replacement, text):
        count = len(re.findall(pattern, text))
        new_text = re.sub(pattern, replacement, text)
        return new_text, count

    content, count = count_and_replace(r'\bid: uuid\.UUID\b', 'id: ID', content)
    if count:
        changes.append(f'Replaced: id: uuid.UUID -> id: ID ({count}x)')

    content, count = count_and_replace(r'\bid: UUID\b', 'id: ID', content)
    if count:
        changes.append(f'Replaced: id: UUID -> id: ID ({count}x)')

    # Replace *_id: UUID with *_id: ID (but not KEEP_UUID patterns)
    def replace_fk_uuid_dot(m):
        field_name = m.group(1) + '_id'
        if field_name in KEEP_UUID:
            return m.group(0)  # Keep original
        return f'{m.group(1)}_id: ID'

    def replace_fk_uuid(m):
        field_name = m.group(1) + '_id'
        if field_name in KEEP_UUID:
            return m.group(0)  # Keep original
        return f'{m.group(1)}_id: ID'

    # Count FK replacements
    fk_pattern_dot = r'\b(\w+)_id: uuid\.UUID\b'
    fk_pattern = r'\b(\w+)_id: UUID\b'

    fk_matches_dot = [(m.group(1) + '_id') for m in re.finditer(fk_pattern_dot, content)
                      if (m.group(1) + '_id') not in KEEP_UUID]
    fk_matches = [(m.group(1) + '_id') for m in re.finditer(fk_pattern, content)
                  if (m.group(1) + '_id') not in KEEP_UUID]

    content = re.sub(fk_pattern_dot, replace_fk_uuid_dot, content)
    content = re.sub(fk_pattern, replace_fk_uuid, content)

    if fk_matches_dot:
        changes.append(f'Replaced FK (uuid.UUID): {", ".join(set(fk_matches_dot))}')
    if fk_matches:
        changes.append(f'Replaced FK (UUID): {", ".join(set(fk_matches))}')

    if content != original:
        if not dry_run:
            path.write_text(content)
        return True, changes
    return False, []


def main():
    # Parse arguments
    dry_run = '--dry-run' in sys.argv
    args = [a for a in sys.argv[1:] if not a.startswith('--')]

    target_dir = Path(args[0]) if args else Path('examples')

    if not target_dir.exists():
        print(f"Error: Directory '{target_dir}' not found")
        sys.exit(1)

    print(f"{'[DRY RUN] ' if dry_run else ''}Updating files in: {target_dir}")
    print("=" * 60)

    updated = []
    skipped = []

    for py_file in sorted(target_dir.rglob('*.py')):
        was_updated, changes = update_file(py_file, dry_run=dry_run)
        if was_updated:
            updated.append((py_file, changes))
            print(f"\n{'[Would update]' if dry_run else 'Updated'}: {py_file}")
            for change in changes:
                print(f"  - {change}")
        else:
            skipped.append(py_file)

    print("\n" + "=" * 60)
    print(f"Summary: {len(updated)} files {'would be ' if dry_run else ''}updated, {len(skipped)} skipped")

    if updated:
        print(f"\n{'Would update' if dry_run else 'Updated'} files:")
        for py_file, _ in updated:
            print(f"  {py_file}")


if __name__ == '__main__':
    main()
