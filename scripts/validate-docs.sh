#!/bin/bash

# FraiseQL Documentation Validation Script
# Comprehensive testing of documentation quality and accuracy

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Validate internal links in markdown files
validate_links() {
    log_info "Validating internal links..."

    local errors=0
    local total_files=0

    # Find all markdown files (excluding archive directory)
    while IFS= read -r -d '' file; do
        ((total_files++))
        local file_errors=0

        # Extract relative links (./ and ../)
        while IFS= read -r line; do
            # Extract markdown links
            links=$(echo "$line" | grep -o '\[.*\](\([^)]*\))' | sed 's/.*(\([^)]*\))/\1/' || true)

            for link in $links; do
                # Skip external links (http/https)
                if [[ $link =~ ^https?:// ]]; then
                    continue
                fi

                # Skip anchor links (#section)
                if [[ $link =~ ^# ]]; then
                    continue
                fi

                # Skip GitHub-relative links (issues, discussions)
                if [[ $link =~ (issues|discussions)$ ]]; then
                    continue
                fi

                # Skip regex patterns or invalid paths
                if [[ $link =~ \\d\{[0-9,]+\} ]]; then
                    continue
                fi

                # Resolve relative path
                local target_path="$file"
                local link_path="$link"

                # Get the directory of the file
                local file_dir="$(dirname "$file")"

                # Handle relative links
                if [[ $link_path =~ ^\.\./ ]]; then
                    # Go up one directory for each ../
                    local up_count=$(echo "$link_path" | grep -o '\.\./' | wc -l)
                    for ((i=0; i<up_count; i++)); do
                        file_dir="$(dirname "$file_dir")"
                    done
                    link_path="${link_path#$(printf '%.0s../' $(seq 1 $up_count))}"
                elif [[ $link_path =~ ^\./ ]]; then
                    link_path="${link_path#./}"
                fi

                target_path="$file_dir/$link_path"

                # Check if target exists
                if [[ ! -f $target_path ]] && [[ ! -d $target_path ]]; then
                    log_error "Broken link in $file: $link (resolved to: $target_path)"
                    ((file_errors++))
                    ((errors++))
                fi
            done
        done < "$file"

        if [[ $file_errors -gt 0 ]]; then
            log_warning "$file: $file_errors broken links"
        fi

    done < <(find "$PROJECT_ROOT" -name "*.md" -type f -not -path "*/archive/*" -print0)

    if [[ $errors -eq 0 ]]; then
        log_success "All $total_files markdown files have valid internal links"
    else
        log_error "Found $errors broken internal links across $total_files files"
        return 1
    fi
}

# Validate file references in documentation
validate_file_references() {
    log_info "Validating file references..."

    local errors=0

    # Check common file references
    local files_to_check=(
        "README.md"
        "pyproject.toml"
        "CONTRIBUTING.md"
        "INSTALLATION.md"
        "AUDIENCES.md"
        "VERSION_STATUS.md"
        "PERFORMANCE_GUIDE.md"
    )

    for file in "${files_to_check[@]}"; do
        if [[ ! -f "$PROJECT_ROOT/$file" ]]; then
            log_error "Referenced file missing: $file"
            ((errors++))
        fi
    done

    # Check directory references
    local dirs_to_check=(
        "docs"
        "examples"
        "scripts"
        "src"
        "tests"
    )

    for dir in "${dirs_to_check[@]}"; do
        if [[ ! -d "$PROJECT_ROOT/$dir" ]]; then
            log_error "Referenced directory missing: $dir"
            ((errors++))
        fi
    done

    if [[ $errors -eq 0 ]]; then
        log_success "All file and directory references are valid"
    else
        log_error "Found $errors missing file/directory references"
        return 1
    fi
}

# Validate code syntax in examples
validate_code_syntax() {
    log_info "Validating code syntax in examples..."

    local errors=0

    # Check if python is available for syntax validation
    if ! command_exists python3; then
        log_warning "Python3 not found, skipping Python syntax validation"
        return 0
    fi

    # Find Python code blocks in markdown
    while IFS= read -r -d '' file; do
        local in_python_block=false
        local line_num=0
        local temp_file="/tmp/fraiseql_syntax_check.py"

        while IFS= read -r line; do
            ((line_num++))

            if [[ $line =~ ^\`\`\`python ]]; then
                in_python_block=true
                # Start collecting Python code
                > "$temp_file"
                continue
            fi

            if [[ $line =~ ^\`\`\` ]] && [[ $in_python_block == true ]]; then
                # End of Python block, validate syntax
                if [[ -s $temp_file ]]; then
                    if ! python3 -m py_compile "$temp_file" 2>/dev/null; then
                        log_error "Invalid Python syntax in $file at line $line_num"
                        ((errors++))
                    fi
                fi
                in_python_block=false
                continue
            fi

            if [[ $in_python_block == true ]]; then
                # Remove markdown formatting from code lines
                clean_line=$(echo "$line" | sed 's/^    //')
                echo "$clean_line" >> "$temp_file"
            fi
        done < "$file"

        # Clean up
        rm -f "$temp_file"

    done < <(find "$PROJECT_ROOT" -name "*.md" -type f -not -path "*/archive/*" -print0)

    if [[ $errors -eq 0 ]]; then
        log_success "All Python code blocks have valid syntax"
    else
        log_error "Found $errors Python syntax errors in documentation"
        return 1
    fi
}

# Test basic installation
test_basic_installation() {
    log_info "Testing basic installation..."

    # This is a basic check - full installation testing would require a clean environment
    if command_exists python3 && command_exists pip; then
        log_success "Python and pip are available for installation testing"
        # Note: Full installation testing should be done in CI with clean environments
        log_info "Note: Full installation testing requires clean environment (use CI)"
    else
        log_warning "Python/pip not available, cannot test installation"
    fi
}

# Check version consistency
check_version_consistency() {
    log_info "Checking version consistency..."

    local errors=0

    # Get version from pyproject.toml
    local pyproject_version
    pyproject_version=$(grep '^version = ' "$PROJECT_ROOT/pyproject.toml" | sed 's/version = "\(.*\)"/\1/')

    if [[ -z $pyproject_version ]]; then
        log_error "Could not find version in pyproject.toml"
        ((errors++))
    else
        log_info "Found version $pyproject_version in pyproject.toml"

        # Check if version appears in README
        if ! grep -q "$pyproject_version" "$PROJECT_ROOT/README.md"; then
            log_error "Version $pyproject_version not found in README.md"
            ((errors++))
        fi

        # Check if version appears in VERSION_STATUS.md
        if ! grep -q "$pyproject_version" "$PROJECT_ROOT/VERSION_STATUS.md"; then
            log_error "Version $pyproject_version not found in VERSION_STATUS.md"
            ((errors++))
        fi
    fi

    if [[ $errors -eq 0 ]]; then
        log_success "Version information is consistent across files"
    else
        log_error "Found $errors version consistency issues"
        return 1
    fi
}

# Main validation function
run_validation() {
    local mode="${1:-all}"
    local exit_code=0

    log_info "Starting FraiseQL documentation validation (mode: $mode)"

    case $mode in
        "links")
            validate_links || exit_code=1
            ;;
        "files")
            validate_file_references || exit_code=1
            ;;
        "syntax")
            validate_code_syntax || exit_code=1
            ;;
        "versions")
            check_version_consistency || exit_code=1
            ;;
        "install")
            test_basic_installation || exit_code=1
            ;;
        "all")
            validate_links || exit_code=1
            validate_file_references || exit_code=1
            validate_code_syntax || exit_code=1
            check_version_consistency || exit_code=1
            test_basic_installation || exit_code=1
            ;;
        *)
            log_error "Unknown validation mode: $mode"
            log_info "Available modes: links, files, syntax, versions, install, all"
            exit 1
            ;;
    esac

    if [[ $exit_code -eq 0 ]]; then
        log_success "Documentation validation completed successfully"
    else
        log_error "Documentation validation found issues"
    fi

    return $exit_code
}

# Show usage
show_usage() {
    cat << EOF
FraiseQL Documentation Validation Script

Usage: $0 [MODE] [OPTIONS]

Modes:
    all         Run all validation checks (default)
    links       Validate internal links only
    files       Validate file references only
    syntax      Validate code syntax only
    versions    Check version consistency only
    install     Test basic installation only

Options:
    -h, --help  Show this help message

Examples:
    $0                          # Run all checks
    $0 links                    # Check links only
    $0 files syntax             # Check files and syntax

EOF
}

# Parse arguments
if [[ $# -gt 0 ]]; then
    case $1 in
        -h|--help)
            show_usage
            exit 0
            ;;
        links|files|syntax|versions|install|all)
            run_validation "$1"
            ;;
        *)
            log_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
else
    run_validation "all"
fi
