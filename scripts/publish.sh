#!/bin/bash
# PyPI Publication Script for FraiseQL
# This script helps automate the PyPI publication process

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_step() {
    echo -e "${GREEN}==>${NC} $1"
}

print_error() {
    echo -e "${RED}Error:${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}Warning:${NC} $1"
}

# Check if we're in the right directory
if [ ! -f "pyproject.toml" ]; then
    print_error "pyproject.toml not found. Are you in the project root?"
    exit 1
fi

# Parse command line arguments
SKIP_TESTS=false
USE_TESTPYPI=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-tests)
            SKIP_TESTS=true
            shift
            ;;
        --test-pypi)
            USE_TESTPYPI=true
            shift
            ;;
        *)
            echo "Usage: $0 [--skip-tests] [--test-pypi]"
            echo "  --skip-tests: Skip running tests"
            echo "  --test-pypi: Upload to TestPyPI instead of PyPI"
            exit 1
            ;;
    esac
done

# Step 1: Run tests (unless skipped)
if [ "$SKIP_TESTS" = false ]; then
    print_step "Running tests..."
    if ! make test-fast; then
        print_error "Tests failed. Fix issues before publishing."
        exit 1
    fi
else
    print_warning "Skipping tests (--skip-tests flag used)"
fi

# Step 2: Check code quality
print_step "Running linter..."
if ! make lint; then
    print_error "Linting failed. Run 'make format' to fix issues."
    exit 1
fi

print_step "Running type checker..."
if ! make type-check; then
    print_error "Type checking failed. Fix type issues before publishing."
    exit 1
fi

# Step 3: Clean previous builds
print_step "Cleaning previous builds..."
rm -rf dist/ build/ *.egg-info src/*.egg-info

# Step 4: Build the package
print_step "Building package..."
python -m build

# Step 5: Check the package
print_step "Checking package with twine..."
python -m twine check dist/*

# Step 6: Display package contents
print_step "Package contents:"
echo "Wheel contents:"
unzip -l dist/*.whl | grep -E "\.py$|\.pyi$|\.typed$" | head -20
echo ""
echo "Source distribution size:"
ls -lh dist/*.tar.gz

# Step 7: Confirm before upload
echo ""
if [ "$USE_TESTPYPI" = true ]; then
    echo -e "${YELLOW}Ready to upload to TestPyPI${NC}"
    REPO_ARGS="--repository testpypi"
else
    echo -e "${RED}Ready to upload to PyPI (Production)${NC}"
    REPO_ARGS=""
fi

echo "Files to upload:"
ls -la dist/

echo ""
read -p "Continue with upload? (y/N) " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    print_warning "Upload cancelled"
    exit 0
fi

# Step 8: Upload
print_step "Uploading to PyPI..."
python -m twine upload $REPO_ARGS dist/*

# Step 9: Success message
echo ""
print_step "🎉 Upload complete!"

if [ "$USE_TESTPYPI" = true ]; then
    echo ""
    echo "Test installation with:"
    echo "  pip install --index-url https://test.pypi.org/simple/ --extra-index-url https://pypi.org/simple fraiseql"
else
    echo ""
    echo "Install with:"
    echo "  pip install fraiseql"
    echo ""
    echo "Don't forget to:"
    echo "  1. Create a GitHub release"
    echo "  2. Update the changelog"
    echo "  3. Announce the release"
fi
