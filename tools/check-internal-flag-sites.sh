#!/usr/bin/env bash
# check-internal-flag-sites.sh — pin the set of production files that READ
# `TypeDefinition.internal` (#665).
#
# WHY THIS GATE EXISTS
# --------------------
# `internal` marks a framework-synthesized bookkeeping projection (the change-log /
# checkpoint views `inject_changelog` synthesizes) so cascade entity classification
# excludes it. The flag is DELIBERATELY scoped to exactly two concerns:
#
#   1. cascade entity classification    — cli  `is_queryable_entity`  (cascade_types.rs)
#   2. the runtime cascade-delivery guard — core mutation runner        (mutation/mod.rs)
#
# The hazard of a property-named flag ("internal") is silent scope creep: a later change
# could reuse it to skip a validation, hide a type from introspection, or drop it from
# federation. Each of those is a SEPARATE policy decision that deserves its own review —
# not a free ride on this flag's meaning. A doc-comment alone is not scope control (this
# repo's own `database_validator.rs` comment is the live proof that comments drift), so
# this gate makes widening `internal`'s reach a deliberate act: a NEW read site fails CI
# until someone adds it to ALLOWED below and states why.
#
# Notably, `database_validator.rs` intentionally does NOT read `.internal`: it must keep
# validating that the change-log views exist, because a missing `v_entity_change_log` /
# `v_transport_checkpoint` is #569's failure mode. If this gate ever reports it, someone
# DRY'd the cascade classifier and the view-existence check together and deleted the #569
# signal — revert that, don't add it to ALLOWED.
#
# This gate constrains READS only. WRITE sites (`changelog.rs` sets the flag) and
# struct-literal initializers (`internal: false`) are not scope-widening and are ignored.
#
# Mirrors the established shell-gate pattern (lint-routes, lint-unwrap, lint-async-trait).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Production files permitted to READ `.internal`. Widening this set is a reviewed choice.
ALLOWED='crates/fraiseql-cli/src/schema/converter/cascade_types.rs
crates/fraiseql-core/src/runtime/executor/runners/mutation/mod.rs'

# Field-access READS of `.internal`, in production code only. Exclusions:
#   - test files (paths ending in tests.rs / _tests.rs, or under a tests/ dir);
#   - assignment WRITES (`x.internal = y`), but NOT match guards (`if x.internal =>`);
#   - string-literal false positives (a `host.internal/` URL).
reads=$(grep -rnP '\.internal\b' crates/*/src/ --include='*.rs' \
  | grep -vE '/tests?\.rs:|_tests\.rs:|/tests/' \
  | grep -vP '\.internal\s*=(?![=>])' \
  | grep -vP '\.internal/' \
  || true)

violations=0
while IFS= read -r line; do
  [ -z "$line" ] && continue
  file="${line%%:*}"
  if ! grep -qxF "$file" <<<"$ALLOWED"; then
    if [ "$violations" -eq 0 ]; then
      echo "ERROR: new TypeDefinition.internal READ site(s) outside the allowed set." >&2
      echo >&2
      echo "  \`internal\` (#665) is scoped to TWO concerns on purpose:" >&2
      echo "    1. cascade entity classification (cli is_queryable_entity)" >&2
      echo "    2. the runtime cascade-delivery guard (core mutation runner)" >&2
      echo "  Reusing it to skip validation, hide from introspection, or drop from" >&2
      echo "  federation is a DIFFERENT policy decision that needs its own review — not a" >&2
      echo "  free ride on this flag. If the new read is intended, add its file to ALLOWED" >&2
      echo "  in tools/check-internal-flag-sites.sh and say why." >&2
      echo "  If the file is database_validator.rs, you DRY'd away #569's missing-view" >&2
      echo "  signal — revert instead." >&2
      echo >&2
    fi
    echo "  $line" >&2
    violations=1
  fi
done <<<"$reads"

if [ "$violations" -ne 0 ]; then
  exit 1
fi

echo "OK: TypeDefinition.internal is read only by cascade classification + the runtime cascade guard."
