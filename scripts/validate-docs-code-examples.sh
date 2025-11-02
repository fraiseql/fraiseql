#!/bin/bash

# Phase 3: Code Example Validation
# Extract and validate Python code blocks from documentation

set -e

echo "🔍 Phase 3: Code Example Validation"
echo "==================================="

# Create output directory
mkdir -p dev/audits/phase3

# Output files
EXTRACTED_CODE_DIR="dev/audits/phase3/extracted_code"
VALIDATION_REPORT="dev/audits/phase3/code_validation_report.md"
SYNTAX_ERRORS="dev/audits/phase3/syntax_errors.txt"

mkdir -p "$EXTRACTED_CODE_DIR"

echo "📝 Extracting Python code blocks from documentation..."

# Find all markdown files in docs/
find docs/ -name "*.md" -type f | while read -r file; do
    echo "Processing: $file"

    # Extract Python code blocks and save them
    # Create a safe filename from the path
    safe_name=$(echo "$file" | sed 's|^docs/||; s|/|_|g; s|\.md$||')

    awk -v safe_name="$safe_name" -v outdir="$EXTRACTED_CODE_DIR" '
    BEGIN { in_python_block = 0; block_count = 0 }
    /^```python/ {
        in_python_block = 1
        block_count++
        output_file = outdir "/" safe_name "_" block_count ".py"
        print "# Extracted from: " FILENAME > output_file
        print "# Block number: " block_count >> output_file
        next
    }
    /^```$/ && in_python_block {
        in_python_block = 0
        close(output_file)
        next
    }
    in_python_block {
        print >> output_file
    }
    ' "$file"
done

echo "✅ Code extraction complete"
echo "📊 Running syntax validation..."

# Initialize counters
total_blocks=0
syntax_errors=0
valid_blocks=0

# Create validation report
cat > "$VALIDATION_REPORT" << 'EOF'
# Phase 3: Code Example Validation Report

## Summary

EOF

# Validate each extracted Python file
echo "" > "$SYNTAX_ERRORS"

for py_file in "$EXTRACTED_CODE_DIR"/*.py; do
    if [ -f "$py_file" ]; then
        total_blocks=$((total_blocks + 1))

        # Check syntax
        if python -m py_compile "$py_file" 2>/dev/null; then
            valid_blocks=$((valid_blocks + 1))
            echo "✅ $(basename "$py_file")"
        else
            syntax_errors=$((syntax_errors + 1))
            echo "❌ $(basename "$py_file")"
            echo "=== $(basename "$py_file") ===" >> "$SYNTAX_ERRORS"
            python -m py_compile "$py_file" 2>> "$SYNTAX_ERRORS" || true
            echo "" >> "$SYNTAX_ERRORS"
        fi
    fi
done

# Update report with summary
cat >> "$VALIDATION_REPORT" << EOF
- **Total Python code blocks extracted**: $total_blocks
- **Valid syntax**: $valid_blocks
- **Syntax errors**: $syntax_errors

## Validation Results

EOF

if [ $syntax_errors -eq 0 ]; then
    echo "- ✅ **All code blocks have valid Python syntax**" >> "$VALIDATION_REPORT"
else
    echo "- ❌ **Found $syntax_errors code blocks with syntax errors**" >> "$VALIDATION_REPORT"
    echo "" >> "$VALIDATION_REPORT"
    echo "### Syntax Errors" >> "$VALIDATION_REPORT"
    echo "" >> "$VALIDATION_REPORT"
    echo "\`\`\`" >> "$VALIDATION_REPORT"
    cat "$SYNTAX_ERRORS" >> "$VALIDATION_REPORT"
    echo "\`\`\`" >> "$VALIDATION_REPORT"
fi

echo ""
echo "📋 Validation Summary:"
echo "   - Total blocks: $total_blocks"
echo "   - Valid: $valid_blocks"
echo "   - Errors: $syntax_errors"

if [ $syntax_errors -gt 0 ]; then
    echo ""
    echo "❌ See $SYNTAX_ERRORS for details"
fi

echo ""
echo "📄 Full report: $VALIDATION_REPORT"
