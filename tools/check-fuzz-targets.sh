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

# ── The fuzz budget must fit inside the job timeout ──────────────────────────────────
#
# #1141: `timeout-minutes: 30` with a per-target budget of 1800s meant the fuzz step
# alone consumed the entire job cap, before `cargo install cargo-fuzz` and a nightly
# build of the harness. Every dispatched leg built successfully and was then cancelled
# with the fuzzer still running — and `cancelled` reads like somebody pressed stop, not
# like a misconfiguration, so it went unexamined.
#
# BUILD_HEADROOM_SECS is what the install + cold nightly build has been observed to need.
# The relationship is arithmetic in one file, so it is checkable rather than a comment
# that goes stale the next time either number moves.
BUILD_HEADROOM_SECS="${FUZZ_BUILD_HEADROOM_SECS:-1500}"

timeout_min="$(sed -nE 's/^[[:space:]]*timeout-minutes:[[:space:]]*([0-9]+).*/\1/p' "$workflow" | head -1)"
dispatch_budget="$(sed -nE 's/^[[:space:]]*default:[[:space:]]*"([0-9]+)".*/\1/p' "$workflow" | head -1)"
schedule_budget="$(sed -nE "s/.*max_total_time[[:space:]]*\|\|[[:space:]]*'([0-9]+)'.*/\1/p" "$workflow" | head -1)"

if [ -z "$timeout_min" ] || [ -z "$dispatch_budget" ] || [ -z "$schedule_budget" ]; then
  echo "ERROR: could not read timeout-minutes / the dispatch default / the schedule default" >&2
  echo "       from ${workflow} — the shape changed and this check went blind." >&2
  exit 1
fi

timeout_secs=$((timeout_min * 60))
for pair in "workflow_dispatch:${dispatch_budget}" "schedule:${schedule_budget}"; do
  trigger="${pair%%:*}"
  budget="${pair##*:}"
  if [ $((budget + BUILD_HEADROOM_SECS)) -gt "$timeout_secs" ]; then
    echo "ERROR: the ${trigger} fuzz budget cannot finish inside the job timeout."
    echo "       budget ${budget}s + ${BUILD_HEADROOM_SECS}s of install/build headroom"
    echo "       = $((budget + BUILD_HEADROOM_SECS))s > timeout-minutes ${timeout_min} (${timeout_secs}s)."
    echo "       Every leg would be CANCELLED mid-fuzz, which reads as somebody pressing stop."
    echo "       Raise timeout-minutes, or lower the budget."
    status=1
  fi
done

if [ "$status" -eq 0 ]; then
  echo "OK: all ${checked} fuzz matrix targets exist in the crate they name;"
  echo "    dispatch ${dispatch_budget}s and scheduled ${schedule_budget}s both fit in ${timeout_min}m."
fi
exit "$status"
