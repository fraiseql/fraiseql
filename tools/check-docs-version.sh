#!/usr/bin/env bash
# check-docs-version.sh — fail when a doc's "vX.Y.Z released" status line disagrees
# with the workspace version in Cargo.toml.
#
# Background (#735 / P24): docs/architecture/overview.md once claimed v2.10.0 in its
# header and v2.8.0 in its footer while the product shipped v2.14.1 — three different
# versions in one document. Historical references ("removed in v2.15.0") are fine;
# a *status* line asserting what is currently released must match Cargo.toml.
#
# Second check (#1146): README.md's Rust install snippet must pin the workspace
# version. It sat at "2.8" for six minors, and tools/release.sh's README step could
# not fix it — the step's guard was satisfied by a historical sentence elsewhere on
# the page, and its blanket sed could not match the snippet anyway. The step is now
# an anchored rewrite (bump_readme_install_snippet); this is the gate that keeps it
# honest between releases.
#
# Overrides, for testing:  DOCS_VERSION_ROOT=<dir>
set -euo pipefail
if [ -n "${DOCS_VERSION_ROOT:-}" ]; then
  cd "$DOCS_VERSION_ROOT"
else
  cd "$(git rev-parse --show-toplevel)"
fi

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

# ── README.md install snippet ────────────────────────────────────────────────────
# Anchored on the dependency line, never on a bare version shape: README.md also carries
# historical references ("removed in v2.15.0") that must not be read as a pin (#1146).
readme="README.md"
if [ -f "$readme" ]; then
  snippet_ver=$(sed -nE 's/^fraiseql = \{ *version *= *"([^"]+)".*/\1/p' "$readme" | head -1)
  if [ -z "$snippet_ver" ]; then
    {
      echo "ERROR: README.md has no 'fraiseql = { version = \"…\"' install snippet."
      echo "       tools/release.sh's bump_readme_install_snippet rewrites that exact line;"
      echo "       if the snippet was reworded, update the helper and this gate together —"
      echo "       a release step whose success message is untrue is what #1146 was filed for."
    } >&2
    exit 1
  fi
  if [ "$snippet_ver" != "$version" ]; then
    {
      echo "ERROR: README.md pins fraiseql ${snippet_ver}, but the workspace is ${version}."
      echo "       A reader copying the documented install line gets the wrong version."
      echo "       Fix the snippet, or re-run the release bump."
    } >&2
    exit 1
  fi
fi

echo "OK: every 'vX.Y.Z released' status line in docs/ matches Cargo.toml (v${version}), and README.md pins ${version}."
