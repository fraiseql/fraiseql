#!/usr/bin/env bash
# check-deadlines.sh — fail if any accepted-advisory deadline in deny.toml has lapsed.
#
# deny.toml has no structured deadline field; per-advisory risk-acceptance
# deadlines live in comments using the convention `# deadline: YYYY-MM-DD`.
# This gate parses those and fails the build once a deadline is in the past,
# forcing the documented re-evaluation rather than letting an accepted advisory
# linger silently. (Header prose uses `# Re-evaluate by:` and is intentionally
# not enforced here.)
#
# Override "today" for testing: DEADLINE_CHECK_TODAY=YYYY-MM-DD bash tools/check-deadlines.sh
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

today="${DEADLINE_CHECK_TODAY:-$(date +%F)}"
found=0
while IFS= read -r line; do
  lineno="${line%%:*}"
  date_str="$(printf '%s' "$line" | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}' | head -1)"
  [ -z "$date_str" ] && continue
  # ISO dates compare correctly as lexical strings.
  if [[ "$date_str" < "$today" ]]; then
    echo "ERROR: lapsed advisory deadline $date_str at deny.toml:$lineno — re-evaluate the risk acceptance."
    found=1
  fi
done < <(grep -niE '#[[:space:]]*deadline:[[:space:]]*[0-9]{4}-[0-9]{2}-[0-9]{2}' deny.toml)

if [ "$found" -eq 0 ]; then
  echo "OK: no lapsed advisory deadlines in deny.toml (today $today)."
fi
exit "$found"
