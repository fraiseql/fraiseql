#!/bin/bash

# FraiseQL Documentation Link Validator
# Validates internal and external markdown links in documentation
# Exit codes:
#   0 = All links valid
#   1 = Broken internal links found
#   2 = Invalid markdown files
#   3 = Script error

set -o pipefail

DOCS_DIR="${1:-.}"
VERBOSE="${VERBOSE:-false}"
CHECK_EXTERNAL="${CHECK_EXTERNAL:-false}"
FAILED=0
WARNINGS=0

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Helper functions
log_error() {
    echo -e "${RED}✗${NC} $1" >&2
    ((FAILED++))
}

log_warning() {
    echo -e "${YELLOW}⚠${NC} $1" >&2
    ((WARNINGS++))
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

# Check if file exists (for relative and absolute paths)
check_file_exists() {
    local file="$1"
    local base_dir="$2"

    # Handle different path formats
    if [[ "$file" == /* ]]; then
        # Absolute path
        [[ -f "$file" ]]
    elif [[ "$file" == ./* ]] || [[ "$file" == ../* ]]; then
        # Relative path with ./ or ../
        [[ -f "${base_dir}/${file}" ]]
    else
        # Relative path
        [[ -f "${base_dir}/${file}" ]]
    fi
}

# Resolve path relative to current file
resolve_path() {
    local link="$1"
    local file_dir="$2"

    # Remove anchors and query strings
    link="${link%%#*}"
    link="${link%%\?*}"

    # Handle empty result
    [[ -z "$link" ]] && return 1

    if [[ "$link" == /* ]]; then
        # Absolute path from docs root
        echo "${DOCS_DIR}${link}"
    elif [[ "$link" == ./* ]]; then
        # Current directory
        echo "${file_dir}/${link:2}"
    elif [[ "$link" == ../* ]]; then
        # Parent directory
        echo "${file_dir}/${link}"
    else
        # Relative path
        echo "${file_dir}/${link}"
    fi
}

# Validate internal links
validate_internal_links() {
    local markdown_file="$1"
    local file_dir=$(dirname "$markdown_file")
    local relative_path="${markdown_file#${DOCS_DIR}/}"

    # Extract links from markdown
    # Matches [text](link) and [text]: link patterns
    local links=$(grep -oE '\[([^\]]+)\]\(([^)]+)\)|^\s*\[([^\]]+\]\s*:\s*([^\s]+)' "$markdown_file" | \
        sed -E 's/\[([^\]]+)\]\(([^)]+)\)/\2/g; s/^\s*\[([^\]]+\]\s*:\s*//g; s/\)$//g' | \
        grep -v '^http')

    while IFS= read -r link; do
        [[ -z "$link" ]] && continue

        # Skip external URLs
        if [[ "$link" == http* ]]; then
            [[ "$CHECK_EXTERNAL" == "true" ]] && validate_external_link "$link" "$relative_path"
            continue
        fi

        # Resolve the path
        local resolved=$(resolve_path "$link" "$file_dir")

        # Remove fragments from resolved path
        resolved="${resolved%%#*}"

        # Normalize path
        resolved=$(cd "$(dirname "$resolved")" && pwd)/$(basename "$resolved") 2>/dev/null || true

        # Check if file exists
        if ! check_file_exists "$resolved" "$DOCS_DIR"; then
            # Try without .md extension
            if ! check_file_exists "${resolved}.md" "$DOCS_DIR" && \
               ! check_file_exists "${resolved%.*}.md" "$DOCS_DIR"; then
                # Try as directory (README.md)
                if ! check_file_exists "${resolved}/README.md" "$DOCS_DIR"; then
                    log_error "Broken link in $relative_path: $link (resolved to: $resolved)"
                fi
            fi
        fi
    done <<< "$links"
}

# Validate external URLs (simple check)
validate_external_link() {
    local url="$1"
    local file="$2"

    if [[ "$VERBOSE" == "true" ]]; then
        log_info "Checking external link: $url"
    fi

    # Check if URL is reachable (requires curl)
    if command -v curl &> /dev/null; then
        if ! curl -s -I --connect-timeout 3 "$url" > /dev/null 2>&1; then
            log_warning "External link unreachable in $file: $url"
        fi
    fi
}

# Main validation
main() {
    log_info "FraiseQL Documentation Link Validator"
    log_info "Checking documentation in: $DOCS_DIR"
    echo ""

    # Verify docs directory exists
    if [[ ! -d "$DOCS_DIR" ]]; then
        log_error "Documentation directory not found: $DOCS_DIR"
        return 3
    fi

    # Count files
    local md_count=$(find "$DOCS_DIR" -name "*.md" -type f | wc -l)
    if [[ $md_count -eq 0 ]]; then
        log_error "No markdown files found in $DOCS_DIR"
        return 2
    fi

    log_info "Found $md_count markdown files"
    echo ""

    # Validate each markdown file
    local file_count=0
    while IFS= read -r markdown_file; do
        ((file_count++))

        [[ "$VERBOSE" == "true" ]] && log_info "Checking $file_count/$md_count: ${markdown_file#${DOCS_DIR}/}"

        validate_internal_links "$markdown_file"

    done < <(find "$DOCS_DIR" -name "*.md" -type f | sort)

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if [[ $FAILED -eq 0 ]]; then
        log_success "All links validated successfully!"
        [[ $WARNINGS -gt 0 ]] && echo "Warnings: $WARNINGS"
        return 0
    else
        log_error "Found $FAILED broken links"
        [[ $WARNINGS -gt 0 ]] && echo "Warnings: $WARNINGS"
        return 1
    fi
}

# Print usage
usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS] [DOCS_DIR]

Validate internal and external markdown links in FraiseQL documentation.

Arguments:
  DOCS_DIR              Documentation directory (default: current directory)

Options:
  -v, --verbose         Print verbose output
  -e, --check-external  Also check external URLs (slower)
  -h, --help           Show this help message

Examples:
  # Check current directory
  $(basename "$0")

  # Check specific directory
  $(basename "$0") /path/to/docs

  # Verbose output
  $(basename "$0") -v ./docs

  # Check external links too
  $(basename "$0") -e ./docs

Environment Variables:
  VERBOSE               Set to 'true' for verbose output
  CHECK_EXTERNAL        Set to 'true' to validate external links

Exit Codes:
  0 = All links valid
  1 = Broken internal links found
  2 = Invalid markdown files
  3 = Script error
EOF
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        -e|--check-external)
            CHECK_EXTERNAL=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            echo "Unknown option: $1"
            usage
            exit 3
            ;;
        *)
            DOCS_DIR="$1"
            shift
            ;;
    esac
done

# Run validation
main
exit $?
