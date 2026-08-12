#!/usr/bin/env bash
# check-guard-parity.sh — fail if a crate hand-rolls an outbound-address guard or a
# production-environment check instead of using `fraiseql-guard`.
#
# Background (issues #776, #802, #816, #836): the workspace accumulated EIGHT hand-rolled
# address predicates and TWO production detectors. Every one was individually reasonable.
# Collectively they were exploitable: five accepted `::ffff:169.254.169.254`, which a
# dual-stack socket routes to the instance-metadata service; four accepted `0.0.0.0/8`;
# none covered NAT64. Two copies lived in the *same crate* and disagreed with each other.
# The two production detectors read the same `FRAISEQL_ENV` with opposite defaults, so a
# server that believed it was in production ran alongside an observer subsystem that did
# not — and honoured a development-only SSRF bypass.
#
# The failure mode is not "someone writes a bad guard". It is "someone writes a *correct*
# guard, and it drifts from its siblings over the following year". So this gate does not
# check quality; it checks that there is only one.
#
# If you are adding an outbound guard: call `fraiseql_guard::net::{is_blocked_ip,
# blocked_host_reason}`. If you need the deployment posture: call
# `fraiseql_guard::deployment::is_production`. If the shared guard is missing a range you
# need, add it there — with a row in `fraiseql_guard::net::vectors` — and every crate gets it.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

found=0

# The one crate allowed to implement these.
GUARD_CRATE='crates/fraiseql-guard/'

# Call sites that classify addresses for a purpose that is not an outbound guard.
# Listed individually, with a reason, so that adding to this list is a visible decision
# rather than a widened regex.
#
#   rate_limit/key.rs   — buckets client IPs for rate-limit keying. Inbound, not outbound.
#   dialect/postgres.rs — emits SQL for the GraphQL network-address filter operators
#   sql_gen/mod.rs        (`isCarrierGrade`, `isPrivate`, …). These are user-facing query
#                         predicates over stored `inet` columns; the CIDR literals are the
#                         feature, not a guard.
EXEMPT_PATHS=(
  'crates/fraiseql-server/src/middleware/rate_limit/key.rs'
  'crates/fraiseql-db/src/dialect/postgres.rs'
  'crates/fraiseql-wire/src/operators/sql_gen/mod.rs'
)

# Emits `path:line:content` for every non-test line of every crate source file.
#
# Test code legitimately names blocked addresses — asserting that `169.254.169.254` is
# refused is the whole point of a guard test. Files under tests/, `*_tests.rs` and
# `tests.rs` are skipped wholesale; for inline `#[cfg(test)]` modules the scan stops at
# the attribute, which works because this repo places them at the end of the file
# (`make lint-tests-layout` forbids new inline `mod tests {` blocks outright).
scan_source_lines() {
  find crates -name '*.rs' -not -path '*/target/*' \
    -not -path '*/tests/*' -not -path '*/benches/*' \
    -not -name 'tests.rs' -not -name '*_tests.rs' -print0 \
    | xargs -0 awk '
        FNR == 1 { in_test = 0 }
        /#\[cfg\(test\)\]/ { in_test = 1 }
        in_test { next }
        # Skip comments — prose about a range is not a check on one.
        /^[[:space:]]*(\/\/|\*)/ { next }
        { print FILENAME ":" FNR ":" $0 }
      '
}

# Drops the exempt paths from a `path:line:content` stream.
drop_exempt() {
  local stream
  stream=$(cat)
  local path
  for path in "${EXEMPT_PATHS[@]}"; do
    stream=$(printf '%s\n' "$stream" | grep -v "^${path}:" || true)
  done
  printf '%s\n' "$stream"
}

# ---------------------------------------------------------------------------
# 1. Hand-rolled address classification.
#
# The tell is a reserved-range constant appearing in a comparison outside the guard
# crate: the RFC 1918 octets, the link-local /16, the CGNAT mask, the ULA/link-local
# IPv6 prefixes, or a literal metadata address.
# ---------------------------------------------------------------------------
addr_matches=$(
  scan_source_lines \
    | grep -E '(is_loopback\(\)|is_unique_local\(\)|is_unicast_link_local\(\)|0xfe00|0xffc0|0xfc00|0xfe80|169\.254\.169\.254|100\.64\.0\.0)' \
    | grep -v "^${GUARD_CRATE}" \
    | drop_exempt \
    | grep -v '^$' || true
)
if [ -n "$addr_matches" ]; then
  echo "ERROR: hand-rolled outbound-address classification outside fraiseql-guard:" >&2
  echo "$addr_matches" >&2
  echo "" >&2
  echo "  Use fraiseql_guard::net::is_blocked_ip / blocked_host_reason instead." >&2
  echo "  Missing a range? Add it to fraiseql-guard/src/net/ with a vectors:: row." >&2
  found=1
fi

# ---------------------------------------------------------------------------
# 2. Hand-rolled production detection.
#
# Any read of FRAISEQL_ENV / FRAISEQL_PROFILE / KUBERNETES_SERVICE_HOST outside the
# guard crate is a second answer to a question that must have one. Reading them to
# *report* the environment is fine; the tell is `env::var` on those names.
# ---------------------------------------------------------------------------
env_matches=$(
  scan_source_lines \
    | grep -E 'env::var(_os)?\(\s*"(FRAISEQL_ENV|FRAISEQL_PROFILE|KUBERNETES_SERVICE_HOST)"' \
    | grep -v "^${GUARD_CRATE}" \
    | drop_exempt \
    | grep -v '^$' || true
)
if [ -n "$env_matches" ]; then
  echo "ERROR: hand-rolled production detection outside fraiseql-guard:" >&2
  echo "$env_matches" >&2
  echo "" >&2
  echo "  Use fraiseql_guard::deployment::is_production instead." >&2
  echo "  Two callers reading one env var with two defaults is how #836 shipped." >&2
  found=1
fi

# ---------------------------------------------------------------------------
# 3. Escape hatches that skip the posture check.
#
# An `*_ALLOW_INSECURE` / `*_ALLOW_PLAINTEXT` variable read directly, rather than through
# `deployment::env_opt_in` + `insecure_bypass_allowed`, is a bypass with no production
# gate. Two of the product's four hatches shipped that way (#882).
# ---------------------------------------------------------------------------
bypass_matches=$(
  scan_source_lines \
    | grep -E 'env::var(_os)?\(\s*"[A-Z_]*(ALLOW_INSECURE|ALLOW_PLAINTEXT|SKIP_VERIFY|DISABLE_TLS)[A-Z_]*"' \
    | grep -v "^${GUARD_CRATE}" \
    | drop_exempt \
    | grep -v '^$' || true
)
if [ -n "$bypass_matches" ]; then
  echo "ERROR: insecure-mode escape hatch read without a production gate:" >&2
  echo "$bypass_matches" >&2
  echo "" >&2
  echo "  Use fraiseql_guard::deployment::insecure_bypass(VAR)." >&2
  echo "  A bypass honoured in production is not a bypass, it is a vulnerability." >&2
  found=1
fi

# ---------------------------------------------------------------------------
# 3b. The same, for hatches named by a constant rather than a literal.
#
# The check above matches `env::var("FRAISEQL_X_ALLOW_INSECURE")`. Both hatches
# that shipped ungated (#882) spelled it `env_opt_in(VAULT_ALLOW_INSECURE_ENV)`
# — a named constant — so the literal scan never saw them. #882's own repro is
# the rule encoded here: a file that consults a hatch and mentions no production
# check anywhere is the defect. `insecure_bypass` satisfies it (it decides and
# logs); so does an explicit `is_production` in the same file, which is how the
# observers guard composes its per-dispatch logging on top of the same policy.
# ---------------------------------------------------------------------------
hatch_files=$(
  scan_source_lines \
    | grep -E 'env_opt_in\(\s*[A-Za-z_:]*(ALLOW_INSECURE|ALLOW_PLAINTEXT|SKIP_VERIFY|DISABLE_TLS)[A-Z_]*\s*[,)]' \
    | grep -v "^${GUARD_CRATE}" \
    | drop_exempt \
    | grep -v '^$' \
    | cut -d: -f1 | sort -u || true
)
# Comments are stripped before looking for the production check: a doc comment
# that merely *names* `insecure_bypass` is prose, not a gate. (The async-trait
# ratchet was fooled exactly this way.)
#
# `grep -E … >/dev/null` rather than `grep -qE`: under `set -o pipefail`, a `-q`
# grep exits at the FIRST match, the upstream awk is then killed by SIGPIPE (141),
# and the pipeline reports failure even though the match succeeded. That made this
# check report a *correctly* gated file as ungated whenever the file was long
# enough for awk still to be writing — a false positive that grows with file size.
ungated_hatch_files=""
for file in $hatch_files; do
  if ! awk '
        /#\[cfg\(test\)\]/ { exit }
        /^[[:space:]]*(\/\/|\*)/ { next }
        { print }
      ' "$file" | grep -E 'insecure_bypass|is_production' >/dev/null; then
    ungated_hatch_files="${ungated_hatch_files}${file}"$'\n'
  fi
done
ungated_hatch_files=$(printf '%s' "$ungated_hatch_files" | grep -v '^$' || true)
if [ -n "$ungated_hatch_files" ]; then
  echo "ERROR: escape hatch consulted with no production check in the file:" >&2
  echo "$ungated_hatch_files" >&2
  echo "" >&2
  echo "  Use fraiseql_guard::deployment::insecure_bypass(VAR), which applies the" >&2
  echo "  policy AND reports the decision. #882 shipped twice because the variable" >&2
  echo "  was read through a named constant and nothing near it asked about prod." >&2
  found=1
fi

# ── Flight upload guard (#953, #1028) ────────────────────────────────────────
# Same failure mode, different capability. A Flight upload names its own target
# table and inserts rows that never pass the mutation pipeline — no SecurityContext,
# no cache invalidation, no change-log outbox row. #953 gated that behind an
# operator allow-list, in `do_exchange`. `do_put` reached the identical capability
# by a different RPC and nobody noticed for a release: two doors, one guarded.
#
# So: exactly one definition of `authorize_upload`, and every Flight handler that
# builds an INSERT must call it.
UPLOAD_GUARD='crates/fraiseql-arrow/src/flight_server/handlers/upload_guard.rs'

defs=$(grep -rln 'fn authorize_upload' crates/ --include='*.rs' | grep -v "^${UPLOAD_GUARD}$" || true)
if [ -n "$defs" ]; then
  echo "ERROR: authorize_upload is defined outside the shared guard module:" >&2
  echo "$defs" >&2
  echo "" >&2
  echo "  There must be exactly one. #1028 was two copies of one decision, and only" >&2
  echo "  one of them existed." >&2
  found=1
fi

# Any handler that builds an INSERT is a write path and must be gated.
writers=$(grep -rln 'build_insert_query' crates/fraiseql-arrow/src/flight_server/handlers/ --include='*.rs' || true)
ungated_writers=""
for file in $writers; do
  # Match the CALL, not the mention: a `use super::upload_guard::authorize_upload;`
  # left behind after the call was deleted satisfied a bare grep, and this check went
  # green through its own red-capability proof. Imports and comments are stripped first.
  # `>/dev/null` rather than `-q` for the same pipefail/SIGPIPE reason as above.
  if ! grep -vE '^[[:space:]]*(//|\*|use )' "$file" \
    | grep -E 'authorize_upload[[:space:]]*\(' >/dev/null; then
    ungated_writers="${ungated_writers}${file}"$'\n'
  fi
done
ungated_writers=$(printf '%s' "$ungated_writers" | grep -v '^$' || true)
if [ -n "$ungated_writers" ]; then
  echo "ERROR: Flight handler builds an INSERT without calling authorize_upload:" >&2
  echo "$ungated_writers" >&2
  echo "" >&2
  echo "  Call handlers::upload_guard::authorize_upload before reading any batch." >&2
  echo "  This is exactly how do_put shipped ungated beside a gated do_exchange (#1028)." >&2
  found=1
fi

if [ "$found" -ne 0 ]; then
  exit 1
fi

echo "OK: one address guard, one production detector, no ungated escape hatches, one Flight upload guard."
