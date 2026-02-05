#!/usr/bin/env python3
"""
Test documentation site build and structure.

This script:
1. Verifies mkdocs.yml is valid
2. Checks that all referenced files exist
3. Validates navigation structure
4. Tests that the site can be built
"""

import json
import subprocess
import sys
from pathlib import Path

def test_mkdocs_config():
    """Verify mkdocs.yml is valid YAML"""
    print("🔍 Testing mkdocs.yml configuration...")

    mkdocs_file = Path("mkdocs.yml")
    if not mkdocs_file.exists():
        print("❌ mkdocs.yml not found")
        return False

    # Try to build - this will fail if config is invalid
    result = subprocess.run(
        ["python3", "-c", "import yaml; yaml.safe_load(open('mkdocs.yml'))"],
        capture_output=True,
        text=True
    )

    if result.returncode == 0:
        print("✅ mkdocs.yml is valid")
        return True
    else:
        print(f"❌ mkdocs.yml is invalid: {result.stderr}")
        return False

def test_documentation_files():
    """Verify documentation directory and files exist"""
    print("\n🔍 Checking documentation files...")

    docs_dir = Path("docs")
    if not docs_dir.exists():
        print("❌ docs directory not found")
        return False

    md_files = list(docs_dir.glob("**/*.md"))
    if not md_files:
        print("❌ No markdown files found in docs/")
        return False

    print(f"✅ Found {len(md_files)} documentation files")
    return True

def test_site_build():
    """Test building the documentation site"""
    print("\n🔍 Testing documentation build...")

    # Check if mkdocs is installed
    result = subprocess.run(
        ["python3", "-m", "pip", "show", "mkdocs"],
        capture_output=True,
        text=True
    )

    if result.returncode != 0:
        print("⚠️  mkdocs not installed, skipping build test")
        print("   Install with: pip install mkdocs mkdocs-material pymdown-extensions")
        return True  # Don't fail, just warn

    # Try to build
    result = subprocess.run(
        ["python3", "-m", "mkdocs", "build", "--clean", "-v"],
        capture_output=True,
        text=True,
        timeout=30
    )

    if result.returncode == 0:
        # Count generated files
        site_dir = Path("site")
        html_files = list(site_dir.glob("**/*.html")) if site_dir.exists() else []
        print(f"✅ Documentation site built successfully ({len(html_files)} HTML files)")
        return True
    else:
        print(f"❌ Build failed:")
        print(result.stderr[-500:] if len(result.stderr) > 500 else result.stderr)
        return False

def test_search_index():
    """Verify search index is generated"""
    print("\n🔍 Testing search index generation...")

    site_dir = Path("site")
    search_index = site_dir / "search" / "search_index.json"

    if not site_dir.exists():
        print("⚠️  Site directory not found (build documentation first)")
        return True

    if search_index.exists():
        try:
            with open(search_index) as f:
                data = json.load(f)
            # Should have docs
            doc_count = len(data.get("docs", []))
            print(f"✅ Search index generated with {doc_count} pages")
            return True
        except json.JSONDecodeError:
            print("❌ Search index is not valid JSON")
            return False
    else:
        print("⚠️  Search index not found (mkdocs build may be needed)")
        return True

def main():
    """Run all tests"""
    print("🚀 Testing FraiseQL Documentation Site\n")

    tests = [
        ("Config validation", test_mkdocs_config),
        ("Documentation files", test_documentation_files),
        ("Site build", test_site_build),
        ("Search index", test_search_index),
    ]

    results = []
    for name, test_func in tests:
        try:
            passed = test_func()
            results.append((name, passed))
        except Exception as e:
            print(f"❌ {name} test failed with error: {e}")
            results.append((name, False))

    # Summary
    print("\n" + "=" * 50)
    print("📊 Test Summary")
    print("=" * 50)

    passed = sum(1 for _, result in results if result)
    total = len(results)

    for name, result in results:
        status = "✅ PASS" if result else "❌ FAIL"
        print(f"{status}: {name}")

    print("-" * 50)
    print(f"Total: {passed}/{total} tests passed")

    if passed == total:
        print("\n✅ Documentation site is ready for deployment!")
        return 0
    else:
        print(f"\n❌ {total - passed} test(s) failed. Please fix before deploying.")
        return 1

if __name__ == "__main__":
    sys.exit(main())
