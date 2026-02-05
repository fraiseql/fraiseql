#!/bin/bash
# Scan Docker image for vulnerabilities using Trivy

set -e

IMAGE="${1:-fraiseql:latest}"
SEVERITY="${2:-HIGH,CRITICAL}"
OUTPUT_DIR="${3:-.}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

echo "Scanning image: $IMAGE"
echo "Severity levels: $SEVERITY"
echo "Output directory: $OUTPUT_DIR"

# Create output directory if it doesn't exist
mkdir -p "$OUTPUT_DIR"

# Scan using Trivy (if available)
if command -v trivy &> /dev/null; then
    echo "Scanning with Trivy..."
    
    # Generate JSON report
    trivy image "$IMAGE" \
        --severity "$SEVERITY" \
        --format json \
        --output "$OUTPUT_DIR/trivy-scan-$TIMESTAMP.json"
    
    echo "Generating summary..."
    trivy image "$IMAGE" \
        --severity "$SEVERITY" \
        --format table \
        --output "$OUTPUT_DIR/trivy-scan-$TIMESTAMP.txt"
    
    echo "Vulnerability scan completed"
    echo "  - JSON: $OUTPUT_DIR/trivy-scan-$TIMESTAMP.json"
    echo "  - Summary: $OUTPUT_DIR/trivy-scan-$TIMESTAMP.txt"
    
    # Exit with error if vulnerabilities found
    VULN_COUNT=$(grep -c '"Severity"' "$OUTPUT_DIR/trivy-scan-$TIMESTAMP.json" || true)
    if [ "$VULN_COUNT" -gt 0 ]; then
        echo "Found $VULN_COUNT vulnerabilities with severity $SEVERITY"
        exit 1
    fi
else
    echo "Warning: trivy not installed. Install it to scan for vulnerabilities:"
    echo "  https://github.com/aquasecurity/trivy#installation"
    exit 1
fi
