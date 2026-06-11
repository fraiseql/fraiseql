#!/usr/bin/env bash
# check-audit-lockstep.sh — fail if deny.toml and .cargo/audit.toml drift apart.
#
# Background: deny.toml (cargo-deny, the Dagger security gate) and
# .cargo/audit.toml (cargo-audit, the `make audit` gate) each carry an
# [advisories].ignore list. When an advisory is accepted in one but not the
# other, `make audit` / `make security` exit non-zero on a clean tree while CI
# stays green — training developers to ignore the failure. This gate requires
# the two ignore sets to match exactly.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Extract the double-quoted RUSTSEC ids from $1. In deny.toml these are the
# `id = "RUSTSEC-…"` table fields; in audit.toml the bare `"RUSTSEC-…"` strings.
# Prose mentions inside `reason = "…"` strings are not wrapped in their own
# quotes, so they are not matched.
ids_in() {
  grep -oE '"RUSTSEC-[0-9]{4}-[0-9]{4,}"' "$1" | tr -d '"' | sort -u
}

deny_ids="$(ids_in deny.toml)"
audit_ids="$(ids_in .cargo/audit.toml)"

only_deny="$(comm -23 <(printf '%s\n' "$deny_ids") <(printf '%s\n' "$audit_ids"))"
only_audit="$(comm -13 <(printf '%s\n' "$deny_ids") <(printf '%s\n' "$audit_ids"))"

status=0
if [ -n "$only_deny" ]; then
  echo "ERROR: advisories ignored in deny.toml but NOT in .cargo/audit.toml:"
  printf '%s\n' "$only_deny" | sed 's/^/  - /'
  status=1
fi
if [ -n "$only_audit" ]; then
  echo "ERROR: advisories ignored in .cargo/audit.toml but NOT in deny.toml:"
  printf '%s\n' "$only_audit" | sed 's/^/  - /'
  status=1
fi

if [ "$status" -eq 0 ]; then
  echo "OK: deny.toml and .cargo/audit.toml advisory ignore lists are in lockstep."
fi
exit "$status"
