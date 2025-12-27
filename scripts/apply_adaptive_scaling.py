#!/usr/bin/env python3
"""
Automated tool to apply adaptive scaling to chaos tests.

This script automates the repetitive work of converting hardcoded iteration
counts to adaptive formulas based on chaos_config.load_multiplier.

Usage:
    # Dry run (preview changes)
    python scripts/apply_adaptive_scaling.py tests/chaos/cache/test_cache_chaos.py --dry-run

    # Apply changes
    python scripts/apply_adaptive_scaling.py tests/chaos/cache/test_cache_chaos.py --apply

    # Batch process directory
    python scripts/apply_adaptive_scaling.py tests/chaos/cache/*.py --apply
"""

import re
import sys
import argparse
from pathlib import Path
from typing import List, Tuple, Optional


class AdaptiveScalingConverter:
    """Convert hardcoded iteration counts to adaptive scaling."""

    def __init__(self, dry_run: bool = True):
        self.dry_run = dry_run
        self.changes_made = 0
        self.files_processed = 0

    def detect_iteration_patterns(self, content: str) -> List[Tuple[str, int, str]]:
        """
        Detect hardcoded iteration patterns in test code.

        Returns list of (pattern, base_value, context) tuples.
        """
        patterns = []

        # Pattern 1: for i in range(NUMBER):
        for match in re.finditer(r'for\s+\w+\s+in\s+range\((\d+)\):', content):
            base_value = int(match.group(1))
            context = match.group(0)
            patterns.append(('for_range', base_value, context))

        # Pattern 2: num_threads = NUMBER
        for match in re.finditer(r'(num_threads|thread_count)\s*=\s*(\d+)', content):
            base_value = int(match.group(2))
            context = match.group(0)
            patterns.append(('num_threads', base_value, context))

        # Pattern 3: total_operations = NUMBER
        for match in re.finditer(r'total_operations\s*=\s*(\d+)', content):
            base_value = int(match.group(1))
            context = match.group(0)
            patterns.append(('total_operations', base_value, context))

        # Pattern 4: iterations = NUMBER
        for match in re.finditer(r'iterations\s*=\s*(\d+)', content):
            base_value = int(match.group(1))
            context = match.group(0)
            patterns.append(('iterations', base_value, context))

        return patterns

    def calculate_min_value(self, base_value: int) -> int:
        """Calculate minimum value (50% of base, rounded down, min 3)."""
        return max(3, base_value // 2)

    def generate_adaptive_formula(self, base_value: int, variable_name: str = 'iterations') -> str:
        """
        Generate adaptive formula code.

        Args:
            base_value: Original hardcoded value
            variable_name: Variable name (iterations, num_threads, etc.)

        Returns:
            Multi-line code string with comment and formula
        """
        min_value = self.calculate_min_value(base_value)
        max_value = base_value * 4  # HIGH profile (4.0x multiplier)

        # Generate comment
        comment = (
            f"# Scale {variable_name} based on hardware "
            f"({base_value} on baseline, {min_value}-{max_value} adaptive)\n"
            f"        # Uses multiplier-based formula to ensure meaningful test on all hardware"
        )

        # Generate formula
        formula = (
            f"{variable_name} = max({min_value}, int({base_value} * "
            f"self.chaos_config.load_multiplier))"
        )

        return f"{comment}\n        {formula}"

    def generate_docstring_scaling_section(self, base_value: int, variable_name: str = 'Iterations') -> str:
        """Generate the 'Adaptive Scaling' section for docstring."""
        min_value = self.calculate_min_value(base_value)
        max_value = base_value * 4

        return f"""
    Adaptive Scaling:
        - {variable_name}: {min_value}-{max_value} based on hardware (base={base_value})
        - LOW (0.5x): {min_value} {variable_name.lower()}
        - MEDIUM (1.0x): {base_value} {variable_name.lower()}
        - HIGH (4.0x): {max_value} {variable_name.lower()}

    Configuration:
        Uses self.chaos_config (auto-injected by conftest.py fixture)
"""

    def add_docstring_section(self, content: str, test_name: str, base_value: int) -> str:
        """Add Adaptive Scaling section to test docstring."""
        # Find the test function and its docstring
        pattern = rf'(def {re.escape(test_name)}\(self\):)\s*"""(.*?)"""'
        match = re.search(pattern, content, re.DOTALL)

        if not match:
            print(f"  ⚠️  Could not find docstring for {test_name}")
            return content

        # Extract docstring content
        func_def = match.group(1)
        docstring_content = match.group(2)

        # Check if already has Adaptive Scaling section
        if 'Adaptive Scaling:' in docstring_content:
            return content

        # Add scaling section before closing triple quotes
        scaling_section = self.generate_docstring_scaling_section(base_value)
        new_docstring = docstring_content.rstrip() + scaling_section

        # Replace in content
        new_content = content.replace(
            match.group(0),
            f'{func_def}\n        """{new_docstring}        """'
        )

        return new_content

    def convert_for_range_pattern(self, content: str, base_value: int, context: str) -> str:
        """
        Convert 'for i in range(N):' to adaptive pattern.

        Before:
            for i in range(10):

        After:
            # Scale iterations based on hardware (10 on baseline, 5-40 adaptive)
            # Uses multiplier-based formula to ensure meaningful test on all hardware
            iterations = max(5, int(10 * self.chaos_config.load_multiplier))

            for i in range(iterations):
        """
        # Generate adaptive code
        adaptive_code = self.generate_adaptive_formula(base_value, 'iterations')

        # Find the exact location and indentation
        match = re.search(rf'(\s*){re.escape(context)}', content)
        if not match:
            return content

        indent = match.group(1)
        replacement = f"{indent}{adaptive_code}\n\n{indent}for i in range(iterations):"

        return content.replace(match.group(0), replacement)

    def convert_variable_assignment(self, content: str, variable_name: str, base_value: int, context: str) -> str:
        """
        Convert variable assignment to adaptive pattern.

        Before:
            num_threads = 6

        After:
            # Scale num_threads based on hardware (6 on baseline, 3-24 adaptive)
            # Uses multiplier-based formula to ensure meaningful test on all hardware
            num_threads = max(3, int(6 * self.chaos_config.load_multiplier))
        """
        adaptive_code = self.generate_adaptive_formula(base_value, variable_name)

        # Find exact location and indentation
        match = re.search(rf'(\s*){re.escape(context)}', content)
        if not match:
            return content

        indent = match.group(1)
        replacement = f"{indent}{adaptive_code}"

        return content.replace(match.group(0), replacement)

    def process_file(self, file_path: Path) -> bool:
        """
        Process a single test file.

        Returns True if changes were made, False otherwise.
        """
        print(f"\n{'='*80}")
        print(f"Processing: {file_path}")
        print(f"{'='*80}")

        # Read file
        try:
            content = file_path.read_text()
        except Exception as e:
            print(f"❌ Error reading file: {e}")
            return False

        original_content = content

        # Detect patterns
        patterns = self.detect_iteration_patterns(content)

        if not patterns:
            print("ℹ️  No hardcoded iteration patterns found")
            return False

        print(f"\nFound {len(patterns)} pattern(s):")
        for pattern_type, base_value, context in patterns:
            print(f"  - {pattern_type}: {base_value} (context: {context[:50]}...)")

        # Apply conversions
        changes_in_file = 0

        for pattern_type, base_value, context in patterns:
            if pattern_type == 'for_range':
                new_content = self.convert_for_range_pattern(content, base_value, context)
                if new_content != content:
                    content = new_content
                    changes_in_file += 1
                    print(f"  ✅ Converted for_range pattern (base={base_value})")

            elif pattern_type in ('num_threads', 'thread_count'):
                new_content = self.convert_variable_assignment(content, 'num_threads', base_value, context)
                if new_content != content:
                    content = new_content
                    changes_in_file += 1
                    print(f"  ✅ Converted num_threads pattern (base={base_value})")

            elif pattern_type == 'total_operations':
                new_content = self.convert_variable_assignment(content, 'total_operations', base_value, context)
                if new_content != content:
                    content = new_content
                    changes_in_file += 1
                    print(f"  ✅ Converted total_operations pattern (base={base_value})")

            elif pattern_type == 'iterations':
                # Skip if already adaptive (contains chaos_config)
                if 'chaos_config' in context:
                    print(f"  ⏭️  Skipping iterations (already adaptive)")
                    continue

                new_content = self.convert_variable_assignment(content, 'iterations', base_value, context)
                if new_content != content:
                    content = new_content
                    changes_in_file += 1
                    print(f"  ✅ Converted iterations pattern (base={base_value})")

        if changes_in_file == 0:
            print("\nℹ️  No changes needed (patterns may already be adaptive)")
            return False

        # Write back (if not dry run)
        if not self.dry_run:
            try:
                file_path.write_text(content)
                print(f"\n✅ Applied {changes_in_file} change(s) to {file_path}")
                self.changes_made += changes_in_file
                return True
            except Exception as e:
                print(f"\n❌ Error writing file: {e}")
                return False
        else:
            print(f"\n🔍 DRY RUN: Would apply {changes_in_file} change(s)")
            print("\nPreview of changes:")
            print("-" * 80)
            # Show diff (simplified - just show the new content)
            print(content[:2000])  # Show first 2000 chars
            if len(content) > 2000:
                print(f"\n... ({len(content) - 2000} more characters)")
            return True

    def process_files(self, file_paths: List[Path]) -> None:
        """Process multiple files."""
        print(f"\n{'='*80}")
        print(f"Adaptive Scaling Converter")
        print(f"Mode: {'DRY RUN (preview only)' if self.dry_run else 'APPLY CHANGES'}")
        print(f"Files to process: {len(file_paths)}")
        print(f"{'='*80}")

        files_changed = 0

        for file_path in file_paths:
            if file_path.suffix != '.py':
                print(f"\n⏭️  Skipping non-Python file: {file_path}")
                continue

            if file_path.name.startswith('conftest'):
                print(f"\n⏭️  Skipping conftest.py: {file_path}")
                continue

            if 'validation' in file_path.name:
                print(f"\n⏭️  Skipping validation test: {file_path}")
                continue

            self.files_processed += 1
            if self.process_file(file_path):
                files_changed += 1

        # Summary
        print(f"\n{'='*80}")
        print(f"Summary")
        print(f"{'='*80}")
        print(f"Files processed: {self.files_processed}")
        print(f"Files changed: {files_changed}")
        if not self.dry_run:
            print(f"Total changes applied: {self.changes_made}")
        else:
            print(f"\n💡 This was a DRY RUN. Use --apply to make actual changes.")
        print(f"{'='*80}\n")


def main():
    parser = argparse.ArgumentParser(
        description='Apply adaptive scaling to chaos tests',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Preview changes to a single file
  python scripts/apply_adaptive_scaling.py tests/chaos/cache/test_cache_chaos.py --dry-run

  # Apply changes to a single file
  python scripts/apply_adaptive_scaling.py tests/chaos/cache/test_cache_chaos.py --apply

  # Apply changes to all files in a category
  python scripts/apply_adaptive_scaling.py tests/chaos/cache/test_*.py --apply

  # Preview changes to all cache tests
  python scripts/apply_adaptive_scaling.py tests/chaos/cache/*.py --dry-run
        """
    )

    parser.add_argument(
        'files',
        nargs='+',
        type=Path,
        help='Test file(s) to process (supports wildcards)'
    )

    parser.add_argument(
        '--apply',
        action='store_true',
        help='Apply changes (default is dry-run preview)'
    )

    parser.add_argument(
        '--dry-run',
        action='store_true',
        help='Preview changes without applying (default)'
    )

    args = parser.parse_args()

    # Default to dry-run unless --apply is specified
    dry_run = not args.apply

    # Expand wildcards
    file_paths = []
    for file_arg in args.files:
        if '*' in str(file_arg):
            # Handle wildcards
            parent = file_arg.parent
            pattern = file_arg.name
            file_paths.extend(parent.glob(pattern))
        else:
            file_paths.append(file_arg)

    if not file_paths:
        print("❌ No files found matching the pattern")
        sys.exit(1)

    # Process files
    converter = AdaptiveScalingConverter(dry_run=dry_run)
    converter.process_files(file_paths)


if __name__ == '__main__':
    main()
