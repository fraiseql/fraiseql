#!/usr/bin/env bash
# check-route-syntax.sh — fail if any axum `.route(...)` literal still uses 0.7-style `:param` captures.
#
# Background (issue #316): the axum 0.7 → 0.8 bump replaced path captures `:param` with `{param}`.
# axum 0.8's `Router::route(...)` hard-panics at build time on the old syntax, so a missed
# migration ships as a server-startup crash, not a compile error.
#
# CRITICAL: the multi-line `awk` branch below is load-bearing. The literal that shipped in
# fraiseql-server v2.3.0 — `"/checkpoint/:listener_id"` at observers/routes.rs:128 — sat on
# its own line inside a `.route(\n  "path",\n  handler\n)` call. The single-line `grep` form
# matched zero lines for that bug. Do NOT delete or "simplify" the awk pass.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

found=0

# Single-line form: `.route("/foo/:bar", handler)`
if matches=$(grep -rn -E '\.route\(\s*"[^"]*/:[a-zA-Z_]' crates/ examples/ --include='*.rs' 2>/dev/null); then
  if [ -n "$matches" ]; then
    echo "ERROR: axum 0.7-style :param captures found (single-line .route()):" >&2
    echo "$matches" >&2
    found=1
  fi
fi

# Multi-line form: `.route(`-on-its-own-line followed by a quoted path containing `/:ident`
# on a subsequent line, before any other top-level argument terminates the call.
multi_matches=$(
  find crates examples -name '*.rs' -not -path '*/target/*' -print0 \
    | xargs -0 awk '
        # Match `.route(` at end of line (possibly trailing whitespace).
        /\.route\(\s*$/                       { in_route = 1; next }
        # Inside an open `.route(` call, look for a quoted axum 0.7 capture literal.
        in_route && /"[^"]*\/:[a-zA-Z_]/      { printf("%s:%d:%s\n", FILENAME, FNR, $0); hits++; }
        # End of arg list when we hit a closing paren at start of a line.
        in_route && /^\s*\)/                  { in_route = 0 }
      END { exit (hits ? 1 : 0) }
    ' 2>/dev/null
) && multi_exit=0 || multi_exit=$?

if [ "$multi_exit" -ne 0 ]; then
  echo "ERROR: axum 0.7-style :param captures found (multi-line .route()):" >&2
  echo "$multi_matches" >&2
  found=1
fi

if [ "$found" -ne 0 ]; then
  cat >&2 <<'EOF'

Migrate to axum 0.8 capture syntax:
  .route("/users/:id", handler)        →  .route("/users/{id}", handler)
  .route("/a/:x/b/:y", handler)        →  .route("/a/{x}/b/{y}", handler)

See issue #316 for the bug class this gate prevents.
EOF
  exit 1
fi

echo "OK: no axum 0.7-style :param captures in crates/ or examples/."
