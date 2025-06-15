#!/bin/bash

# LLM Token Test Suite Setup Script

set -e  # Exit on error

echo "Setting up LLM Token Test Suite..."
echo "=================================="

# Check Python version
PYTHON_VERSION=$(python3 --version 2>&1 | awk '{print $2}')
REQUIRED_VERSION="3.9"

if ! python3 -c "import sys; exit(0 if sys.version_info >= (3, 9) else 1)"; then
    echo "Error: Python 3.9 or higher is required. Found: $PYTHON_VERSION"
    exit 1
fi

echo "✓ Python version: $PYTHON_VERSION"

# Create virtual environment if it doesn't exist
if [ ! -d "venv" ]; then
    echo "Creating virtual environment..."
    python3 -m venv venv
    echo "✓ Virtual environment created"
else
    echo "✓ Virtual environment already exists"
fi

# Activate virtual environment
source venv/bin/activate

# Upgrade pip
echo "Upgrading pip..."
pip install --upgrade pip

# Install core requirements
echo "Installing core dependencies..."
pip install tiktoken httpx pytest pytest-asyncio python-dotenv pydantic rich

# Ask about LLM providers
echo ""
echo "Which LLM providers do you want to use?"
echo "1. OpenAI (GPT models)"
echo "2. Anthropic (Claude models)"
echo "3. Local models (Hugging Face)"
echo "4. All providers"
echo "5. Core only (no LLM providers)"
echo ""
read -p "Enter your choice (1-5): " choice

case $choice in
    1)
        echo "Installing OpenAI..."
        pip install openai
        ;;
    2)
        echo "Installing Anthropic..."
        pip install anthropic
        ;;
    3)
        echo "Installing Hugging Face transformers..."
        pip install transformers torch
        ;;
    4)
        echo "Installing all LLM providers..."
        pip install openai anthropic transformers torch
        ;;
    5)
        echo "Skipping LLM providers..."
        ;;
    *)
        echo "Invalid choice. Installing core only..."
        ;;
esac

# Install remaining dependencies
echo "Installing analysis and visualization tools..."
pip install pandas numpy matplotlib seaborn
pip install ast-grep-py graphql-core

# Install development tools
echo "Installing development tools..."
pip install black ruff mypy

# Create necessary directories
echo "Creating directory structure..."
mkdir -p benchmarks/results
mkdir -p benchmarks/scenarios
mkdir -p benchmarks/generated

# Check for .env file
if [ ! -f "benchmarks/.env" ]; then
    echo ""
    echo "Creating .env file..."
    cat > benchmarks/.env << 'EOF'
# LLM Provider Configuration

# OpenAI
OPENAI_API_KEY=your-openai-api-key-here
OPENAI_MODEL=gpt-4
OPENAI_TEMPERATURE=0.2

# Anthropic
ANTHROPIC_API_KEY=your-anthropic-api-key-here
ANTHROPIC_MODEL=claude-3-opus-20240229
ANTHROPIC_TEMPERATURE=0.2

# Local Models (Hugging Face)
HF_MODEL=codellama/CodeLlama-7b-Python-hf
HF_DEVICE=cuda  # or cpu

# Test Configuration
MAX_TOKENS=4000
TIMEOUT_SECONDS=60
PARALLEL_TESTS=False

# Output Configuration
SAVE_GENERATED_CODE=True
GENERATE_VISUALIZATIONS=True
REPORT_FORMAT=json  # json, html, markdown
EOF
    echo "✓ Created .env template file"
    echo ""
    echo "⚠️  Please update benchmarks/.env with your API keys"
else
    echo "✓ .env file already exists"
fi

# Create a simple test to verify installation
echo "Creating verification script..."
cat > benchmarks/verify_setup.py << 'EOF'
#!/usr/bin/env python3
"""Verify that the test suite is properly set up"""

import sys

def check_import(module_name):
    try:
        __import__(module_name)
        print(f"✓ {module_name}")
        return True
    except ImportError:
        print(f"✗ {module_name} - Not installed")
        return False

print("Checking required modules...")
print("-" * 30)

required = ["tiktoken", "httpx", "pytest", "pydantic", "dotenv"]
optional = ["openai", "anthropic", "transformers", "pandas", "matplotlib"]

all_good = True
for module in required:
    if not check_import(module):
        all_good = False

print("\nChecking optional modules...")
print("-" * 30)

for module in optional:
    check_import(module)

if all_good:
    print("\n✅ All required modules are installed!")
    print("You can now run the test suite with:")
    print("  python benchmarks/llm_token_test_suite.py")
else:
    print("\n❌ Some required modules are missing!")
    print("Please run: pip install -r benchmarks/requirements.txt")
    sys.exit(1)

# Test token counting
print("\nTesting token counting...")
print("-" * 30)
try:
    import tiktoken
    enc = tiktoken.encoding_for_model("gpt-4")
    test_text = "Hello, world!"
    tokens = len(enc.encode(test_text))
    print(f"✓ Token counting works: '{test_text}' = {tokens} tokens")
except Exception as e:
    print(f"✗ Token counting failed: {e}")
EOF

# Make scripts executable
chmod +x benchmarks/verify_setup.py

echo ""
echo "Running verification..."
echo "======================="
python benchmarks/verify_setup.py

echo ""
echo "Setup complete! 🎉"
echo ""
echo "Next steps:"
echo "1. Update benchmarks/.env with your API keys"
echo "2. Run the test suite: python benchmarks/llm_token_test_suite.py"
echo "3. View results in benchmarks/results/"
