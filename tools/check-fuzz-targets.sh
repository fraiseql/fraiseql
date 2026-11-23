#!/usr/bin/env bash
# check-fuzz-targets.sh — fail when .github/workflows/fuzz.yml names a fuzz target that
# does not exist in the crate it names, or a fuzz target on disk has no row and no stated
# exemption.
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
# Both directions. Every matrix row must exist on disk, and every target on disk must
# have a matrix row or a line in tools/fuzz-unscheduled-targets.txt saying why not
# (#1847). The second direction was left out while 11 of the 24 targets on disk waited
# for triage outside the matrix; they compiled on every PR and fuzzed nothing, and the
# Arrow targets the workflow is modelled on were among them. They are all scheduled now,
# so the reverse check starts green, and a new target that is written but never
# scheduled fails here instead of joining them.
#
# Pure bash, no toolchain, no git history → Dagger ShellGates.
#
# Overrides, for testing:
#   FUZZ_TARGETS_WORKFLOW=<path>   parse this workflow instead of the default
#   FUZZ_TARGETS_ROOT=<dir>        resolve crates/ under this directory
#   FUZZ_UNSCHEDULED=<path>        read this exemption list instead of the default
set -euo pipefail

if [ -n "${FUZZ_TARGETS_ROOT:-}" ]; then
  cd "$FUZZ_TARGETS_ROOT"
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root"
fi

workflow="${FUZZ_TARGETS_WORKFLOW:-.github/workflows/fuzz.yml}"
unscheduled="${FUZZ_UNSCHEDULED:-tools/fuzz-unscheduled-targets.txt}"

if [ ! -f "$workflow" ]; then
  echo "ERROR: fuzz workflow not found: $workflow" >&2
  exit 1
fi

status=0
checked=0
scheduled=""
# `crate/target<TAB>budget` for each row that carries its own budget, checked below.
row_budgets=""

# Matrix rows are single-line `- { crate: X, target: Y }` entries, optionally with a
# third key, `budget: N`, capping that row's fuzz time below the campaign's (a run gets
# the smaller of the two). Parsed with sed rather than a YAML library because this gate
# runs in the minimal ShellGates container.
row_re='^[[:space:]]*-[[:space:]]*\{[[:space:]]*crate:[[:space:]]*([A-Za-z0-9_-]+),[[:space:]]*target:[[:space:]]*([A-Za-z0-9_-]+)[[:space:]]*(,[[:space:]]*budget:[[:space:]]*([^[:space:],}]+)[[:space:]]*)?\}.*'

# Every line that starts a `crate:` row must be one this gate reads. A row it cannot
# read is a row nothing checks: an unknown key, or a budget spelled another way, would
# leave its target unchecked while the workflow still runs it as written.
while IFS= read -r line; do
  if ! grep -E -- "$row_re" <<<"$line" >/dev/null; then
    echo "ERROR: a matrix row this gate cannot read: ${line#"${line%%[![:space:]]*}"}"
    echo "       Rows are \`- { crate: X, target: Y }\` or \`- { crate: X, target: Y, budget: N }\`."
    status=1
  fi
done < <(grep -E '^[[:space:]]*-[[:space:]]*\{[[:space:]]*crate:' "$workflow" || true)

while IFS=$'\t' read -r crate target budget; do
  [ -z "$crate" ] && continue
  checked=$((checked + 1))
  scheduled="${scheduled}${crate}/${target}"$'\n'
  [ -n "$budget" ] && row_budgets="${row_budgets}${crate}/${target}"$'\t'"${budget}"$'\n'

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
done < <(sed -nE "s/${row_re}/\\1\t\\2\t\\4/p" "$workflow")

if [ "$checked" -eq 0 ]; then
  # A grep-based YAML parser that silently matches nothing would pass forever — the same
  # fabricated-success shape this gate exists to catch.
  echo "ERROR: no matrix rows parsed from ${workflow} — the include: format changed" >&2
  exit 1
fi

# ── Every target on disk is scheduled, or says why not ───────────────────────────────
#
# The exemption list is `<crate>/<target>  <why>`, one per line, `#` comments allowed. A
# line without a reason is refused: an exemption nobody can review is how the 11 waited.
# An exemption naming a scheduled target, or a target that does not exist, is refused
# too, so the list cannot go stale.
exempt=""
if [ -f "$unscheduled" ]; then
  while IFS= read -r line; do
    case "$line" in ''|'#'*) continue ;; esac
    name="${line%%[[:space:]]*}"
    reason="${line#"$name"}"
    if [ -z "${reason//[[:space:]]/}" ]; then
      echo "ERROR: ${unscheduled}: \`${name}\` has no reason; write why it is not scheduled"
      status=1
    fi
    if [ ! -f "crates/${name%%/*}/fuzz/fuzz_targets/${name#*/}.rs" ]; then
      echo "ERROR: ${unscheduled}: \`${name}\` is not a fuzz target on disk; remove the line"
      status=1
    fi
    if grep -Fx -- "$name" <<<"$scheduled" >/dev/null; then
      echo "ERROR: ${unscheduled}: \`${name}\` is scheduled in ${workflow}; remove the line"
      status=1
    fi
    exempt="${exempt}${name}"$'\n'
  done <"$unscheduled"
fi

on_disk=0
for source_file in crates/*/fuzz/fuzz_targets/*.rs; do
  [ -f "$source_file" ] || continue
  on_disk=$((on_disk + 1))
  crate="${source_file#crates/}"
  crate="${crate%%/*}"
  target="$(basename "$source_file" .rs)"
  if ! grep -Fx -- "${crate}/${target}" <<<"$scheduled" >/dev/null \
    && ! grep -Fx -- "${crate}/${target}" <<<"$exempt" >/dev/null; then
    echo "ERROR: ${crate} / ${target} — a fuzz target on disk that ${workflow} never runs."
    echo "       Add a matrix row, or a line in ${unscheduled} saying why not."
    status=1
  fi
done

if [ "$on_disk" -eq 0 ]; then
  echo "ERROR: no fuzz targets found under crates/*/fuzz/fuzz_targets/ — the layout changed" >&2
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

# ── A row's own budget ───────────────────────────────────────────────────────────────
#
# A cap: the row runs for the smaller of it and the campaign's budget, so a dispatch's
# short verification run stays short. It must be a whole number of seconds, below the
# scheduled budget (a cap at or above it changes nothing, and reads as if it did), and,
# on its own, inside the job timeout with the same build headroom as above.
capped=""
while IFS=$'\t' read -r row budget; do
  [ -z "$row" ] && continue
  case "$budget" in
    '' | *[!0-9]* | 0*)
      echo "ERROR: ${row}: budget \`${budget}\` is not a whole number of seconds above 0."
      status=1
      continue
      ;;
  esac
  if [ "$budget" -ge "$schedule_budget" ]; then
    echo "ERROR: ${row}: its budget ${budget}s is not below the scheduled ${schedule_budget}s."
    echo "       A row's budget caps the campaign's, so this one changes nothing. Lower it,"
    echo "       or remove it."
    status=1
  fi
  if [ $((budget + BUILD_HEADROOM_SECS)) -gt "$timeout_secs" ]; then
    echo "ERROR: ${row}: its budget ${budget}s + ${BUILD_HEADROOM_SECS}s of install/build headroom"
    echo "       = $((budget + BUILD_HEADROOM_SECS))s > timeout-minutes ${timeout_min} (${timeout_secs}s)."
    status=1
  fi
  capped="${capped} ${row} capped at ${budget}s;"
done <<<"$row_budgets"

if [ "$status" -eq 0 ]; then
  [ -n "$capped" ] && echo "OK: row budgets:${capped%;}."
  echo "OK: all ${checked} fuzz matrix targets exist in the crate they name, and all"
  echo "    ${on_disk} targets on disk are scheduled or exempted with a reason;"
  echo "    dispatch ${dispatch_budget}s and scheduled ${schedule_budget}s both fit in ${timeout_min}m."
fi
exit "$status"
