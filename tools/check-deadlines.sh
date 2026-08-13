#!/usr/bin/env bash
# check-deadlines.sh — fail if any accepted-advisory deadline has lapsed.
#
# Neither dependency tool has a native expiry mechanism: cargo-deny 0.19 accepts
# only `id` and `reason` on an `[advisories].ignore` entry (anything else is an
# `unexpected-keys` config error), and cargo-audit's ignore list is bare strings.
# So a risk acceptance would otherwise be permanent by default. The convention
# is a `# deadline: YYYY-MM-DD` comment, and THIS gate is the only thing that
# enforces it — which is also why it scans every file that carries acceptances,
# not just the first one anybody thought of (#1103).
#
# Semantics: the deadline is the FIRST day the acceptance is no longer valid.
# `# deadline: 2026-12-01` passes on 2026-11-30 and fails on 2026-12-01. The
# comparison used to be strict, which quietly granted one extra day and left
# the convention ambiguous about whether the date was inclusive.
#
# Warning window: within DEADLINE_WARN_DAYS (default 30) the gate prints a WARN
# line and still exits 0. A deadline reddens a REQUIRED check on a *date* rather
# than on a push — every open branch at once, with no commit to bisect (#978's
# shape). The window makes it visible on every preflight run for weeks first.
#
# Overrides, for testing:
#   DEADLINE_CHECK_TODAY=YYYY-MM-DD   pretend today is this date
#   DEADLINE_CHECK_FILES="a.toml b"   scan these files instead of the defaults
#   DEADLINE_WARN_DAYS=N              warning-window width in days
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

today="${DEADLINE_CHECK_TODAY:-$(date +%F)}"
warn_days="${DEADLINE_WARN_DAYS:-30}"

# Every file carrying a dated risk acceptance. deny.toml and .cargo/audit.toml
# are kept in lockstep by advisory id (tools/check-audit-lockstep.sh) but their
# deadline comments are independent prose — scanning only one let three of
# audit.toml's lapse by two months without any gate noticing.
default_files="deny.toml .cargo/audit.toml"
# Word-splitting is the interface here: the override is a space-separated list.
# shellcheck disable=SC2206
files=(${DEADLINE_CHECK_FILES:-$default_files})

# The warning window needs date arithmetic; the hard check is a lexical ISO
# comparison and needs none. Where `date -d` is unavailable (BSD userland), warn
# support degrades to silence rather than taking the gate down with it.
warn_until="$(date -d "$today + $warn_days days" +%F 2>/dev/null || true)"

found=0
for file in "${files[@]}"; do
  if [ ! -f "$file" ]; then
    # A gate that greps a path list is a no-op the moment a file is renamed out
    # from under it — and it would still print OK. Refuse instead.
    echo "ERROR: deadline scan target not found: $file"
    found=1
    continue
  fi
  while IFS= read -r line; do
    lineno="${line%%:*}"
    date_str="$(printf '%s' "$line" | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}' | head -1)"
    [ -z "$date_str" ] && continue
    # ISO dates compare correctly as lexical strings.
    if [[ ! "$today" < "$date_str" ]]; then
      echo "ERROR: lapsed advisory deadline $date_str at $file:$lineno — re-evaluate the risk acceptance."
      found=1
    elif [ -n "$warn_until" ] && [[ ! "$warn_until" < "$date_str" ]]; then
      echo "WARN: advisory deadline $date_str at $file:$lineno lapses within $warn_days days — schedule the re-evaluation now."
    fi
  done < <(grep -niE '#[[:space:]]*deadline:[[:space:]]*[0-9]{4}-[0-9]{2}-[0-9]{2}' "$file" || true)
done

if [ "$found" -eq 0 ]; then
  echo "OK: no lapsed advisory deadlines in ${files[*]} (today $today)."
fi
exit "$found"
