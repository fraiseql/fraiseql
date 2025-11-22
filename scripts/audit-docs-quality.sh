#!/bin/bash

# Documentation Quality Audit - Phase 1: Automated Pattern Detection
# Scans for common documentation quality issues using grep patterns

set -e

echo "🔍 Starting Documentation Quality Audit - Automated Scan"
echo "======================================================"

# Output file
OUTPUT_FILE="dev/audits/docs-quality-issues-automated.md"

# Initialize counters
total_issues=0
critical_count=0
high_count=0
medium_count=0
low_count=0

# Function to count issues and format output
count_and_format() {
    local category="$1"
    local priority="$2"
    local results="$3"
    local max_examples="${4:-10}"

    if [ -n "$results" ]; then
        local count=$(echo "$results" | wc -l)
        total_issues=$((total_issues + count))

        case $priority in
            "CRITICAL") critical_count=$((critical_count + count)) ;;
            "HIGH") high_count=$((high_count + count)) ;;
            "MEDIUM") medium_count=$((medium_count + count)) ;;
            "LOW") low_count=$((low_count + count)) ;;
        esac

        echo "## $category ($count found)"
        if [ "$count" -gt "$max_examples" ]; then
            echo "$results" | head -n "$max_examples" | sed 's/^/- /'
            echo "- ... and $((count - max_examples)) more instances"
        else
            echo "$results" | sed 's/^/- /'
        fi
        echo ""
    fi
}

# Create output directory if it doesn't exist
mkdir -p dev/audits

# Start writing the report
cat > "$OUTPUT_FILE" << 'EOF'
# Documentation Quality Issues - Automated Scan

EOF

echo "Scanning for tone issues..."
TONE_RESULTS=$(grep -rn "interview\|TODO\|FIXME\|WIP\|XXX" docs/ --include="*.md" 2>/dev/null || true)
count_and_format "Tone Issues" "HIGH" "$TONE_RESULTS" 20 >> "$OUTPUT_FILE"

echo "Scanning for placeholder text..."
PLACEHOLDER_RESULTS=$(grep -rn "\[TODO\]\|\[DESCRIPTION\]\|Lorem ipsum" docs/ --include="*.md" 2>/dev/null || true)
count_and_format "Placeholder Text" "HIGH" "$PLACEHOLDER_RESULTS" 15 >> "$OUTPUT_FILE"

echo "Scanning for code blocks without language tags..."
CODEBLOCK_RESULTS=$(grep -rn "^\`\`\`$" docs/ --include="*.md" 2>/dev/null || true)
count_and_format "Code Blocks Without Language Tags" "HIGH" "$CODEBLOCK_RESULTS" 5 >> "$OUTPUT_FILE"

echo "Scanning for excessive exclamation marks..."
EXCLAMATION_RESULTS=$(grep -rn "!!!\|!!!!!" docs/ --include="*.md" 2>/dev/null || true)
count_and_format "Excessive Exclamation Marks" "MEDIUM" "$EXCLAMATION_RESULTS" 10 >> "$OUTPUT_FILE"

echo "Scanning for absolute GitHub URLs..."
GITHUB_URLS=$(grep -rn "https://github.com/fraiseql/fraiseql" docs/ --include="*.md" 2>/dev/null || true)
count_and_format "Absolute GitHub URLs" "MEDIUM" "$GITHUB_URLS" 10 >> "$OUTPUT_FILE"

echo "Scanning for trailing whitespace..."
TRAILING_WS=$(grep -rn "[[:space:]]$" docs/ --include="*.md" 2>/dev/null || true)
count_and_format "Trailing Whitespace" "LOW" "$TRAILING_WS" 5 >> "$OUTPUT_FILE"

echo "Scanning for inconsistent list formatting..."
LIST_MIX=$(grep -rn "^[[:space:]]*-[[:space:]]" docs/ --include="*.md" | head -20 2>/dev/null || true)
LIST_MIX_COUNT=$(echo "$LIST_MIX" | wc -l)
if [ "$LIST_MIX_COUNT" -gt 0 ]; then
    ASTERISK_LISTS=$(grep -rn "^[[:space:]]*\*[[:space:]]" docs/ --include="*.md" | head -20 2>/dev/null || true)
    if [ -n "$ASTERISK_LISTS" ]; then
        MIXED_LISTS="Mixed bullet styles found (both - and * used)\n$LIST_MIX\n$ASTERISK_LISTS"
        count_and_format "Inconsistent List Formatting" "LOW" "$MIXED_LISTS" >> "$OUTPUT_FILE"
    fi
fi

echo "Scanning for wrong Python version references..."
PYTHON_VERSION=$(grep -rn "Python 3\.11\|python 3\.11\|Python 3\.12\|python 3\.12" docs/ --include="*.md" 2>/dev/null || true)
count_and_format "Wrong Python Version References" "CRITICAL" "$PYTHON_VERSION" 20 >> "$OUTPUT_FILE"

# Add summary
cat >> "$OUTPUT_FILE" << EOF

## Summary
- Total issues found: $total_issues
- CRITICAL: $critical_count
- HIGH: $high_count
- MEDIUM: $medium_count
- LOW: $low_count

EOF

echo "✅ Automated scan complete!"
echo "📊 Results written to $OUTPUT_FILE"
echo "   - Total issues found: $total_issues"
echo "   - CRITICAL: $critical_count"
echo "   - HIGH: $high_count"
echo "   - MEDIUM: $medium_count"
echo "   - LOW: $low_count"
