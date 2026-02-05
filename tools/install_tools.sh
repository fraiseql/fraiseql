#!/bin/bash
# Install essential Rust development tools for FraiseQL v2

set -e

echo "🦀 Installing Rust Development Tools for FraiseQL v2"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: cargo not found. Please install Rust first:${NC}"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

echo -e "${GREEN}✓${NC} Rust toolchain found: $(rustc --version)"
echo ""

# Function to install tool
install_tool() {
    local name=$1
    local package=$2
    local flags=$3

    if command -v $name &> /dev/null; then
        echo -e "${YELLOW}⊙${NC} $name already installed ($(command -v $name))"
    else
        echo -e "${GREEN}→${NC} Installing $name..."
        cargo install $package $flags
    fi
}

echo "📦 Installing Essential Tools..."
echo ""

# Core development tools
install_tool "cargo-watch" "cargo-watch"
install_tool "cargo-nextest" "cargo-nextest" "--locked"
install_tool "cargo-llvm-cov" "cargo-llvm-cov"
install_tool "cargo-audit" "cargo-audit"
install_tool "cargo-outdated" "cargo-outdated"
install_tool "cargo-edit" "cargo-edit"

echo ""
echo "🔍 Installing Code Quality Tools..."
echo ""

install_tool "cargo-deny" "cargo-deny"
install_tool "cargo-machete" "cargo-machete"
install_tool "taplo" "taplo-cli"

echo ""
echo "⚡ Installing Performance Tools..."
echo ""

install_tool "flamegraph" "flamegraph"
install_tool "cargo-bloat" "cargo-bloat"
install_tool "cargo-expand" "cargo-expand"

echo ""
echo "🗄️  Installing Database Tools..."
echo ""

install_tool "sqlx" "sqlx-cli" "--no-default-features --features postgres,mysql,sqlite"

echo ""
echo "🎉 Installation Complete!"
echo ""
echo -e "${GREEN}✓${NC} All essential tools installed"
echo ""

# Check for fast linker
echo "🔗 Checking for fast linker..."
if command -v mold &> /dev/null; then
    echo -e "${GREEN}✓${NC} mold linker found"
elif command -v zld &> /dev/null; then
    echo -e "${GREEN}✓${NC} zld linker found"
elif command -v lld &> /dev/null; then
    echo -e "${GREEN}✓${NC} lld linker found"
else
    echo -e "${YELLOW}⚠${NC}  No fast linker detected. Install one for faster builds:"
    echo ""
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        echo "  sudo apt install mold    # Ubuntu/Debian"
        echo "  sudo pacman -S mold       # Arch Linux"
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        echo "  brew install michaeleisel/zld/zld"
    fi
    echo ""
fi

# Check Rust components
echo "🔧 Checking Rust components..."
MISSING_COMPONENTS=()

if ! rustup component list | grep -q "rustfmt.*installed"; then
    MISSING_COMPONENTS+=("rustfmt")
fi

if ! rustup component list | grep -q "clippy.*installed"; then
    MISSING_COMPONENTS+=("clippy")
fi

if ! rustup component list | grep -q "rust-analyzer.*installed"; then
    MISSING_COMPONENTS+=("rust-analyzer")
fi

if ! rustup component list | grep -q "llvm-tools.*installed"; then
    MISSING_COMPONENTS+=("llvm-tools-preview")
fi

if [ ${#MISSING_COMPONENTS[@]} -gt 0 ]; then
    echo -e "${YELLOW}⚠${NC}  Missing components: ${MISSING_COMPONENTS[@]}"
    echo "Installing missing components..."
    rustup component add ${MISSING_COMPONENTS[@]}
    echo -e "${GREEN}✓${NC} Components installed"
else
    echo -e "${GREEN}✓${NC} All required components installed"
fi

echo ""
echo "📚 Next Steps:"
echo ""
echo "  1. Make sure you have PostgreSQL installed for tests:"
echo "     make db-setup"
echo ""
echo "  2. Run tests to verify setup:"
echo "     make test"
echo ""
echo "  3. Start development:"
echo "     make watch     # Auto-run tests on changes"
echo ""
echo "  4. See all available commands:"
echo "     make help"
echo ""
echo "For more tools, see: tools/RECOMMENDED_TOOLS.md"
echo ""
echo -e "${GREEN}Happy coding! 🚀${NC}"
