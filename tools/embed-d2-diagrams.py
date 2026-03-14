#!/usr/bin/env python3
"""
D2 Diagram Embedder: Convert D2 diagrams to SVGs and embed in markdown

Usage:
    python3 embed-d2-diagrams.py docs/

This script finds all D2 code blocks in markdown files, generates SVGs using d2,
and embeds them with collapsible details tags for GitHub rendering.
"""

import os
import re
import subprocess
import sys
from pathlib import Path


def has_d2_blocks(content: str) -> bool:
    """Check if markdown contains D2 blocks."""
    return '```d2' in content


def extract_d2_blocks(content: str) -> list[tuple[int, str]]:
    """Extract D2 code blocks with their positions."""
    blocks = []
    pattern = r'```d2\n(.*?)\n```'
    for match in re.finditer(pattern, content, re.DOTALL):
        blocks.append((match.start(), match.group(1)))
    return blocks


def d2_to_svg(d2_code: str) -> str | None:
    """Convert D2 code to SVG using the d2 command."""
    try:
        result = subprocess.run(
            ['d2', '-'],
            input=d2_code,
            capture_output=True,
            text=True,
            timeout=10
        )
        if result.returncode == 0:
            return result.stdout.strip()
        else:
            print(f"⚠️  D2 error: {result.stderr}", file=sys.stderr)
            return None
    except FileNotFoundError:
        print("❌ d2 command not found. Install with: curl -fsSL https://d2lang.com/install.sh | sh -s --", file=sys.stderr)
        return None
    except subprocess.TimeoutExpired:
        print("⚠️  D2 generation timeout (>10s)", file=sys.stderr)
        return None
    except Exception as e:
        print(f"❌ Error generating SVG: {e}", file=sys.stderr)
        return None


def embed_diagrams(content: str) -> str:
    """Replace D2 blocks with embedded SVG + source."""
    def replace_block(match):
        d2_code = match.group(1)
        svg = d2_to_svg(d2_code)

        if svg:
            # Embed SVG in collapsible details
            return f"""<details>
<summary>📊 View Diagram (click to expand)</summary>

{svg}

</details>

**Diagram Source:**
```d2
{d2_code}
```"""
        else:
            # If SVG generation failed, keep original
            return match.group(0)

    pattern = r'```d2\n(.*?)\n```'
    return re.sub(pattern, replace_block, content, flags=re.DOTALL)


def process_file(filepath: Path) -> bool:
    """Process a single markdown file."""
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()

        if not has_d2_blocks(content):
            return False

        print(f"📄 Processing: {filepath}")

        new_content = embed_diagrams(content)

        if new_content != content:
            with open(filepath, 'w', encoding='utf-8') as f:
                f.write(new_content)
            print(f"   ✅ Embedded {len(extract_d2_blocks(content))} D2 diagram(s)")
            return True
        else:
            print(f"   ⏭️  No changes needed")
            return False

    except Exception as e:
        print(f"❌ Error processing {filepath}: {e}", file=sys.stderr)
        return False


def main():
    """Main entry point."""
    if len(sys.argv) < 2:
        print("Usage: python3 embed-d2-diagrams.py <docs_directory>")
        sys.exit(1)

    docs_dir = Path(sys.argv[1]).resolve()

    if not docs_dir.exists():
        print(f"❌ Directory not found: {docs_dir}", file=sys.stderr)
        sys.exit(1)

    print(f"🔍 Scanning for D2 diagrams in: {docs_dir}\n")

    markdown_files = sorted(docs_dir.glob('**/*.md'))
    if not markdown_files:
        print("⚠️  No markdown files found")
        sys.exit(0)

    processed = 0
    updated = 0

    for filepath in markdown_files:
        if process_file(filepath):
            updated += 1
        processed += 1

    print(f"\n📊 Summary:")
    print(f"   Processed: {processed} files")
    print(f"   Updated: {updated} files")
    print(f"   ✅ Complete!")


if __name__ == '__main__':
    main()
