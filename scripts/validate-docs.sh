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

    # Find all markdown files (excluding archive and dev/audits directories)
    while IFS= read -r -d '' file; do
        ((total_files++))
        local file_errors=0

        # Extract relative links (./ and ../)
        # Use grep to find all links in the file at once for better performance
        links=$(grep -o '\[.*\](\([^)]*\))' "$file" | sed 's/.*(\([^)]*\))/\1/' || true)

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

            # Strip anchor from link (e.g., file.md#section -> file.md)
            local link_path="$link"
            if [[ $link_path =~ ^([^#]+)# ]]; then
                link_path="${BASH_REMATCH[1]}"
            fi

            # Resolve relative path
            local target_path="$file"

            # Get the directory of the file
            local file_dir="$(dirname "$file")"

            # Handle absolute paths (starting with /)
            if [[ $link_path =~ ^/ ]]; then
                target_path="$PROJECT_ROOT$link_path"

                # Remove trailing slash for directory checks
                local check_path="$target_path"
                if [[ $check_path =~ /$ ]]; then
                    check_path="${check_path%/}"
                fi

                # Check if target exists
                if [[ ! -f $check_path ]] && [[ ! -d $check_path ]]; then
                    log_error "Broken link in $file: $link (resolved to: $target_path)"
                    ((file_errors++))
                    ((errors++))
                fi
                continue
            fi

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

            # Remove trailing slash for directory checks
            local check_path="$target_path"
            if [[ $check_path =~ /$ ]]; then
                check_path="${check_path%/}"
            fi

            # Check if target exists (file or directory)
            if [[ ! -f $check_path ]] && [[ ! -d $check_path ]]; then
                log_error "Broken link in $file: $link (resolved to: $target_path)"
                ((file_errors++))
                ((errors++))
            fi
        done

        if [[ $file_errors -gt 0 ]]; then
            log_warning "$file: $file_errors broken links"
        fi

    done < <(find "$PROJECT_ROOT" -name "*.md" -type f \
        -not -path "*/archive/*" \
        -not -path "*/dev/audits/*" \
        -not -path "*/.phases/*" \
        -not -path "*/.venv/*" \
        -not -path "*/venv/*" \
        -not -path "*/node_modules/*" \
        -print0)

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
        "dev/architecture/audiences.md"
        "docs/guides/performance-guide.md"
        "docs/getting-started/installation.md"
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
# Validate Python syntax using AST parsing (documentation-friendly)
validate_python_ast() {
    local code="$1"
    local file="$2"
    local line_num="$3"

    # Write code to temp file and validate
    local temp_code_file="/tmp/fraiseql_ast_check.py"
    echo "$code" > "$temp_code_file"

    local result
    result=$(python3 -c "
import ast
import sys
try:
    with open('$temp_code_file', 'r') as f:
        code = f.read()
    ast.parse(code)
    print('VALID')
except SyntaxError as e:
    print('SYNTAX_ERROR: ' + str(e))
except Exception as e:
    print('OTHER_ERROR: ' + str(e))
" 2>/dev/null)

    rm -f "$temp_code_file"
    echo "$result"
}

# Detect documentation patterns in code
detect_documentation_pattern() {
    local code="$1"

    # Check for documentation indicators
    if echo "$code" | grep -q "^\s*# Example\|^# Note\|^# \.\.\.\|^\s*\.\.\.\|^\s*# TODO\|^\s*# FIXME"; then
        echo "documentation_example"
        return 0
    fi

    # Check for incomplete function/class patterns (functions/classes without bodies)
    if echo "$code" | grep -q "^\s*def.*:\s*$" && ! echo "$code" | grep -q "^\s*pass\|^return\|^raise\|^yield\|^\"\"\""; then
        echo "incomplete_function"
        return 0
    fi

    # Check for class definitions without complete bodies
    if echo "$code" | grep -q "^\s*class.*:\s*$" && ! echo "$code" | grep -q "^\s*pass\|^def\|^class\|^\"\"\""; then
        echo "incomplete_class"
        return 0
    fi

    # Check for FraiseQL framework usage
    if echo "$code" | grep -q "@type\|from fraiseql\|fraiseql\."; then
        echo "fraiseql_example"
        return 0
    fi

    # Check for code that looks like examples (short snippets, contains ...)
    if echo "$code" | grep -q "\.\.\." && [[ $(echo "$code" | wc -l) -lt 10 ]]; then
        echo "code_snippet"
        return 0
    fi

    echo "complete_code"
}

# Add context for common documentation patterns
add_documentation_context() {
    local code="$1"
    local pattern="$2"

    case "$pattern" in
        "fraiseql_example")
            # Add common imports for FraiseQL examples
            echo "# Auto-added imports for FraiseQL examples"
            echo "from typing import Optional, List, Dict, Any"
            echo "from datetime import datetime"
            echo "from uuid import UUID"
            echo ""
            echo "# Mock FraiseQL imports (for documentation validation)"
            echo "class type:"
            echo "    def __init__(self, **kwargs): pass"
            echo "    def __call__(self, cls): return cls"
            echo ""
            echo "$code"
            ;;
        "code_snippet"|"documentation_example")
            # Add basic typing imports for code snippets
            echo "# Auto-added imports for code validation"
            echo "from typing import Optional, List, Dict, Any"
            echo "from datetime import datetime"
            echo "from uuid import UUID"
            echo ""
            echo "$code"
            ;;
        "incomplete_function"|"incomplete_class")
            # For incomplete code, add pass statements to make it valid
            if echo "$code" | grep -q "^\s*def.*:\s*$" && ! echo "$code" | grep -q "^\s*pass\|^return\|^raise\|^yield"; then
                echo "$code"
                echo "    pass"
            elif echo "$code" | grep -q "^\s*class.*:\s*$" && ! echo "$code" | grep -q "^\s*pass\|^def\|^class"; then
                echo "$code"
                echo "    pass"
            else
                echo "$code"
            fi
            ;;
        *)
            echo "$code"
            ;;
    esac
}

# Classify and report syntax errors appropriately
classify_and_report_error() {
    local error_msg="$1"
    local file="$2"
    local line_num="$3"
    local pattern="$4"

    # Extract line number from error message if available
    local error_line=""
    if echo "$error_msg" | grep -q "line [0-9]"; then
        error_line=$(echo "$error_msg" | sed 's/.*line \([0-9]*\).*/\1/')
        error_line=$((line_num + error_line - 1))
    else
        error_line=$line_num
    fi

    if echo "$error_msg" | grep -q "No module named\|ImportError"; then
        case "$pattern" in
            "documentation_example"|"incomplete_function"|"incomplete_class"|"fraiseql_example")
                log_warning "Missing import in $file:$error_line (documentation example - treating as warning)"
                return 0  # Warning, not error
                ;;
            *)
                log_error "Missing import in $file:$error_line"
                return 1
                ;;
        esac
    elif echo "$error_msg" | grep -q "unexpected indent\|expected an indented"; then
        log_error "Indentation error in $file:$error_line - check markdown formatting (Python code needs proper indentation)"
        return 1
    elif echo "$error_msg" | grep -q "invalid syntax"; then
        log_error "Syntax error in $file:$error_line - $error_msg"
        return 1
    else
        log_error "Code validation error in $file:$error_line - $error_msg"
        return 1
    fi

    return 0
}

validate_code_syntax() {
    log_info "Validating code syntax in examples (enhanced mode)..."

    local errors=0
    local warnings=0

    # Check if python is available for syntax validation
    if ! command_exists python3; then
        log_warning "Python3 not found, skipping Python syntax validation"
        return 0
    fi

    # Find Python code blocks in markdown (excluding archive and dev/audits directories)
    while IFS= read -r -d '' file; do
        local in_python_block=false
        local line_num=0
        local block_start_line=0
        local temp_file="/tmp/fraiseql_syntax_check.py"

        while IFS= read -r line; do
            ((line_num++))

            if [[ $line =~ ^\`\`\`python ]]; then
                in_python_block=true
                block_start_line=$line_num
                # Start collecting Python code
                > "$temp_file"
                continue
            fi

            if [[ $line =~ ^\`\`\` ]] && [[ $in_python_block == true ]]; then
                # End of Python block, validate syntax
                if [[ -s $temp_file ]]; then
                    local code
                    code=$(cat "$temp_file")

                    # Skip empty or trivial blocks
                    if [[ -z "$(echo "$code" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | grep -v '^$')" ]]; then
                        continue
                    fi

                    # Detect pattern
                    local pattern
                    pattern=$(detect_documentation_pattern "$code")

                    # For documentation examples and code snippets, be more lenient
                    case "$pattern" in
                        "documentation_example"|"code_snippet")
                            # Skip AST validation for documentation snippets - they're not meant to be complete programs
                            log_info "Skipping detailed validation for $file:$block_start_line ($pattern)"
                            continue
                            ;;
                        "incomplete_function"|"incomplete_class")
                            # Try to make incomplete code valid by adding context
                            local enhanced_code
                            enhanced_code=$(add_documentation_context "$code" "$pattern")
                            local validation_result
                            validation_result=$(validate_python_ast "$enhanced_code" "$file" "$block_start_line")

                            if [[ $validation_result == "VALID" ]]; then
                                # Success - continue
                                continue
                            else
                                # For incomplete code that we tried to fix, treat as warning
                                log_warning "Could not validate incomplete code in $file:$block_start_line"
                                ((warnings++))
                                continue
                            fi
                            ;;
                        *)
                            # For complete code, do full validation
                            local enhanced_code
                            enhanced_code=$(add_documentation_context "$code" "$pattern")
                            local validation_result
                            validation_result=$(validate_python_ast "$enhanced_code" "$file" "$block_start_line")

                            if [[ $validation_result == "VALID" ]]; then
                                # Success - continue
                                continue
                            elif [[ $validation_result == *"SYNTAX_ERROR:"* ]]; then
                                local error_msg
                                error_msg=$(echo "$validation_result" | sed 's/^SYNTAX_ERROR: //')
                                if classify_and_report_error "$error_msg" "$file" "$block_start_line" "$pattern"; then
                                    ((warnings++))
                                else
                                    ((errors++))
                                fi
                            elif [[ $validation_result == *"OTHER_ERROR:"* ]]; then
                                local error_msg
                                error_msg=$(echo "$validation_result" | sed 's/^OTHER_ERROR: //')
                                if classify_and_report_error "$error_msg" "$file" "$block_start_line" "$pattern"; then
                                    ((warnings++))
                                else
                                    ((errors++))
                                fi
                            else
                                log_error "Unknown validation result for $file:$block_start_line: '$validation_result'"
                                ((errors++))
                            fi
                            ;;
                    esac
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

    done < <(find "$PROJECT_ROOT" -name "*.md" -type f \
        -not -path "*/archive/*" \
        -not -path "*/dev/audits/*" \
        -not -path "*/.phases/*" \
        -not -path "*/.venv/*" \
        -not -path "*/venv/*" \
        -not -path "*/node_modules/*" \
        -print0)

    if [[ $errors -eq 0 ]]; then
        if [[ $warnings -gt 0 ]]; then
            log_success "All Python code blocks validated ($warnings warnings for documentation examples)"
        else
            log_success "All Python code blocks have valid syntax"
        fi
    else
        log_error "Found $errors syntax errors and $warnings warnings in documentation"
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
