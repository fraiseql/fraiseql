#!/usr/bin/env python3
"""
Comprehensive Markdown Link Verification Script
Scans all markdown files and verifies internal links
"""

import re
import os
from pathlib import Path
from collections import defaultdict, Counter
from typing import List, Dict, Tuple, Set
from urllib.parse import urlparse
import sys


class LinkVerifier:
    def __init__(self, root_dir: str, exclude_dirs: List[str]):
        self.root_dir = Path(root_dir).resolve()
        self.exclude_dirs = exclude_dirs
        self.stats = {
            'files_scanned': 0,
            'total_links': 0,
            'links_by_type': Counter(),
            'broken_links': []
        }
        # Regex to extract markdown links: [text](url)
        self.link_pattern = re.compile(r'\[([^\]]+)\]\(([^)]+)\)')

    def should_exclude_path(self, path: Path) -> bool:
        """Check if path should be excluded based on exclude_dirs"""
        path_str = str(path.relative_to(self.root_dir))
        for exclude in self.exclude_dirs:
            if path_str.startswith(exclude) or f'/{exclude}/' in f'/{path_str}/':
                return True
        return False

    def find_markdown_files(self) -> List[Path]:
        """Find all markdown files, excluding specified directories"""
        md_files = []
        for md_file in self.root_dir.rglob('*.md'):
            if not self.should_exclude_path(md_file):
                md_files.append(md_file)
        return sorted(md_files)

    def categorize_link(self, url: str) -> str:
        """Categorize a link by its type"""
        url = url.strip()

        # Empty link
        if not url:
            return 'empty'

        # Anchor only (#section)
        if url.startswith('#'):
            return 'anchor'

        # External URLs (http:// or https://)
        if url.startswith(('http://', 'https://')):
            # Check if it's a GitHub URL for this repo
            if 'github.com/fraiseql/fraiseql' in url:
                return 'github-repo'
            return 'external'

        # Mailto links
        if url.startswith('mailto:'):
            return 'mailto'

        # Directory links (ending with /)
        if url.endswith('/'):
            return 'directory'

        # Absolute paths (starting with /)
        if url.startswith('/'):
            return 'absolute'

        # Relative paths (./ or ../ or just filename)
        return 'relative'

    def resolve_link_path(self, link_url: str, source_file: Path) -> Tuple[Path, str]:
        """
        Resolve a link URL to an actual file path
        Returns: (resolved_path, anchor)
        """
        # Strip anchor if present
        anchor = ''
        if '#' in link_url:
            link_url, anchor = link_url.split('#', 1)

        link_url = link_url.strip()

        # Get the directory containing the source file
        source_dir = source_file.parent

        # Resolve relative path
        if link_url.startswith('/'):
            # Absolute path from repository root
            resolved = self.root_dir / link_url.lstrip('/')
        else:
            # Relative path from source file directory
            resolved = source_dir / link_url

        # Normalize the path
        try:
            resolved = resolved.resolve()
        except (OSError, RuntimeError):
            pass  # Path might not exist, we'll check that later

        return resolved, anchor

    def verify_github_repo_link(self, url: str) -> Tuple[bool, str]:
        """
        Verify a GitHub URL that references this repository
        Returns: (is_valid, error_message)
        """
        import re

        # Extract the file path from GitHub URL
        # Pattern: https://github.com/fraiseql/fraiseql/(blob|tree)/(main|dev)/path/to/file
        pattern = r'github\.com/fraiseql/fraiseql/(?:blob|tree)/(?:main|dev)/(.+?)(?:#.*)?$'
        match = re.search(pattern, url)

        if not match:
            # Could be other GitHub URLs like releases, actions, etc.
            # These are external and don't need file verification
            return True, ""

        file_path = match.group(1)

        # Resolve the path relative to repository root
        resolved_path = self.root_dir / file_path

        # Check if the path exists
        if not resolved_path.exists():
            return False, f"GitHub URL references non-existent path: {file_path}"

        return True, ""

    def verify_directory_link(self, link_url: str, source_file: Path) -> Tuple[bool, str]:
        """
        Verify that a directory link points to an existing directory
        Returns: (is_valid, error_message)
        """
        # Remove trailing slash for path resolution
        dir_url = link_url.rstrip('/')
        resolved_path, anchor = self.resolve_link_path(dir_url, source_file)

        # Check if the path exists
        if not resolved_path.exists():
            return False, f"Directory not found: {resolved_path}"

        # Check if it's a directory
        if not resolved_path.is_dir():
            return False, f"Path is not a directory: {resolved_path}"

        return True, ""

    def verify_internal_link(self, link_url: str, source_file: Path) -> Tuple[bool, str]:
        """
        Verify that an internal link points to an existing file
        Returns: (is_valid, error_message)
        """
        resolved_path, anchor = self.resolve_link_path(link_url, source_file)

        # Check if the path exists
        if not resolved_path.exists():
            return False, f"File not found: {resolved_path}"

        # Check if it's a file (not a directory)
        if not resolved_path.is_file():
            return False, f"Path is not a file: {resolved_path}"

        # If there's an anchor, we could verify it exists in the file
        # but that would require parsing markdown headers - skipping for now

        return True, ""

    def extract_links_from_file(self, file_path: Path) -> List[Dict]:
        """Extract all links from a markdown file"""
        links = []

        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()

            # Find all links with line numbers
            for line_num, line in enumerate(content.split('\n'), start=1):
                for match in self.link_pattern.finditer(line):
                    link_text = match.group(1)
                    link_url = match.group(2)

                    links.append({
                        'file': file_path,
                        'line': line_num,
                        'text': link_text,
                        'url': link_url,
                        'type': self.categorize_link(link_url)
                    })
        except Exception as e:
            print(f"Error reading {file_path}: {e}", file=sys.stderr)

        return links

    def verify_all_links(self):
        """Main verification process"""
        print(f"Scanning markdown files in: {self.root_dir}")
        print(f"Excluding directories: {', '.join(self.exclude_dirs)}\n")

        # Find all markdown files
        md_files = self.find_markdown_files()
        self.stats['files_scanned'] = len(md_files)

        print(f"Found {len(md_files)} markdown files\n")

        # Process each file
        all_links = []
        for md_file in md_files:
            links = self.extract_links_from_file(md_file)
            all_links.extend(links)
            if links:
                print(f"  {md_file.relative_to(self.root_dir)}: {len(links)} links")

        self.stats['total_links'] = len(all_links)
        print(f"\nTotal links found: {len(all_links)}\n")

        # Categorize and verify links
        print("Verifying links...\n")

        for link in all_links:
            link_type = link['type']
            self.stats['links_by_type'][link_type] += 1

            # Verify internal links (relative and absolute)
            if link_type in ('relative', 'absolute'):
                is_valid, error_msg = self.verify_internal_link(link['url'], link['file'])

                if not is_valid:
                    self.stats['broken_links'].append({
                        'file': link['file'],
                        'line': link['line'],
                        'text': link['text'],
                        'url': link['url'],
                        'type': link_type,
                        'error': error_msg
                    })

            # Verify directory links
            elif link_type == 'directory':
                is_valid, error_msg = self.verify_directory_link(link['url'], link['file'])

                if not is_valid:
                    self.stats['broken_links'].append({
                        'file': link['file'],
                        'line': link['line'],
                        'text': link['text'],
                        'url': link['url'],
                        'type': link_type,
                        'error': error_msg
                    })

            # Verify GitHub repository links
            elif link_type == 'github-repo':
                is_valid, error_msg = self.verify_github_repo_link(link['url'])

                if not is_valid:
                    self.stats['broken_links'].append({
                        'file': link['file'],
                        'line': link['line'],
                        'text': link['text'],
                        'url': link['url'],
                        'type': link_type,
                        'error': error_msg
                    })

    def generate_report(self, output_file: str):
        """Generate a detailed markdown report"""
        with open(output_file, 'w', encoding='utf-8') as f:
            f.write("# FraiseQL Link Verification Report\n\n")
            f.write(f"**Repository**: {self.root_dir}\n")
            f.write(f"**Generated**: {self._get_timestamp()}\n\n")

            # Summary Statistics
            f.write("## Summary Statistics\n\n")
            f.write(f"- **Files Scanned**: {self.stats['files_scanned']}\n")
            f.write(f"- **Total Links**: {self.stats['total_links']}\n")
            f.write(f"- **Broken Links**: {len(self.stats['broken_links'])}\n\n")

            # Links by Type
            f.write("## Links by Type\n\n")
            f.write("| Type | Count |\n")
            f.write("|------|-------|\n")
            for link_type, count in sorted(self.stats['links_by_type'].items()):
                f.write(f"| {link_type} | {count} |\n")
            f.write("\n")

            # Broken Links
            if self.stats['broken_links']:
                f.write("## Broken Links\n\n")
                f.write(f"Found **{len(self.stats['broken_links'])}** broken links:\n\n")

                # Group by file
                by_file = defaultdict(list)
                for broken in self.stats['broken_links']:
                    by_file[broken['file']].append(broken)

                for file_path in sorted(by_file.keys()):
                    rel_path = file_path.relative_to(self.root_dir)
                    f.write(f"### {rel_path}\n\n")

                    for broken in by_file[file_path]:
                        f.write(f"**Line {broken['line']}**\n")
                        f.write(f"- Link Text: `{broken['text']}`\n")
                        f.write(f"- Link URL: `{broken['url']}`\n")
                        f.write(f"- Type: `{broken['type']}`\n")
                        f.write(f"- Error: {broken['error']}\n\n")
            else:
                f.write("## Broken Links\n\n")
                f.write("No broken links found! All internal links are valid.\n\n")

            # External Links Summary (not verified but listed)
            external_count = self.stats['links_by_type'].get('external', 0)
            if external_count > 0:
                f.write("## External Links\n\n")
                f.write(f"Found **{external_count}** external links (not verified).\n\n")
                f.write("External link verification requires HTTP requests and is not included in this report.\n\n")

    def _get_timestamp(self):
        """Get current timestamp"""
        from datetime import datetime
        return datetime.now().strftime("%Y-%m-%d %H:%M:%S")

    def print_summary(self):
        """Print summary to console"""
        print("\n" + "="*70)
        print("VERIFICATION SUMMARY")
        print("="*70)
        print(f"\nFiles Scanned: {self.stats['files_scanned']}")
        print(f"Total Links: {self.stats['total_links']}")
        print(f"\nLinks by Type:")
        for link_type, count in sorted(self.stats['links_by_type'].items()):
            print(f"  - {link_type}: {count}")

        print(f"\nBroken Links: {len(self.stats['broken_links'])}")

        if self.stats['broken_links']:
            print("\nBROKEN LINKS DETAILS:")
            print("-" * 70)
            for broken in self.stats['broken_links']:
                rel_path = broken['file'].relative_to(self.root_dir)
                print(f"\nFile: {rel_path}")
                print(f"Line: {broken['line']}")
                print(f"Text: {broken['text']}")
                print(f"URL: {broken['url']}")
                print(f"Error: {broken['error']}")
        else:
            print("\nAll internal links are valid!")


def main():
    # Configuration
    root_dir = "/home/lionel/code/fraiseql"
    exclude_dirs = ['.venv', 'venv', 'archive', 'dev/audits', 'node_modules']
    output_file = "/tmp/fraiseql_link_verification_report.md"

    # Verify repository exists
    if not Path(root_dir).exists():
        print(f"Error: Directory not found: {root_dir}", file=sys.stderr)
        sys.exit(1)

    # Create verifier and run
    verifier = LinkVerifier(root_dir, exclude_dirs)
    verifier.verify_all_links()
    verifier.generate_report(output_file)
    verifier.print_summary()

    print(f"\n\nDetailed report written to: {output_file}")
    print("="*70 + "\n")


if __name__ == "__main__":
    main()
