#!/usr/bin/env bash
# check-principal-producers.sh — pin the set of production sites that turn a credential
# into a principal, so a new transport cannot dispatch one it never resolved (#1336).
#
# WHY THIS GATE EXISTS
# --------------------
# `[identity.enrichment]` documents "when enrichment is enabled, *every* authenticated
# request resolves and fail-closes". For three releases that was true of `/graphql`
# alone. REST, MCP and gRPC each built a `SecurityContext` and dispatched it without
# resolving, so an enriched read failed for 100% of callers over those transports while
# an unknown subject was served the rows `/graphql` answers 403 to.
#
# Nothing caught it, and nothing could have: a transport that skips the resolve answers
# every request that reads no enriched field exactly as a transport that runs it does.
# The defect is invisible from the outside and invisible to the transport's own tests.
# What makes it findable is the shape — a principal produced somewhere that does not
# also resolve — and that is what this gate matches.
#
# It was not a one-off. #858 is the same file list one rule earlier: the shared context
# builder was introduced because MCP called `SecurityContext::from_user` directly and
# lost `tenant_id` and every JWT claim — and gRPC was still calling it directly when
# #1336 was written, three releases later. Both defects were "a transport produced its
# own principal". This gate is about that sentence, not about either rule.
#
# The engine's backstop (`enforce_enrichment_resolved`) refuses an unresolved principal
# at execution, so a missed transport fails closed rather than serving. This gate is the
# other half: it turns that runtime refusal into a build-time one, and it reaches the
# gRPC read arms, which never enter the engine at all (#1348).
#
# Mirrors tools/check-mutation-dispatch-sites.sh (#1327), including its staleness check.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# A credential becoming a principal. `from_user` is the core constructor, the server's
# `build_security_context` wraps it and is what a transport should call, and
# `service_account` mints one from a static secret — all three take something a caller
# presented and return an identity to dispatch with.
#
# `system_job` is deliberately absent: it mints the server acting as *itself*, from no
# credential at all, and marks its own principal Exempt at the construction site. Adding
# it would force an allowlist entry for every scheduled source, which is how an
# allowlist stops being read.
#
# `OptionalSecurityContext` is in the list because the first version of this gate was
# NOT, and the proof run showed why: REST does not *construct* a principal, it takes one
# from the shared axum extractor — so deleting REST's resolve call left the gate green.
# That is the exact defect #1336 fixed. Obtaining a principal counts as producing one.
# Matched as a bare word rather than a call: a handler takes it by type
# (`ctx: OptionalSecurityContext`), destructures it, or borrows it, and requiring a
# following `(` saw only the destructuring form.
PRODUCER='(SecurityContext::(from_user|service_account)|build_security_context)\s*\(|OptionalSecurityContext'

# The one seam that resolves a produced principal and fail-closes. `enrich_identity` is
# the GraphQL handler's thin stage around it, listed because that transport is split
# across `handler.rs` and `handler/stages.rs`; the non-vacuity check at the bottom
# asserts the stage really does call the seam, so this second name cannot become a
# second implementation.
RESOLVER='(resolve_request_identity|enrich_identity)\s*\('

# Producers that legitimately do not resolve, each for a stated structural reason.
# This is not a severity ladder — an entry here is a claim that the resolve happens
# somewhere else on the same request, and it is wrong the moment that stops being true.
#
#   extractors.rs         the shared builder and the extractor itself. Its consumers
#                         resolve: REST inside `RestSecurityContext`, GraphQL in the
#                         handler's stage.
#   api_key/mod.rs        mints a principal the GraphQL handler's `authenticate` stage
#   service_account.rs    returns — and that stage resolves before dispatch.
#   admin_sql.rs          the operator SQL console builds an impersonation context whose
#                         whole subject is the operator's chosen identity (#962).
#   introspection.rs      serves the schema document; it executes no user operation and
#                         reads no row, so there is no identity to scope by.
#   tenant_admin.rs       the tenant admin API reads the principal to *name* the actor in
#   observers/handlers.rs an audit record. Admin-token gated, no user data operation.
#
# `security_context.rs` is NOT here, and the staleness check below is why: it only
# *defines* these constructors, so listing it would have been an allowlist entry that
# excused a file with nothing to excuse — and would have hidden a real producer added
# there later.
DEFERRED='crates/fraiseql-server/src/extractors.rs|crates/fraiseql-server/src/api_key/mod.rs|crates/fraiseql-server/src/service_account.rs|crates/fraiseql-server/src/routes/api/admin_sql.rs|crates/fraiseql-server/src/routes/introspection.rs|crates/fraiseql-server/src/routes/api/tenant_admin.rs|crates/fraiseql-server/src/observers/handlers.rs'

# Producers that do NOT resolve and are tracked as defects. Each needs an issue.
# The Flight transport lives in a crate that cannot reach the resolver (#1349), which is
# why it is a gap rather than an oversight — covering it is a seam change, not a call
# added to a handler. It is not silent: both Flight read paths enter the engine, so
# `enforce_enrichment_resolved` refuses their unmarked principals rather than serving
# them unenriched.
KNOWN='crates/fraiseql-arrow/src/flight_server/handlers/do_get.rs|crates/fraiseql-arrow/src/flight_server/handlers/do_exchange.rs'

# Production code only: a test mints principals freely, and must. Excluding test FILES
# is not enough — `fraiseql-arrow`'s Flight service carries three `#[cfg(test)]` modules
# inline, and counting those as transports made this gate's first run a false positive.
# So inline test modules are stripped too, by brace depth: the same load-bearing `awk`
# pass check-route-syntax.sh needs for multi-line `.route(...)` calls.
strip_inline_tests() {
  awk '
    /^[[:space:]]*#\[cfg\(test\)\]/            { pending = 1; next }
    pending && /^[[:space:]]*(#\[|$)/            { next }
    pending && /mod[[:space:]]+[A-Za-z0-9_]+[[:space:]]*\{/ {
      pending = 0; skip = 1; depth = 0
      depth += gsub(/\{/, "{") - gsub(/\}/, "}")
      if (depth <= 0) skip = 0
      next
    }
    pending                                      { pending = 0 }
    skip {
      depth += gsub(/\{/, "{") - gsub(/\}/, "}")
      if (depth <= 0) skip = 0
      next
    }
    # Comment lines produce nothing. A gate that counted prose would go red on a doc
    # comment naming the function it pins — how lint-async-trait once did.
    /^[[:space:]]*(\/\/|\/\/\/|\/\/!)/       { next }
    { print }
  ' "$1"
}

producer_files=$(
  grep -rlE "$PRODUCER" crates/*/src --include='*.rs' \
    | grep -vE '/(tests|[a-z_]+_tests)\.rs$' \
    | grep -vE '^[^:]*/tests/' \
    | grep -vE '/test_support\.rs$' \
    | sort -u
)

violations=''
for file in $producer_files; do
  # NOT `| grep -q`: under `pipefail`, grep -q closes the pipe on its first match and
  # awk dies of SIGPIPE, so the pipeline reports failure *because it matched*. That
  # inversion made this gate's main loop skip every file — vacuous, and green.
  hits=$(strip_inline_tests "$file" | grep -cE "$PRODUCER" || true)
  if [ "${hits:-0}" -eq 0 ]; then
    continue
  fi
  if echo "$file" | grep -qE "^($DEFERRED|$KNOWN)$"; then
    continue
  fi
  if grep -qE "$RESOLVER" "$file"; then
    continue
  fi
  violations="${violations}${file}"$'\n'
done

if [ -n "$violations" ]; then
  echo "ERROR: a transport produces a principal and never resolves its identity (#1336):"
  echo "$violations"
  echo "When [identity.enrichment] is enabled, EVERY authenticated request resolves and"
  echo "fail-closes. A transport that skips it serves an unknown subject the rows"
  echo "/graphql refuses, and fails every enriched read for every caller — and it looks"
  echo "identical to a working transport on any request that reads no enriched field."
  echo
  echo "Call the seam after building the context:"
  echo "  crate::identity::resolve_request_identity(resolver, ctx.as_mut()).await"
  echo "  Denied => 403 / PERMISSION_DENIED, Unavailable => 503 / UNAVAILABLE,"
  echo "  using EnrichmentOutcome::DENIED_MESSAGE and ::UNAVAILABLE_MESSAGE so the"
  echo "  outward body cannot differ by transport."
  echo
  echo "If the site genuinely cannot reach a resolver, add it to KNOWN in"
  echo "tools/check-principal-producers.sh with the issue that tracks the gap."
  exit 1
fi

# The allowlists are only safe while they are exact. A DEFERRED or KNOWN entry that
# stopped producing must not stay listed: it would excuse a file that no longer needs it
# and hide a NEW unresolved producer added to that same file.
for entry in $(echo "${DEFERRED}|${KNOWN}" | tr '|' ' '); do
  if [ ! -f "$entry" ]; then
    echo "ERROR: allowlist entry $entry does not exist — remove it from this gate."
    exit 1
  fi
  hits=$(strip_inline_tests "$entry" | grep -cE "$PRODUCER" || true)
  if [ "${hits:-0}" -eq 0 ]; then
    echo "ERROR: allowlist entry $entry no longer produces a principal."
    echo "Remove it so the file is gated again."
    exit 1
  fi
done

# A KNOWN entry that started resolving is a fixed gap, and leaving it listed would let
# the next unresolved producer into the same file unnoticed.
for entry in $(echo "${KNOWN}" | tr '|' ' '); do
  if grep -qE "$RESOLVER" "$entry"; then
    echo "ERROR: KNOWN entry $entry now resolves its principal."
    echo "The gap it tracks is closed: remove it from KNOWN so the file is gated again."
    exit 1
  fi
done

# Non-vacuity: if the seam itself vanished, every file would "fail to resolve" and the
# loop above would have reported it — but a RENAMED seam would silently match nothing
# here while the DEFERRED list kept the gate quiet. Check the seam exists.
if ! grep -rqE "fn resolve_request_identity" crates/fraiseql-server/src; then
  echo "ERROR: the enrichment seam resolve_request_identity no longer exists."
  echo "Either it was renamed (update RESOLVER) or this gate is now vacuous."
  exit 1
fi

# `enrich_identity` is accepted above as a name for "resolves", which is only true while
# it is a wrapper. If it stopped calling the seam it would be a second implementation of
# the rule, and this gate would be blessing the drift it exists to prevent.
stage='crates/fraiseql-server/src/routes/graphql/handler/stages.rs'
if ! grep -qE "resolve_request_identity\s*\(" "$stage"; then
  echo "ERROR: $stage no longer calls resolve_request_identity."
  echo "RESOLVER accepts enrich_identity only because that stage is a wrapper around the"
  echo "seam. A second implementation of the rule is what #1336 was."
  exit 1
fi

echo "OK: every principal producer resolves its identity (known gaps, #1349: $KNOWN)"
