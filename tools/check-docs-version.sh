#!/usr/bin/env bash
# check-docs-version.sh — fail when a doc's "vX.Y.Z released" status line disagrees
# with the workspace version in Cargo.toml.
#
# Background (#735 / P24): docs/architecture/overview.md once claimed v2.10.0 in its
# header and v2.8.0 in its footer while the product shipped v2.14.1 — three different
# versions in one document. Historical references ("removed in v2.15.0") are fine;
# a *status* line asserting what is currently released must match Cargo.toml.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

version=$(grep -m1 '^version = ' Cargo.toml | sed -E 's/version = "([^"]+)"/\1/')
if [ -z "$version" ]; then
  echo "ERROR: could not read workspace version from Cargo.toml" >&2
  exit 1
fi

bad=$(grep -rnE "v[0-9]+\.[0-9]+\.[0-9]+ (released|\(released\))" docs/ --include='*.md' \
  | grep -v "v${version} " || true)

if [ -n "$bad" ]; then
  {
    echo "ERROR: docs claim a released version that is not the workspace version (v${version}):"
    echo "$bad"
    echo
    echo "Update the status line, or reword a historical reference so it does not read as"
    echo "a current-release claim (e.g. 'shipped in vX.Y.Z' instead of 'vX.Y.Z released')."
  } >&2
  exit 1
fi

echo "OK: every 'vX.Y.Z released' status line in docs/ matches Cargo.toml (v${version})."
