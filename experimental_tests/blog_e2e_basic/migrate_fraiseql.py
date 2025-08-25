#!/usr/bin/env python3
"""Automated FraiseQL Pattern Migration Script

This script automates common FraiseQL pattern migrations from
Enhanced/Optimized patterns to clean default patterns.
"""

import re
import sys
from pathlib import Path


def migrate_file(file_path: Path) -> bool:
    """Migrate a single file to use clean FraiseQL patterns."""
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read()

        original_content = content

        # Migration patterns
        migrations = [
            # Import migrations
            (r'from enhanced_fraiseql_pattern import', 'from fraiseql_defaults import'),
            (r'from fraiseql_tests\.enhanced_mutation import', 'from fraiseql_defaults import'),

            # Class name migrations
            (r'OptimizedFraiseQLMutation', 'FraiseQLMutation'),
            (r'EnhancedFraiseQLError', 'FraiseQLError'),

            # Legacy migrations
            (r'PrintOptimMutation', 'LegacyFraiseQLMutation'),
            (r'MutationResultBase', 'LegacyMutationResultBase'),
        ]

        # Apply migrations
        for pattern, replacement in migrations:
            content = re.sub(pattern, replacement, content)

        # Write back if changed
        if content != original_content:
            with open(file_path, 'w', encoding='utf-8') as f:
                f.write(content)
            return True

        return False

    except Exception as e:
        print(f'Error migrating {file_path}: {e}')
        return False


def main():
    if len(sys.argv) < 2:
        print('Usage: python migrate_fraiseql.py <project_path>')
        sys.exit(1)

    project_path = Path(sys.argv[1])
    if not project_path.exists():
        print(f'Project path does not exist: {project_path}')
        sys.exit(1)

    print('🚀 Starting FraiseQL pattern migration...')

    files_migrated = 0
    files_processed = 0

    for py_file in project_path.rglob('*.py'):
        # Skip test and migration files
        if any(skip in str(py_file) for skip in ['test_', '__pycache__', '.venv']):
            continue

        files_processed += 1
        if migrate_file(py_file):
            files_migrated += 1
            print(f'✅ Migrated: {py_file}')

    print(f'\n📊 Migration Complete:')
    print(f'   Files processed: {files_processed}')
    print(f'   Files migrated: {files_migrated}')

    if files_migrated > 0:
        print('\n⚠️  Please review changes and test your application!')


if __name__ == '__main__':
    main()
