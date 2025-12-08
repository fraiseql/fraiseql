#!/usr/bin/env python3
"""
Contradiction Detection Script for FraiseQL Documentation

Searches for key topics across documentation and identifies potential
contradictions or inconsistencies in explanations, recommendations, or examples.

Usage:
    python scripts/check_contradictions.py
    python scripts/check_contradictions.py --topic "trinity pattern"
    python scripts/check_contradictions.py --verbose
"""

import argparse
import re
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Dict, Set


@dataclass
class TopicMention:
    """Represents a mention of a topic in documentation"""
    file_path: Path
    line_number: int
    context: str  # Surrounding text
    full_line: str


@dataclass
class Contradiction:
    """Represents a potential contradiction"""
    topic: str
    description: str
    mentions: List[TopicMention]
    severity: str  # 'critical', 'high', 'medium', 'low'


@dataclass
class ContradictionReport:
    """Full contradiction analysis report"""
    topics_checked: List[str] = field(default_factory=list)
    contradictions: List[Contradiction] = field(default_factory=list)
    files_scanned: int = 0

    @property
    def critical_count(self) -> int:
        return len([c for c in self.contradictions if c.severity == 'critical'])

    @property
    def high_count(self) -> int:
        return len([c for c in self.contradictions if c.severity == 'high'])

    @property
    def medium_count(self) -> int:
        return len([c for c in self.contradictions if c.severity == 'medium'])

    @property
    def total_count(self) -> int:
        return len(self.contradictions)


class ContradictionDetector:
    """Detects contradictions in documentation"""

    # Key topics to check
    KEY_TOPICS = {
        'trinity_pattern': {
            'search_terms': [
                r'tb_\w+',  # Trinity base tables
                r'v_\w+',   # Trinity views
                r'tv_\w+',  # Trinity computed views
                'trinity pattern',
                'trinity identifier',
                'table naming',
            ],
            'files': ['docs/database/*.md', 'docs/core/*.md', 'docs/patterns/*.md'],
        },
        'table_naming': {
            'search_terms': [
                'table naming',
                'naming convention',
                'CREATE TABLE',
                'recommended.*table',
            ],
            'files': ['docs/database/*.md', 'docs/core/*.md'],
        },
        'security_profiles': {
            'search_terms': [
                'STANDARD.*profile',
                'REGULATED.*profile',
                'RESTRICTED.*profile',
                'SecurityProfile',
            ],
            'files': ['docs/security-compliance/*.md', 'docs/features/security-*.md'],
        },
        'mutation_cascade': {
            'search_terms': [
                'CASCADE',
                'enable_cascade',
                'cascade.*mutation',
            ],
            'files': ['docs/mutations/*.md', 'docs/features/*.md', 'docs/guides/*.md'],
        },
    }

    def __init__(self, docs_root: Path, verbose: bool = False):
        self.docs_root = docs_root
        self.verbose = verbose
        self.report = ContradictionReport()

    def search_topic_mentions(self, topic: str, search_terms: List[str], file_patterns: List[str]) -> Dict[str, List[TopicMention]]:
        """Search for mentions of a topic across documentation"""
        mentions_by_term = defaultdict(list)

        # Get all files matching patterns
        files_to_scan = set()
        for pattern in file_patterns:
            files_to_scan.update(self.docs_root.glob(pattern))

        for file_path in sorted(files_to_scan):
            try:
                content = file_path.read_text(encoding='utf-8')
                lines = content.split('\n')

                for line_num, line in enumerate(lines, 1):
                    for term in search_terms:
                        if re.search(term, line, re.IGNORECASE):
                            # Get context (3 lines before and after)
                            context_start = max(0, line_num - 4)
                            context_end = min(len(lines), line_num + 3)
                            context = '\n'.join(lines[context_start:context_end])

                            mention = TopicMention(
                                file_path=file_path,
                                line_number=line_num,
                                context=context,
                                full_line=line.strip()
                            )
                            mentions_by_term[term].append(mention)

            except Exception as e:
                if self.verbose:
                    print(f"Error reading {file_path}: {e}")

        return mentions_by_term

    def check_trinity_pattern_consistency(self) -> List[Contradiction]:
        """Check for contradictions in trinity pattern explanations"""
        contradictions = []

        if self.verbose:
            print("🔍 Checking trinity pattern consistency...")

        # Search for anti-patterns (simple table names in recommendations)
        simple_table_pattern = r'CREATE TABLE (users|posts|comments|products|orders)(?!\w)'

        all_files = list(self.docs_root.rglob('*.md'))
        problematic_files = []

        for file_path in all_files:
            # Skip example files and migration guides (they're allowed to show "before" state)
            if 'example' in str(file_path).lower() or 'migration' in str(file_path).lower():
                continue

            try:
                content = file_path.read_text(encoding='utf-8')
                lines = content.split('\n')

                for line_num, line in enumerate(lines, 1):
                    if re.search(simple_table_pattern, line, re.IGNORECASE):
                        # Check if this is in a "before" or "don't do this" context
                        context_start = max(0, line_num - 5)
                        context_end = min(len(lines), line_num + 2)
                        context = '\n'.join(lines[context_start:context_end]).lower()

                        # Skip if explicitly marked as "before" or "bad" example
                        if any(marker in context for marker in ['before:', '❌', 'bad:', 'avoid:', 'don\'t:', 'incorrect:', '# wrong']):
                            continue

                        problematic_files.append(TopicMention(
                            file_path=file_path,
                            line_number=line_num,
                            context='\n'.join(lines[context_start:context_end]),
                            full_line=line.strip()
                        ))

            except Exception as e:
                if self.verbose:
                    print(f"Error checking {file_path}: {e}")

        if problematic_files:
            contradictions.append(Contradiction(
                topic='trinity_pattern',
                description='Files using simple table names (users, posts, etc.) without trinity pattern context',
                mentions=problematic_files,
                severity='high'
            ))

        return contradictions

    def check_naming_recommendations(self) -> List[Contradiction]:
        """Check that naming convention docs consistently recommend trinity pattern"""
        contradictions = []

        if self.verbose:
            print("🔍 Checking naming convention recommendations...")

        # Find the authoritative naming docs
        naming_docs = [
            self.docs_root / 'database' / 'TABLE_NAMING_CONVENTIONS.md',
            self.docs_root / 'database' / 'trinity_identifiers.md',
            self.docs_root / 'core' / 'trinity-pattern.md',
        ]

        recommendations = {}

        for doc in naming_docs:
            if not doc.exists():
                continue

            try:
                content = doc.read_text(encoding='utf-8')
                lines = content.split('\n')

                # Look for recommendation keywords
                for line_num, line in enumerate(lines, 1):
                    lower_line = line.lower()
                    if any(keyword in lower_line for keyword in ['recommend', 'should', 'best practice', 'prefer']):
                        if any(pattern in lower_line for pattern in ['simple', 'users', 'posts', 'without prefix']):
                            # Found recommendation for simple naming
                            recommendations[doc] = ('simple', line_num, line.strip())
                        elif any(pattern in lower_line for pattern in ['tb_', 'v_', 'tv_', 'trinity']):
                            # Found recommendation for trinity
                            recommendations[doc] = ('trinity', line_num, line.strip())

            except Exception as e:
                if self.verbose:
                    print(f"Error checking {doc}: {e}")

        # Check for contradictions
        if len(set(rec[0] for rec in recommendations.values())) > 1:
            mentions = [
                TopicMention(
                    file_path=doc,
                    line_number=rec[1],
                    context=rec[2],
                    full_line=rec[2]
                )
                for doc, rec in recommendations.items()
            ]
            contradictions.append(Contradiction(
                topic='table_naming',
                description='Inconsistent table naming recommendations across authoritative docs',
                mentions=mentions,
                severity='critical'
            ))

        return contradictions

    def check_security_profiles(self) -> List[Contradiction]:
        """Check security profile descriptions for consistency"""
        contradictions = []

        if self.verbose:
            print("🔍 Checking security profile descriptions...")

        # Define what each profile should enable
        expected_features = {
            'STANDARD': {'basic audit', 'https', 'sql injection'},
            'REGULATED': {'cryptographic audit', 'kms', 'row-level security', 'slsa'},
            'RESTRICTED': {'field-level encryption', 'multi-factor', 'zero-trust'},
        }

        # Search for profile descriptions
        security_files = list((self.docs_root / 'security-compliance').glob('*.md')) if (self.docs_root / 'security-compliance').exists() else []

        profile_descriptions = defaultdict(list)

        for file_path in security_files:
            try:
                content = file_path.read_text(encoding='utf-8')
                lines = content.split('\n')

                current_profile = None
                for line_num, line in enumerate(lines, 1):
                    # Detect profile sections
                    if re.search(r'##\s+(STANDARD|REGULATED|RESTRICTED)\s+Profile', line, re.IGNORECASE):
                        current_profile = re.search(r'(STANDARD|REGULATED|RESTRICTED)', line, re.IGNORECASE).group(1).upper()

                    if current_profile:
                        profile_descriptions[current_profile].append((file_path, line_num, line))

            except Exception as e:
                if self.verbose:
                    print(f"Error checking {file_path}: {e}")

        # TODO: Analyze descriptions for contradictions
        # For now, just report if profiles are mentioned

        return contradictions

    def detect_all(self) -> ContradictionReport:
        """Run all contradiction checks"""
        print("🔍 Checking for contradictions in FraiseQL documentation...")
        print()

        # Count files
        self.report.files_scanned = len(list(self.docs_root.rglob('*.md')))

        # Run all checks
        all_contradictions = []
        all_contradictions.extend(self.check_trinity_pattern_consistency())
        all_contradictions.extend(self.check_naming_recommendations())
        all_contradictions.extend(self.check_security_profiles())

        self.report.contradictions = all_contradictions
        self.report.topics_checked = ['trinity_pattern', 'table_naming', 'security_profiles']

        return self.report

    def print_report(self) -> None:
        """Print contradiction report"""
        print("=" * 80)
        print("CONTRADICTION DETECTION REPORT")
        print("=" * 80)
        print(f"Files Scanned:        {self.report.files_scanned}")
        print(f"Topics Checked:       {len(self.report.topics_checked)}")
        print()
        print(f"Critical Issues:      {self.report.critical_count}")
        print(f"High Priority:        {self.report.high_count}")
        print(f"Medium Priority:      {self.report.medium_count}")
        print(f"Total Issues:         {self.report.total_count}")
        print()

        if self.report.contradictions:
            print("=" * 80)
            print("CONTRADICTIONS FOUND")
            print("=" * 80)

            for contradiction in self.report.contradictions:
                severity_icon = {
                    'critical': '🔴',
                    'high': '🟠',
                    'medium': '🟡',
                    'low': '🟢'
                }.get(contradiction.severity, '⚪')

                print(f"\n{severity_icon} [{contradiction.severity.upper()}] {contradiction.topic}")
                print(f"   {contradiction.description}")
                print(f"   Found in {len(contradiction.mentions)} locations:")

                for mention in contradiction.mentions[:5]:  # Show first 5
                    rel_path = mention.file_path.relative_to(self.docs_root)
                    print(f"   - {rel_path}:{mention.line_number}")
                    print(f"     {mention.full_line[:100]}")

                if len(contradiction.mentions) > 5:
                    print(f"   ... and {len(contradiction.mentions) - 5} more")

        else:
            print("✅ No contradictions found!")

        print("\n" + "=" * 80)

    def save_report(self, output_file: Path) -> None:
        """Save report to file"""
        with open(output_file, 'w') as f:
            f.write("Contradiction Detection Report\n")
            f.write("=" * 80 + "\n\n")
            f.write(f"Files Scanned: {self.report.files_scanned}\n")
            f.write(f"Topics Checked: {len(self.report.topics_checked)}\n")
            f.write(f"Critical Issues: {self.report.critical_count}\n")
            f.write(f"High Priority: {self.report.high_count}\n")
            f.write(f"Medium Priority: {self.report.medium_count}\n")
            f.write(f"Total Issues: {self.report.total_count}\n\n")

            if self.report.contradictions:
                f.write("Contradictions Found:\n")
                f.write("-" * 80 + "\n\n")

                for contradiction in self.report.contradictions:
                    f.write(f"\n[{contradiction.severity.upper()}] {contradiction.topic}\n")
                    f.write(f"Description: {contradiction.description}\n")
                    f.write(f"Locations ({len(contradiction.mentions)}):\n\n")

                    for mention in contradiction.mentions:
                        rel_path = mention.file_path.relative_to(self.docs_root)
                        f.write(f"  File: {rel_path}:{mention.line_number}\n")
                        f.write(f"  Line: {mention.full_line}\n")
                        f.write(f"  Context:\n")
                        for ctx_line in mention.context.split('\n')[:3]:
                            f.write(f"    {ctx_line}\n")
                        f.write("\n")
            else:
                f.write("✅ No contradictions found!\n")

        print(f"\n📄 Report saved to: {output_file}")


def main():
    parser = argparse.ArgumentParser(description="Detect contradictions in FraiseQL documentation")
    parser.add_argument(
        '--verbose', '-v',
        action='store_true',
        help='Verbose output'
    )
    parser.add_argument(
        '--report',
        type=Path,
        default=Path('.phases/docs-review/contradiction_report.txt'),
        help='Output file for report'
    )
    args = parser.parse_args()

    # Find repository root
    repo_root = Path(__file__).parent.parent
    docs_root = repo_root / 'docs'

    if not docs_root.exists():
        print(f"Error: Documentation directory not found: {docs_root}")
        return 1

    # Run detection
    detector = ContradictionDetector(docs_root, verbose=args.verbose)
    detector.detect_all()
    detector.print_report()

    # Save report
    args.report.parent.mkdir(parents=True, exist_ok=True)
    detector.save_report(args.report)

    # Exit with error if critical or high contradictions found
    return 1 if (detector.report.critical_count > 0 or detector.report.high_count > 0) else 0


if __name__ == '__main__':
    exit(main())
