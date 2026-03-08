#!/usr/bin/env bash
# check-vendor-security.sh
#
# Alerts when the vendored graphql-parser diverges from the upstream crates.io version.
# Run as part of `make security` to catch upstream security fixes.
#
# See vendor/MAINTENANCE.md for the full maintenance protocol.
set -euo pipefail

VENDORED_TOML="${BASH_SOURCE[0]%/*}/../vendor/graphql-parser/Cargo.toml"
VENDORED_VERSION=$(grep '^version' "$VENDORED_TOML" | head -1 | awk '{print $3}' | tr -d '"')

echo "Checking vendor/graphql-parser ($VENDORED_VERSION) against crates.io..."

UPSTREAM_VERSION=$(cargo search graphql-parser --limit 1 2>/dev/null \
  | grep '^graphql-parser ' \
  | awk '{print $3}' \
  | tr -d '"' \
  || true)

if [ -z "$UPSTREAM_VERSION" ]; then
    echo "INFO: Could not reach crates.io (network or rate-limit). Skipping vendor version check."
    exit 0
fi

if [ "$VENDORED_VERSION" = "$UPSTREAM_VERSION" ]; then
    echo "OK: vendor/graphql-parser ($VENDORED_VERSION) matches upstream ($UPSTREAM_VERSION)."
else
    echo ""
    echo "WARNING: vendor/graphql-parser ($VENDORED_VERSION) differs from upstream ($UPSTREAM_VERSION)."
    echo "         Review the upstream changelog for security fixes before shipping."
    echo "         Upstream changelog: https://github.com/graphql-rust/graphql-parser/blob/master/CHANGELOG.md"
    echo "         Upstream releases:  https://github.com/graphql-rust/graphql-parser/releases"
    echo ""
    echo "         If this is only a feature release (not a security fix), update vendor/MAINTENANCE.md"
    echo "         to document the version gap. If it is a security fix, port it immediately."
    echo ""
    # Non-zero exit to surface the warning in CI; treat as advisory (not hard failure)
    # by overriding with CI_VENDOR_WARN_ONLY=1 in environments where the gap is known.
    if [ "${CI_VENDOR_WARN_ONLY:-0}" = "1" ]; then
        echo "CI_VENDOR_WARN_ONLY=1 — continuing despite version gap."
        exit 0
    fi
    exit 1
fi
