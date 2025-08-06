#!/bin/bash
# Type check using Python 3.12 while project uses Python 3.13

set -e

echo "🔍 Running mypy type checks with Python 3.12..."

# Check if Python 3.12 is installed
if ! command -v python3.12 &> /dev/null; then
    echo "❌ Python 3.12 is not installed. Please install it first:"
    echo "   Ubuntu/Debian: sudo apt install python3.12"
    echo "   macOS: brew install python@3.12"
    echo "   Or use pyenv: pyenv install 3.12"
    exit 1
fi

# Create a temporary virtual environment with Python 3.12
TEMP_VENV=$(mktemp -d)/venv
python3.12 -m venv "$TEMP_VENV"
source "$TEMP_VENV/bin/activate"

# Install the project and mypy
echo "📦 Installing dependencies..."
pip install -q --upgrade pip
pip install -q -e .
pip install -q mypy

# Run mypy
echo "🏃 Running mypy..."
mypy src/fraiseql --ignore-missing-imports --show-error-codes

# Clean up
deactivate
rm -rf "$(dirname "$TEMP_VENV")"

echo "✅ Type checking complete!"
