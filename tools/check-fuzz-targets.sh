#!/usr/bin/env bash
# check-fuzz-targets.sh — fail when .github/workflows/fuzz.yml names a fuzz target that
# does not exist in the crate it names.
#
# Background (#1128): the matrix declared `{ crate: fraiseql-core, target: toml_config }`.
# That target was MOVED, not deleted — `5a4a8f86b` (#909) removed
# `fraiseql_core::config::FraiseQLConfig`, the whole TOML tree nothing read, and its fuzz
# target went with it; a live `toml_config` target now lives in
# `crates/fraiseql-server/fuzz/`. Right target, wrong crate. `cargo fuzz build` failed with
# "no bin target named toml_config", and because a scheduled workflow's failure notifies
# nobody, EVERY run in the visible history — back to 2026-05-17, twelve of them — was red.
# The libFuzzer campaign (#441) produced no signal for three months.
#
# ⚠ ONE-DIRECTIONAL BY DESIGN: every matrix row must exist on disk; a target on disk need
# NOT be in the matrix. The matrix is the deliberately-curated "#441 minimum" — 14 of the
# 26 targets in the tree are intentionally outside it, awaiting triage. A bidirectional
# gate would go red on all 14 immediately and be switched off within a week, which is how
# a gate becomes a comment.
#
# Pure bash, no toolchain, no git history → Dagger ShellGates.
#
# Overrides, for testing:
#   FUZZ_TARGETS_WORKFLOW=<path>   parse this workflow instead of the default
#   FUZZ_TARGETS_ROOT=<dir>        resolve crates/ under this directory
set -euo pipefail

if [ -n "${FUZZ_TARGETS_ROOT:-}" ]; then
  cd "$FUZZ_TARGETS_ROOT"
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root"
fi

workflow="${FUZZ_TARGETS_WORKFLOW:-.github/workflows/fuzz.yml}"

if [ ! -f "$workflow" ]; then
  echo "ERROR: fuzz workflow not found: $workflow" >&2
  exit 1
fi

status=0
checked=0

# Matrix rows are single-line `- { crate: X, target: Y }` entries. Parsed with sed rather
# than a YAML library because this gate runs in the minimal ShellGates container.
while IFS=$'\t' read -r crate target; do
  [ -z "$crate" ] && continue
  checked=$((checked + 1))

  manifest="crates/${crate}/fuzz/Cargo.toml"
  source_file="crates/${crate}/fuzz/fuzz_targets/${target}.rs"

  if [ ! -f "$manifest" ]; then
    echo "ERROR: ${crate} / ${target} — no fuzz manifest at ${manifest}"
    status=1
    continue
  fi

  # `name = "<target>"` must appear as a [[bin]] name. Plain grep, no `grep -q`: under
  # `pipefail` a successful `| grep -q` match kills the producer with SIGPIPE (141) and a
  # MATCH reads as a failure.
  if ! grep -E "^name = \"${target}\"$" "$manifest" >/dev/null; then
    echo "ERROR: ${crate} / ${target} — no [[bin]] named \"${target}\" in ${manifest}"
    echo "       cargo fuzz will fail with: no bin target named \`${target}\`"
    # Point at the crate that does have it, if any — this exact defect was a target that
    # moved between crates, and naming the new home turns a puzzle into a one-line fix.
    other="$(grep -lE "^name = \"${target}\"$" crates/*/fuzz/Cargo.toml 2>/dev/null | head -1 || true)"
    if [ -n "$other" ]; then
      echo "       a target with that name exists in: ${other}"
    fi
    status=1
  fi

  if [ ! -f "$source_file" ]; then
    echo "ERROR: ${crate} / ${target} — no source at ${source_file}"
    status=1
  fi
done < <(sed -nE 's/^[[:space:]]*-[[:space:]]*\{[[:space:]]*crate:[[:space:]]*([A-Za-z0-9_-]+),[[:space:]]*target:[[:space:]]*([A-Za-z0-9_-]+)[[:space:]]*\}.*/\1\t\2/p' "$workflow")

if [ "$checked" -eq 0 ]; then
  # A grep-based YAML parser that silently matches nothing would pass forever — the same
  # fabricated-success shape this gate exists to catch.
  echo "ERROR: no matrix rows parsed from ${workflow} — the include: format changed" >&2
  exit 1
fi

if [ "$status" -eq 0 ]; then
  echo "OK: all ${checked} fuzz matrix targets exist in the crate they name."
fi
exit "$status"
