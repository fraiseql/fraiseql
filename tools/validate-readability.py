#!/usr/bin/env python3

"""
FraiseQL Documentation Readability Validator

Validates documentation readability metrics:
- Average sentence length (target: < 20 words)
- Flesch-Kincaid grade level (target: < 12)
- Paragraph length (target: max 5 sentences)
- Code block size (target: < 30 lines)

Usage:
    python3 validate-readability.py [--verbose] [docs_dir]

Exit codes:
    0 = All files meet readability targets
    1 = Some files below target
    2 = No markdown files found
    3 = Script error
"""

import os
import sys
import re
from pathlib import Path
from typing import List, Dict, Tuple
from collections import defaultdict

# ANSI color codes
GREEN = '\033[0;32m'
RED = '\033[0;31m'
YELLOW = '\033[1;33m'
BLUE = '\033[0;34m'
NC = '\033[0m'


class ReadabilityValidator:
    def __init__(self, docs_dir: str, verbose: bool = False):
        self.docs_dir = Path(docs_dir).resolve()
        self.verbose = verbose
        self.issues: Dict[str, List[str]] = defaultdict(list)
        self.metrics: Dict[str, Dict] = {}
        self.checked_files = 0

    def log_error(self, message: str) -> None:
        """Log an error message"""
        print(f"{RED}✗{NC} {message}", file=sys.stderr)

    def log_warning(self, message: str) -> None:
        """Log a warning message"""
        print(f"{YELLOW}⚠{NC} {message}", file=sys.stderr)

    def log_success(self, message: str) -> None:
        """Log a success message"""
        print(f"{GREEN}✓{NC} {message}")

    def log_info(self, message: str) -> None:
        """Log an info message"""
        print(f"{BLUE}ℹ{NC} {message}")

    def count_words(self, text: str) -> int:
        """Count words in text"""
        return len(text.split())

    def count_syllables(self, word: str) -> int:
        """Estimate syllable count"""
        word = word.lower()
        count = 0
        vowels = "aeiouy"
        previous_was_vowel = False

        for char in word:
            is_vowel = char in vowels
            if is_vowel and not previous_was_vowel:
                count += 1
            previous_was_vowel = is_vowel

        # Adjust for silent e
        if word.endswith("e"):
            count -= 1

        # Adjust for le
        if word.endswith("le") and len(word) > 2 and word[-3] not in vowels:
            count += 1

        return max(1, count)

    def calculate_flesch_kincaid(self, text: str) -> float:
        """Calculate Flesch-Kincaid grade level"""
        sentences = re.split(r'[.!?]+', text)
        sentences = [s.strip() for s in sentences if s.strip()]

        if not sentences:
            return 0

        words = text.split()
        syllables = sum(self.count_syllables(word) for word in words if word.isalpha())

        if len(words) == 0:
            return 0

        # Flesch-Kincaid Grade Level formula
        grade = (0.39 * len(words) / len(sentences)) + (11.8 * syllables / len(words)) - 15.59
        return max(0, grade)

    def extract_paragraphs(self, content: str) -> List[str]:
        """Extract paragraphs from markdown content"""
        # Remove code blocks
        content_no_code = re.sub(r'```.*?```', '', content, flags=re.DOTALL)
        # Remove links and formatting
        content_no_format = re.sub(r'\[([^\]]+)\]\([^\)]+\)', r'\1', content_no_code)
        content_no_format = re.sub(r'\*\*([^\*]+)\*\*', r'\1', content_no_format)
        content_no_format = re.sub(r'_([^_]+)_', r'\1', content_no_format)

        # Split by blank lines
        paragraphs = [p.strip() for p in content_no_format.split('\n\n') if p.strip()]
        return paragraphs

    def extract_code_blocks(self, content: str) -> List[str]:
        """Extract code blocks"""
        blocks = []
        matches = re.finditer(r'```.*?\n(.*?)\n```', content, re.DOTALL)
        for match in matches:
            blocks.append(match.group(1))
        return blocks

    def check_readability(self, file_path: Path) -> Tuple[bool, Dict]:
        """Check readability metrics for a file"""
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()
        except (IOError, OSError):
            return False, {}

        metrics = {}
        issues = []

        # Extract paragraphs and code blocks
        paragraphs = self.extract_paragraphs(content)
        code_blocks = self.extract_code_blocks(content)

        if not paragraphs:
            return True, metrics

        # Check paragraph length (max 5 sentences)
        for i, para in enumerate(paragraphs):
            sentences = re.split(r'[.!?]+', para)
            sentences = [s.strip() for s in sentences if s.strip()]

            if len(sentences) > 5:
                issues.append(f"Paragraph {i+1}: {len(sentences)} sentences (target: < 5)")

        # Check code block size
        for i, block in enumerate(code_blocks):
            lines = block.strip().split('\n')
            if len(lines) > 30:
                issues.append(f"Code block {i+1}: {len(lines)} lines (target: < 30)")

        # Calculate readability metrics
        full_text = ' '.join(paragraphs)
        grade_level = self.calculate_flesch_kincaid(full_text)
        avg_sentence_length = self.count_words(full_text) / max(1, len(re.split(r'[.!?]+', full_text)))

        metrics['grade_level'] = grade_level
        metrics['avg_sentence_length'] = avg_sentence_length
        metrics['paragraph_count'] = len(paragraphs)
        metrics['code_block_count'] = len(code_blocks)

        if grade_level > 12:
            issues.append(f"Grade level: {grade_level:.1f} (target: < 12)")

        if avg_sentence_length > 20:
            issues.append(f"Avg sentence length: {avg_sentence_length:.1f} words (target: < 20)")

        return len(issues) == 0, issues, metrics

    def validate_file(self, markdown_file: Path) -> None:
        """Validate a single markdown file"""
        self.checked_files += 1
        relative_path = markdown_file.relative_to(self.docs_dir)

        valid, issues, metrics = self.check_readability(markdown_file)
        self.metrics[str(relative_path)] = metrics

        if not valid:
            self.issues[str(relative_path)] = issues
            if self.verbose:
                self.log_warning(f"{relative_path}:")
                for issue in issues:
                    self.log_warning(f"  - {issue}")

    def validate_docs(self) -> int:
        """Validate readability of all documentation"""
        if not self.docs_dir.exists():
            self.log_error(f"Documentation directory not found: {self.docs_dir}")
            return 3

        # Find all markdown files
        markdown_files = sorted(self.docs_dir.glob('**/*.md'))

        if not markdown_files:
            self.log_error(f"No markdown files found in {self.docs_dir}")
            return 2

        self.log_info(f"FraiseQL Documentation Readability Validator")
        self.log_info(f"Checking documentation in: {self.docs_dir}")
        self.log_info(f"Found {len(markdown_files)} markdown files")
        print()

        # Validate each file
        for markdown_file in markdown_files:
            self.validate_file(markdown_file)

        # Calculate statistics
        grade_levels = [m.get('grade_level', 0) for m in self.metrics.values() if 'grade_level' in m]
        sentence_lengths = [m.get('avg_sentence_length', 0) for m in self.metrics.values() if 'avg_sentence_length' in m]

        avg_grade = sum(grade_levels) / len(grade_levels) if grade_levels else 0
        max_grade = max(grade_levels) if grade_levels else 0
        avg_sentence = sum(sentence_lengths) / len(sentence_lengths) if sentence_lengths else 0
        max_sentence = max(sentence_lengths) if sentence_lengths else 0

        # Print summary
        print()
        print("━" * 60)
        print(f"{BLUE}ℹ{NC} Readability Summary")
        print()
        print(f"  Average Grade Level: {avg_grade:.1f} (target: < 12)")
        print(f"  Maximum Grade Level: {max_grade:.1f}")
        print(f"  Average Sentence Length: {avg_sentence:.1f} words (target: < 20)")
        print(f"  Maximum Sentence Length: {max_sentence:.1f} words")
        print()

        if self.issues:
            self.log_error(f"Found readability issues in {len(self.issues)} file(s)")
            return 1
        else:
            self.log_success(f"All {self.checked_files} files meet readability targets!")
            return 0


def main():
    """Main entry point"""
    import argparse

    parser = argparse.ArgumentParser(
        description="Validate readability of FraiseQL documentation"
    )

    parser.add_argument(
        'docs_dir',
        nargs='?',
        default='.',
        help='Documentation directory (default: current directory)'
    )

    parser.add_argument(
        '-v', '--verbose',
        action='store_true',
        help='Print verbose output'
    )

    args = parser.parse_args()

    validator = ReadabilityValidator(args.docs_dir, verbose=args.verbose)
    return validator.validate_docs()


if __name__ == '__main__':
    sys.exit(main())
