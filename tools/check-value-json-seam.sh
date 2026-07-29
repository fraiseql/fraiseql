#!/usr/bin/env bash
# check-value-json-seam.sh — fail if a GraphQL argument value is serialized, unescaped or
# variable-detected by hand instead of going through `fraiseql_core::graphql::value_json`.
#
# Background (issue #719): the parser built `value_json` with
# `format!("\"{}\"", s.replace('"', "\\\""))`, escaping only the double quote. A Windows
# path, a newline or a control character therefore produced *invalid JSON*, and the reader
# dropped it with `.ok()?`. A dropped `where:` argument does not narrow a result set — it
# widens it, which is why a serialization bug was filed as a security issue.
#
# The same seam carried variables in-band as the string `"$name"`, so a literal `"$100"`
# was indistinguishable from a reference to a variable called `100` and resolved to `null`.
#
# Both are structural, not spelling: every consumer that re-derived the encoding was a
# place for the two to drift. There is now one module, and this gate keeps it that way.
#
# If you are reading a stored argument: call `value_json::decode`, then
# `value_json::variable_name` / `value_json::resolve_variables`.
# If you are writing one: call `value_json::encode`.
# If you are rendering one back to GraphQL source: call `value_json::to_graphql`.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

found=0

# The one module allowed to define the encoding.
SEAM='crates/fraiseql-core/src/graphql/value_json'

# Escapers for formats that are NOT JSON, where the replacement set is defined by
# that format's own spec and `serde_json` would be wrong. Each needs a reason.
#   metrics.rs            — Prometheus exposition format label values escape exactly
#                           `\`, `"` and newline (Prometheus docs, "Text format").
#   audit_export_syslog.rs — RFC 5424 §6.3.3 SD-PARAM values escape `"`, `\` and `]`.
NON_JSON_ESCAPERS='crates/fraiseql-server/src/routes/metrics.rs|crates/fraiseql-core/src/security/audit_export_syslog.rs'

# --- Rule 1: no hand-rolled JSON string escaping -----------------------------------
# The `\` + `"` replacement chain that #719 reports, in any order.
echo "→ hand-rolled JSON string escaping"
if hits=$(grep -rn --include='*.rs' -F 'replace('"'"'"'"'"', "\\\""' crates/ \
    | grep -v "^${SEAM}" \
    | grep -Ev "^(${NON_JSON_ESCAPERS})" || true); [ -n "$hits" ]; then
    echo "✗ hand-rolled JSON escaping — use serde_json (#719):"
    echo "$hits"
    found=1
fi

# --- Rule 2: no in-band `$`-prefix variable detection -------------------------------
# A variable reference is the tagged object `{"$var": "name"}`, not a string that happens
# to start with `$`. Detecting it by prefix is what collided with the literal `"$100"`.
echo "→ in-band \$-prefix variable detection"
if hits=$(grep -rn --include='*.rs' -E "(strip_prefix|starts_with)\('\\\$'\)" crates/ \
    | grep -v "^${SEAM}" || true); [ -n "$hits" ]; then
    echo "✗ variable references are detected by prefix — use value_json::variable_name (#719):"
    echo "$hits"
    found=1
fi

# --- Rule 3: no hand-rolled unquoting of a stored argument --------------------------
# Peeling the outer quotes off `value_json` and unescaping only `\"` is the reader-side
# half of the same defect.
echo "→ hand-rolled unquoting of value_json"
if hits=$(grep -rn --include='*.rs' -B2 -A2 -F 'value_json' crates/ \
    | grep -F 'starts_with(' | grep -F "'\"'" \
    | grep -v "^${SEAM}" || true); [ -n "$hits" ]; then
    echo "✗ value_json is unquoted by hand — use value_json::decode (#719):"
    echo "$hits"
    found=1
fi

# --- Rule 4: value_json must not be deserialized with a silent fallback -------------
# `.ok()?` / `unwrap_or_default()` on a `value_json` parse drops the argument. A dropped
# filter widens the result set; the parse must propagate.
echo "→ silent fallback on a value_json parse"
if hits=$(grep -rn --include='*.rs' -E 'from_str.*value_json' crates/ \
    | grep -v "^${SEAM}" || true); [ -n "$hits" ]; then
    echo "✗ value_json is parsed directly — use value_json::decode, which fails loud (#719):"
    echo "$hits"
    found=1
fi

if [ "$found" -ne 0 ]; then
    echo ""
    echo "check-value-json-seam: FAILED — see the header of tools/check-value-json-seam.sh"
    exit 1
fi

echo "✓ check-value-json-seam: the value_json encoding has one owner"
