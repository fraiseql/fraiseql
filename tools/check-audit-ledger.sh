#!/usr/bin/env bash
# check-audit-ledger.sh — fail when a finding in the tracked security audit has no row with
# evidence in the findings ledger.
#
# Background: the audit of 2026-06-11 (docs/security/audits/2026-06-11.md) found one critical,
# forty-six high and about thirty medium findings. The fixes shipped across v2.7.0 and later
# and CHANGELOG.md cites them by ID, but nothing tied the audit to those citations: the audit
# file itself sat untracked for three months and the remediation plan still read "Not Started"
# for phases whose fixes had shipped. The ledger (docs/security/audits/2026-06-11-status.md) is
# the one place that answers "what happened to finding X". This gate keeps it complete: every
# `### C<n>` / `### H<n>` heading and every bold MEDIUM bullet title in the audit must have a
# ledger row whose Evidence cell (the fourth) is not empty and not a placeholder.
#
# The gate also refuses an audit from which it extracts no findings: a gate that finds nothing
# to check passes for the wrong reason.
#
# Overrides, for testing:  AUDIT_LEDGER_AUDIT=<path> AUDIT_LEDGER_LEDGER=<path>
set -euo pipefail

audit="${AUDIT_LEDGER_AUDIT:-}"
ledger="${AUDIT_LEDGER_LEDGER:-}"
if [ -z "$audit" ] || [ -z "$ledger" ]; then
  cd "$(git rev-parse --show-toplevel)"
  audit="${audit:-docs/security/audits/2026-06-11.md}"
  ledger="${ledger:-docs/security/audits/2026-06-11-status.md}"
fi

for f in "$audit" "$ledger"; do
  if [ ! -f "$f" ]; then
    echo "ERROR: audit ledger scan target not found: $f" >&2
    exit 1
  fi
done

# Findings: `### C1`, `### H12` headings anywhere; bold bullet titles inside `## MEDIUM`.
mapfile -t ids < <(grep -oE '^### (C|H)[0-9]+\b' "$audit" | sed -E 's/^### //')
mapfile -t medium_titles < <(awk '
  /^## /        { in_medium = ($0 == "## MEDIUM") }
  in_medium && /^- \*\*/ {
    t = $0; sub(/^- \*\*/, "", t); sub(/\*\*.*$/, "", t); print t
  }' "$audit")

total=$(( ${#ids[@]} + ${#medium_titles[@]} ))
if [ "$total" -eq 0 ]; then
  echo "ERROR: no findings extracted from $audit (expected ### C/H headings or ## MEDIUM bullets)" >&2
  exit 1
fi

# evidence_of <finding-key> → prints the trimmed fourth cell of the first ledger row whose
# first cell equals the key; prints nothing when there is no such row.
evidence_of() {
  local key="$1"
  awk -F'|' -v key="$key" '
    NF >= 5 {
      k = $2; gsub(/^[ \t]+|[ \t]+$/, "", k)
      if (k == key) { e = $5; gsub(/^[ \t]+|[ \t]+$/, "", e); print e; exit }
    }' "$ledger"
}

failures=0
check() {
  local key="$1" label="$2"
  local row
  if ! grep -qF -- "| $key |" "$ledger"; then
    echo "MISSING: $label has no ledger row" >&2
    failures=$((failures + 1))
    return
  fi
  row="$(evidence_of "$key")"
  case "$row" in
    ""|"TBD"|"tbd"|"—"|"-"|"?")
      echo "NO EVIDENCE: $label has an empty or placeholder evidence cell" >&2
      failures=$((failures + 1))
      ;;
  esac
}

for id in "${ids[@]}"; do check "$id" "$id"; done
for t in "${medium_titles[@]}"; do check "**$t**" "medium '$t'"; done

if [ "$failures" -gt 0 ]; then
  echo "audit ledger: $failures of $total findings lack a row with evidence ($ledger)" >&2
  exit 1
fi
echo "audit ledger: ok — $total findings (${#ids[@]} C/H, ${#medium_titles[@]} medium) each have evidence"
