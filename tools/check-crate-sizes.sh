#!/usr/bin/env bash
# check-crate-sizes.sh — each crate's production code within its line budget, and every
# crate with a budget. The counting rules, and why a budget counts production code only
# (#1882), are in tools/check-crate-sizes.py; this is its entry point for the
# `lint-crate-sizes` make target and the Dagger ShellGates leg.
set -euo pipefail
exec python3 "$(dirname "${BASH_SOURCE[0]}")/check-crate-sizes.py" "$@"
