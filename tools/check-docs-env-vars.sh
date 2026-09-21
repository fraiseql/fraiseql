#!/usr/bin/env bash
# check-docs-env-vars.sh — fail if docs or example markdown name a FRAISEQL_* env var
# that nothing reads.
#
# Background (issue #838): the operator runbooks prescribed `export FRAISEQL_QUERY_CACHE_SIZE=…`
# style mitigations for variables with zero readers in the workspace. An on-call engineer
# following a runbook during a live incident sets the variable, sees no effect, believes the
# mitigation is applied, and moves on to a wrong hypothesis. A documented variable must have a
# reader, or the documentation is an instruction to do nothing.
#
# Mechanics: every FRAISEQL_[A-Z0-9_]+ token in markdown under docs/, examples/ and the root
# README must appear (as a substring, so `FRAISEQL_NATS_*` families match their literal prefix
# reads) in at least one reader file: Rust source under crates/, or a non-markdown file under
# examples/ (example sidecars legitimately read their own variables). Deliberate exceptions —
# operator-chosen names shown in `*_env = "…"` config examples — live in
# tools/docs-env-vars.allow with a reason each.
#
# One more case (2026-09-21): a dated audit record under docs/security/audits/ quotes the tree
# as it was, so it may name a variable that has since been removed — provided CHANGELOG.md
# records the removal. The record is not an instruction, and the status ledger beside it says
# what became of the finding. The exception is by location AND by changelog: a runbook naming
# a removed variable is still an orphan, and an audit record naming a variable the changelog
# never removed is still an orphan.
#
# Overrides, for testing:  DOCS_ENV_VARS_ROOT=<dir>
set -euo pipefail
if [ -n "${DOCS_ENV_VARS_ROOT:-}" ]; then
  cd "$DOCS_ENV_VARS_ROOT"
else
  cd "$(git rev-parse --show-toplevel)"
fi

allowlist=tools/docs-env-vars.allow

doc_tokens=$(
  { grep -rhoE 'FRAISEQL_[A-Z0-9_]*[A-Z0-9]' docs/ README.md 2>/dev/null || true
    grep -rhoE 'FRAISEQL_[A-Z0-9_]*[A-Z0-9]' examples/ --include='*.md' 2>/dev/null || true
  } | sort -u
)

orphans=""
for token in $doc_tokens; do
  # Allowlisted (first whitespace-separated field of a non-comment line)?
  if [ -f "$allowlist" ] && grep -vE '^\s*(#|$)' "$allowlist" | awk '{print $1}' | grep -qxF "$token"; then
    continue
  fi
  # A reader anywhere in Rust source?
  if grep -rqF "$token" crates/ --include='*.rs' 2>/dev/null; then
    continue
  fi
  # A reader in example code (non-markdown)?
  if grep -rqF "$token" examples/ --exclude='*.md' 2>/dev/null; then
    continue
  fi
  files=$(grep -rlF "$token" docs/ README.md examples/ 2>/dev/null | grep -v "$allowlist" | sort -u | tr '\n' ' ')
  # Quoted only by dated audit records, and recorded as removed in the changelog?
  if [ -n "$files" ] \
     && ! printf '%s\n' "$files" | tr ' ' '\n' | grep -v '^$' | grep -qvE '^docs/security/audits/' \
     && grep -F "$token" CHANGELOG.md 2>/dev/null | grep -qiE '\bremoved\b'; then
    continue
  fi
  orphans="${orphans}  ${token}  →  ${files}\n"
done

if [ -n "$orphans" ]; then
  {
    echo "ERROR: documented FRAISEQL_* environment variables with no reader in the workspace:"
    printf '%b' "$orphans"
    cat <<'EOF'

Every FRAISEQL_* variable named in docs/, examples/*.md or README.md must be read by code
(crates/**/*.rs, or the example's own non-markdown sources). Fix the doc to name the real
knob, implement the variable, or — only for an operator-chosen name in a `*_env = "…"`
example — add it to tools/docs-env-vars.allow with a reason.

See issue #838 for the incident class this gate prevents.
EOF
  } >&2
  exit 1
fi

echo "OK: every FRAISEQL_* variable named in docs/, README.md and examples/*.md has a reader."
