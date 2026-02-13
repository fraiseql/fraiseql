#!/bin/bash
# Generate Software Bill of Materials (SBOM) for FraiseQL Docker image

set -e

IMAGE="${1:-fraiseql:latest}"
OUTPUT_DIR="${2:-.}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

echo "Generating SBOM for image: $IMAGE"
echo "Output directory: $OUTPUT_DIR"

# Create output directory if it doesn't exist
mkdir -p "$OUTPUT_DIR"

# Generate SBOM using Syft (if available)
if command -v syft &> /dev/null; then
    echo "Generating SBOM with Syft..."
    syft "$IMAGE" -o spdx-json > "$OUTPUT_DIR/sbom-spdx-$TIMESTAMP.json"
    syft "$IMAGE" -o cyclonedx-json > "$OUTPUT_DIR/sbom-cyclonedx-$TIMESTAMP.json"
    echo "SBOM generated successfully"
    echo "  - SPDX: $OUTPUT_DIR/sbom-spdx-$TIMESTAMP.json"
    echo "  - CycloneDX: $OUTPUT_DIR/sbom-cyclonedx-$TIMESTAMP.json"
else
    echo "Warning: syft not installed. Install it to generate SBOMs:"
    echo "  curl -sSfL https://raw.githubusercontent.com/anchore/syft/main/install.sh | sh -s -- -b /usr/local/bin"
    exit 1
fi
