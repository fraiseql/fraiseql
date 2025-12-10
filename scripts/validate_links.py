#!/usr/bin/env python3
"""
Link Validation Script for FraiseQL Documentation

Validates all links in markdown files:
- Internal links (relative paths to other docs)
- Anchor links (headings within documents)
- External links (HTTP/HTTPS URLs)

Usage:
    python scripts/validate_links.py
    python scripts/validate_links.py --check-external
    python scripts/validate_links.py --fix
"""

import argparse
import re
import sys
import urllib.request
from pathlib import Path
from typing import List, Tuple, Dict, Set
from dataclasses import dataclass, field
from urllib.parse import urlparse, unquote
import time

@dataclass
class LinkIssue:
    """Represents a broken link issue"""
    file_path: Path
    line_number: int
    link_text: str
    link_target: str
    issue_type: str  # 'broken_file', 'broken_anchor', 'broken_external', 'malformed'
    suggestion: str = ""


@dataclass
class ValidationReport:
    """Validation results"""
    total_files: int = 0
    total_links: int = 0
    internal_links: int = 0
    external_links: int = 0
    anchor_links: int = 0
    issues: List[LinkIssue] = field(default_factory=list)

    @property
    def broken_count(self) -> int:
        return len(self.issues)

    @property
    def success_rate(self) -> float:
        if self.total_links == 0:
            return 100.0
        return ((self.total_links - self.broken_count) / self.total_links) * 100


class LinkValidator:
    """Validates markdown links"""

    # Pattern to match markdown links: [text](url) or [text](url "title")
    LINK_PATTERN = re.compile(r'\[([^\]]+)\]\(([^)]+)\)')

    # Pattern to match headings for anchor validation
    HEADING_PATTERN = re.compile(r'^#+\s+(.+)$', re.MULTILINE)

    def __init__(self, docs_root: Path, check_external: bool = False):
        self.docs_root = docs_root
        self.check_external = check_external
        self.report = ValidationReport()
        self.heading_cache: Dict[Path, Set[str]] = {}

    def slugify_heading(self, heading: str) -> str:
        """Convert heading text to GitHub-style anchor slug"""
        # Remove markdown formatting
        heading = re.sub(r'`([^`]+)`', r'\1', heading)  # Remove code blocks
        heading = re.sub(r'\*\*([^*]+)\*\*', r'\1', heading)  # Remove bold
        heading = re.sub(r'\*([^*]+)\*', r'\1', heading)  # Remove italic

        # Convert to lowercase and replace spaces/special chars with hyphens
        slug = heading.lower()
        slug = re.sub(r'[^\w\s-]', '', slug)  # Remove special chars except hyphens
        slug = re.sub(r'[-\s]+', '-', slug)  # Replace spaces/multiple hyphens with single hyphen
        slug = slug.strip('-')  # Remove leading/trailing hyphens

        return slug

    def extract_headings(self, file_path: Path) -> Set[str]:
        """Extract all heading anchors from a markdown file"""
        if file_path in self.heading_cache:
            return self.heading_cache[file_path]

        try:
            content = file_path.read_text(encoding='utf-8')
            headings = self.HEADING_PATTERN.findall(content)
            anchors = {self.slugify_heading(h) for h in headings}
            self.heading_cache[file_path] = anchors
            return anchors
        except Exception as e:
            print(f"Warning: Could not read {file_path}: {e}")
            return set()

    def validate_internal_link(self, source_file: Path, link_target: str) -> Tuple[bool, str]:
        """Validate an internal link (relative path)"""
        # Split anchor from path
        if '#' in link_target:
            path_part, anchor_part = link_target.split('#', 1)
        else:
            path_part, anchor_part = link_target, None

        # Resolve the target file path
        if path_part:
            # Relative to current file
            target_path = (source_file.parent / path_part).resolve()
        else:
            # Anchor in same file
            target_path = source_file

        # Check if file exists
        if not target_path.exists():
            return False, f"File not found: {target_path}"

        # If there's an anchor, validate it exists in the target file
        if anchor_part:
            anchors = self.extract_headings(target_path)
            if anchor_part not in anchors:
                return False, f"Anchor '#{anchor_part}' not found in {target_path.name}"

        return True, ""

    def validate_external_link(self, url: str) -> Tuple[bool, str]:
        """Validate an external HTTP/HTTPS link"""
        try:
            # Add timeout and user agent to avoid being blocked
            req = urllib.request.Request(
                url,
                headers={'User-Agent': 'FraiseQL-Link-Validator/1.0'}
            )
            with urllib.request.urlopen(req, timeout=10) as response:
                if response.status == 200:
                    return True, ""
                else:
                    return False, f"HTTP {response.status}"
        except urllib.error.HTTPError as e:
            return False, f"HTTP {e.code}"
        except urllib.error.URLError as e:
            return False, f"URL Error: {e.reason}"
        except Exception as e:
            return False, f"Error: {str(e)}"

    def validate_file(self, file_path: Path) -> None:
        """Validate all links in a markdown file"""
        self.report.total_files += 1

        try:
            content = file_path.read_text(encoding='utf-8')
        except Exception as e:
            print(f"Error reading {file_path}: {e}")
            return

        # Find all links
        for match in self.LINK_PATTERN.finditer(content):
            link_text = match.group(1)
            link_target = match.group(2).strip()

            # Get line number
            line_number = content[:match.start()].count('\n') + 1

            self.report.total_links += 1

            # Skip mailto links
            if link_target.startswith('mailto:'):
                continue

            # External link
            if link_target.startswith(('http://', 'https://')):
                self.report.external_links += 1
                if self.check_external:
                    is_valid, error = self.validate_external_link(link_target)
                    if not is_valid:
                        self.report.issues.append(LinkIssue(
                            file_path=file_path,
                            line_number=line_number,
                            link_text=link_text,
                            link_target=link_target,
                            issue_type='broken_external',
                            suggestion=error
                        ))
                    # Rate limit external checks
                    time.sleep(0.5)
                continue

            # Anchor link (starts with #)
            if link_target.startswith('#'):
                self.report.anchor_links += 1
                anchor = link_target[1:]  # Remove leading #
                anchors = self.extract_headings(file_path)
                if anchor not in anchors:
                    self.report.issues.append(LinkIssue(
                        file_path=file_path,
                        line_number=line_number,
                        link_text=link_text,
                        link_target=link_target,
                        issue_type='broken_anchor',
                        suggestion=f"Anchor not found. Available: {', '.join(sorted(anchors)[:5])}"
                    ))
                continue

            # Internal relative link
            self.report.internal_links += 1
            is_valid, error = self.validate_internal_link(file_path, link_target)
            if not is_valid:
                self.report.issues.append(LinkIssue(
                    file_path=file_path,
                    line_number=line_number,
                    link_text=link_text,
                    link_target=link_target,
                    issue_type='broken_file',
                    suggestion=error
                ))

    def validate_all(self) -> ValidationReport:
        """Validate all markdown files"""
        print(f"🔍 Scanning for markdown files in {self.docs_root}...")
        md_files = sorted(self.docs_root.rglob("*.md"))

        print(f"📄 Found {len(md_files)} markdown files")
        print(f"{'🌐 Checking external links' if self.check_external else '⚠️  Skipping external links (use --check-external to enable)'}")
        print()

        for i, md_file in enumerate(md_files, 1):
            rel_path = md_file.relative_to(self.docs_root)
            print(f"[{i}/{len(md_files)}] Validating {rel_path}...", end='\r')
            self.validate_file(md_file)

        print()  # New line after progress
        return self.report

    def print_report(self) -> None:
        """Print validation report"""
        print("\n" + "=" * 80)
        print("LINK VALIDATION REPORT")
        print("=" * 80)
        print(f"Files Scanned:     {self.report.total_files}")
        print(f"Total Links:       {self.report.total_links}")
        print(f"  - Internal:      {self.report.internal_links}")
        print(f"  - External:      {self.report.external_links} {'(checked)' if self.check_external else '(skipped)'}")
        print(f"  - Anchors:       {self.report.anchor_links}")
        print()
        print(f"Broken Links:      {self.report.broken_count}")
        print(f"Success Rate:      {self.report.success_rate:.1f}%")
        print()

        if self.report.issues:
            print("=" * 80)
            print("BROKEN LINKS")
            print("=" * 80)

            # Group by issue type
            by_type = {}
            for issue in self.report.issues:
                by_type.setdefault(issue.issue_type, []).append(issue)

            for issue_type, issues in sorted(by_type.items()):
                print(f"\n{issue_type.upper().replace('_', ' ')} ({len(issues)}):")
                print("-" * 80)

                for issue in issues:
                    rel_path = issue.file_path.relative_to(self.docs_root)
                    print(f"\n  File: {rel_path}:{issue.line_number}")
                    print(f"  Link: [{issue.link_text}]({issue.link_target})")
                    if issue.suggestion:
                        print(f"  Issue: {issue.suggestion}")
        else:
            print("✅ No broken links found!")

        print("\n" + "=" * 80)

    def save_report(self, output_file: Path) -> None:
        """Save report to file"""
        with open(output_file, 'w') as f:
            f.write("Link Validation Report\n")
            f.write("=" * 80 + "\n\n")
            f.write(f"Files Scanned: {self.report.total_files}\n")
            f.write(f"Total Links: {self.report.total_links}\n")
            f.write(f"  - Internal: {self.report.internal_links}\n")
            f.write(f"  - External: {self.report.external_links}\n")
            f.write(f"  - Anchors: {self.report.anchor_links}\n")
            f.write(f"Broken Links: {self.report.broken_count}\n")
            f.write(f"Success Rate: {self.report.success_rate:.1f}%\n\n")

            if self.report.issues:
                f.write("Broken Links:\n")
                f.write("-" * 80 + "\n\n")

                for issue in self.report.issues:
                    rel_path = issue.file_path.relative_to(self.docs_root)
                    f.write(f"File: {rel_path}:{issue.line_number}\n")
                    f.write(f"Link: [{issue.link_text}]({issue.link_target})\n")
                    f.write(f"Type: {issue.issue_type}\n")
                    if issue.suggestion:
                        f.write(f"Issue: {issue.suggestion}\n")
                    f.write("\n")
            else:
                f.write("✅ No broken links found!\n")

        print(f"\n📄 Report saved to: {output_file}")


def main():
    parser = argparse.ArgumentParser(description="Validate links in FraiseQL documentation")
    parser.add_argument(
        '--check-external',
        action='store_true',
        help='Check external HTTP/HTTPS links (slow, may have false positives)'
    )
    parser.add_argument(
        '--report',
        type=Path,
        default=Path('.phases/docs-review/link_validation_report.txt'),
        help='Output file for report (default: .phases/docs-review/link_validation_report.txt)'
    )
    args = parser.parse_args()

    # Find repository root
    repo_root = Path(__file__).parent.parent
    docs_root = repo_root / 'docs'

    if not docs_root.exists():
        print(f"Error: Documentation directory not found: {docs_root}")
        sys.exit(1)

    # Run validation
    validator = LinkValidator(docs_root, check_external=args.check_external)
    validator.validate_all()
    validator.print_report()

    # Save report
    args.report.parent.mkdir(parents=True, exist_ok=True)
    validator.save_report(args.report)

    # Exit with error code if broken links found
    sys.exit(1 if validator.report.broken_count > 0 else 0)


if __name__ == '__main__':
    main()
