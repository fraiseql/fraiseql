#!/usr/bin/env bash
# check-not-implemented.sh — no route answers 501 through `not_implemented(` except the one
# allowed site (ADR-0007 §5, the shipped gate, check 8).
#
# A route that exists and answers "not implemented" is a capability the registry reports
# as served and the deployment does not serve. The one allowance is revoke's conditional
# 501 (`routes/studio/auth_users.rs`): the route is real, and it answers 501 only when no
# revocation store is configured, which an operator can fix (#749). Anything else is a
# capability to finish or a route to remove.
#
# Positional: every banned call is reported by file and line, and the allowance is a file
# AND a count, so a second 501 added beside the allowed one fails. Shrink-only: an allowed
# file whose count falls fails too, so the entry goes when the call does. Importing the
# helper under another name is refused, since the scan reads the name.
#
# Exit 0: clean. Exit 1: a violation. Exit 2: the scan found no definition of the helper,
# so a moved or renamed helper cannot turn the gate into a scan of nothing.
set -euo pipefail
root="${NOT_IMPLEMENTED_ROOT:-$(git rev-parse --show-toplevel)}"
cd "$root"

routes="crates/fraiseql-server/src/routes"
# file:count — the calls each file may make.
allowed=("${routes}/studio/auth_users.rs:1")

if ! grep -rqE '^\s*(pub(\([a-z]+\))?\s+)?fn not_implemented\(' "$routes" --include='*.rs' 2>/dev/null; then
  echo "ERROR: no \`fn not_implemented(\` under ${routes}: the helper moved or was renamed;" >&2
  echo "       point this gate at it rather than letting it scan nothing." >&2
  exit 2
fi

violations=0

# Calls, by file:line: not a comment line, not the definition.
calls=$(grep -rnE 'not_implemented\(' "$routes" --include='*.rs' \
  | grep -vE '^[^:]+:[0-9]+:\s*//' \
  | grep -vE '^[^:]+:[0-9]+:.*fn not_implemented\(' || true)

aliases=$(grep -rnE 'not_implemented\s+as\s' "$routes" --include='*.rs' || true)
if [ -n "$aliases" ]; then
  echo "ERROR: \`not_implemented\` imported under another name; this gate reads the name:" >&2
  echo "$aliases" >&2
  violations=1
fi

declare -A allowance=()
for entry in "${allowed[@]}"; do
  allowance["${entry%:*}"]="${entry##*:}"
done

declare -A seen=()
while IFS= read -r line; do
  [ -z "$line" ] && continue
  file="${line%%:*}"
  seen["$file"]=$(( ${seen["$file"]:-0} + 1 ))
done <<< "$calls"

for file in "${!seen[@]}"; do
  limit="${allowance["$file"]:-0}"
  if [ "${seen["$file"]}" -gt "$limit" ]; then
    echo "ERROR: ${file} answers 501 through not_implemented( ${seen["$file"]} time(s), allowed ${limit}:" >&2
    grep -F "${file}:" <<< "$calls" >&2
    violations=1
  fi
done

for file in "${!allowance[@]}"; do
  if [ "${seen["$file"]:-0}" -lt "${allowance["$file"]}" ]; then
    echo "ERROR: ${file} is allowed ${allowance["$file"]} not_implemented( call(s) and makes" >&2
    echo "       ${seen["$file"]:-0}: lower its entry in this gate (the allowance only shrinks)." >&2
    violations=1
  fi
done

if [ "$violations" -ne 0 ]; then
  exit 1
fi
echo "OK: not_implemented( under ${routes} only at its allowed site(s)."
