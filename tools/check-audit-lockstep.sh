#!/usr/bin/env bash
# check-audit-lockstep.sh — fail if the three files that record accepted advisories
# drift apart: deny.toml, .cargo/audit.toml, and docs/dependency-risk-policy.md.
#
# Background: deny.toml (cargo-deny, the Dagger security gate) and .cargo/audit.toml
# (cargo-audit, the `make audit` gate) each carry an [advisories].ignore list. When an
# advisory is accepted in one but not the other, `make audit` / `make security` exit
# non-zero on a clean tree while CI stays green — training developers to ignore the
# failure.
#
# ⚠ Extended 2026-08-16 to a THIRD file (#1110). The two-file check passed for months
# while the published policy — the only one of the three that users and auditors read —
# listed **four** of the eight accepted advisories and carried three deadlines that
# disagreed with deny.toml, two of them reading as already lapsed. Both machine-read
# files agreed with each other, which is exactly why comparing only those two could not
# see it.
#
# What this gate does NOT check is whether a stated dependency path is true; that is
# tools/check-advisory-paths.sh, which needs cargo and so runs in the security leg.
#
# Overrides, for testing:
#   LOCKSTEP_DENY=<path> LOCKSTEP_AUDIT=<path> LOCKSTEP_DOC=<path>
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

deny_file="${LOCKSTEP_DENY:-deny.toml}"
audit_file="${LOCKSTEP_AUDIT:-.cargo/audit.toml}"
doc_file="${LOCKSTEP_DOC:-docs/dependency-risk-policy.md}"

for f in "$deny_file" "$audit_file" "$doc_file"; do
  if [ ! -f "$f" ]; then
    echo "ERROR: lockstep scan target not found: $f" >&2
    exit 1
  fi
done

# Extract the double-quoted RUSTSEC ids from $1. In deny.toml these are the
# `id = "RUSTSEC-…"` table fields; in audit.toml the bare `"RUSTSEC-…"` strings.
# Prose mentions inside `reason = "…"` strings are not wrapped in their own
# quotes, so they are not matched.
ids_in() {
  grep -oE '"RUSTSEC-[0-9]{4}-[0-9]{4,}"' "$1" | tr -d '"' | sort -u || true
}

# The published table's rows: `| RUSTSEC-… | crate | path | exposure | mitigation | date |`.
# Anchored at the line start so prose mentioning an id elsewhere in the document is not
# mistaken for an acceptance row.
doc_ids() {
  grep -oE '^\| *RUSTSEC-[0-9]{4}-[0-9]{4,}' "$doc_file" \
    | grep -oE 'RUSTSEC-[0-9]{4}-[0-9]{4,}' | sort -u || true
}

# id<TAB>deadline for deny.toml. Each `{id = …, reason = "… Deadline YYYY-MM-DD …"}`
# entry is one line, so the id and its date are read from the same line — no positional
# guessing from the surrounding comment blocks, which deliberately cover several ids at
# once (0098/0099/0104 share one, 0194/0195 share another).
deny_deadlines() {
  grep -oE '\{ *id *= *"RUSTSEC-[0-9]{4}-[0-9]{4,}".*' "$deny_file" \
    | sed -nE 's/.*"(RUSTSEC-[0-9]{4}-[0-9]{4,})".*[Dd]eadline[: ]+([0-9]{4}-[0-9]{2}-[0-9]{2}).*/\1\t\2/p' \
    | sort -u || true
}

# id<TAB>deadline for the published table: last non-empty cell of the row.
doc_deadlines() {
  awk -F'|' '
    /^\| *RUSTSEC-[0-9]{4}-[0-9]{4,}/ {
      id = $2; gsub(/[^A-Z0-9-]/, "", id)
      for (i = NF; i > 0; i--) {
        cell = $i
        gsub(/^[ \t]+|[ \t]+$/, "", cell)
        if (cell ~ /^[0-9]{4}-[0-9]{2}-[0-9]{2}$/) { print id "\t" cell; break }
      }
    }
  ' "$doc_file" | sort -u
}

deny_ids="$(ids_in "$deny_file")"
audit_ids="$(ids_in "$audit_file")"
policy_ids="$(doc_ids)"

status=0

report_diff() {
  local label_a="$1" label_b="$2" a="$3" b="$4"
  local only
  only="$(comm -23 <(printf '%s\n' "$a") <(printf '%s\n' "$b"))"
  if [ -n "$only" ]; then
    echo "ERROR: advisories accepted in ${label_a} but NOT in ${label_b}:"
    printf '%s\n' "$only" | sed 's/^/  - /'
    status=1
  fi
}

report_diff "$deny_file" "$audit_file" "$deny_ids" "$audit_ids"
report_diff "$audit_file" "$deny_file" "$audit_ids" "$deny_ids"
report_diff "$deny_file" "$doc_file" "$deny_ids" "$policy_ids"
report_diff "$doc_file" "$deny_file" "$policy_ids" "$deny_ids"

# ── Deadlines: the published date must be the one deny.toml carries ──────────
#
# ⚠ Do NOT solve this by adding the doc to DEADLINE_CHECK_FILES. check-deadlines.sh
# greps for `# deadline: YYYY-MM-DD` COMMENT lines, which a markdown table column is
# not — it would scan the file, match nothing, and read as coverage. With equality
# enforced here, check-deadlines.sh's existing scan of deny.toml covers the published
# dates transitively.
deny_dl="$(deny_deadlines)"
doc_dl="$(doc_deadlines)"

while IFS=$'\t' read -r id date; do
  [ -z "$id" ] && continue
  want="$(printf '%s\n' "$deny_dl" | awk -F'\t' -v i="$id" '$1 == i {print $2}')"
  if [ -z "$want" ]; then
    echo "ERROR: ${doc_file} publishes a deadline for ${id} but ${deny_file} states none."
    echo "       Every acceptance needs a 'Deadline YYYY-MM-DD' in its deny.toml reason."
    status=1
  elif [ "$want" != "$date" ]; then
    echo "ERROR: deadline mismatch for ${id}: ${doc_file} says ${date}, ${deny_file} says ${want}."
    echo "       deny.toml is the source of truth — correct the published table."
    status=1
  fi
done < <(printf '%s\n' "$doc_dl")

# An accepted advisory with no published deadline at all: the row exists but its date
# cell is missing or malformed, which would otherwise slip through the loop above.
while IFS= read -r id; do
  [ -z "$id" ] && continue
  if [ -z "$(printf '%s\n' "$doc_dl" | awk -F'\t' -v i="$id" '$1 == i {print $2}')" ]; then
    echo "ERROR: ${doc_file} lists ${id} without a YYYY-MM-DD deadline cell."
    status=1
  fi
done < <(printf '%s\n' "$policy_ids")

if [ "$status" -eq 0 ]; then
  n="$(printf '%s\n' "$deny_ids" | grep -cE '^RUSTSEC-' || true)"
  echo "OK: ${n} accepted advisories agree across ${deny_file}, ${audit_file} and ${doc_file} (ids and deadlines)."
fi
exit "$status"
