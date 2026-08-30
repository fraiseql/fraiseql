#!/usr/bin/env bash
# check-sdk-dead-surface.sh — refuse an SDK authoring surface the compiler cannot consume.
#
# WHY THIS GATE EXISTS
# --------------------
# An SDK builder that writes a key no compile path reads is the most reliably recurring
# defect in this repo: #755, #779, #780, #806, #807, #847, #890, #926, #927, #956 are all
# the same shape. The author follows the SDK's own documentation, `fraiseql compile` prints
# success or fails naming a key they never typed, and the declared behaviour never happens.
#
# The seam catches *most* of it structurally — `IntermediateSchema` and `IntermediateQuery`
# deny unknown fields, so a key that reaches `schema.json` fails the compile loudly. What
# the seam cannot catch is an SDK surface that never reaches the wire at all: Java's
# `sqlSourceDispatch` stored its config in a registry field nothing serialized, and Dart's
# `SqlSourceDispatch` annotation was read by nothing, because Dart has no reflection layer
# over its annotations. Both were completely inert, so both compiled, tested and shipped
# green for as long as they existed. Only a grep can see that.
#
# So this gate pins, by name, the surfaces that have been removed for having no consumer.
# Reintroducing one is then a deliberate act: implement the consumer first, then delete
# the entry here in the same change.
#
# ADDING TO THIS LIST
# -------------------
# Anchor the regex where a bare name would collide. `class FraiseQLField` matched C#'s
# unrelated `public static class FraiseQLField` in UpdateField.cs; `^class FraiseQLField`
# matches only a Dart top-level declaration, which is the surface that was removed.
# One entry per removed surface: a regex, and the issue that removed it. The reason text is
# shell-expanded when the array is built, so it must contain no backticks — one there turned
# the explanation into a command substitution and the gate died before checking anything.
# Keep the regex
# specific enough that a prose mention in a CHANGELOG or migration note does not trip it —
# those files are excluded below rather than made unwritable.
#
# Mirrors the established shell-gate pattern (lint-routes, lint-internal-flag, lint-guard-parity).
set -euo pipefail
# Not `cd "$(git rev-parse --show-toplevel)"`: outside a repo that expands to the empty
# string and becomes a silent `cd ""`, leaving the gate to scan whatever directory it
# happened to start in.
cd "$(git rev-parse --show-toplevel 2>/dev/null || dirname "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")")"

# name|regex|issue — the surface, how to spot it, and why it went.
REMOVED=(
    "sql_source_dispatch|sql_source_dispatch|#926 — no compiler consumer; one query per source is the supported pattern"
    "SqlSourceDispatch|SqlSourceDispatch|#926 — Go builders and the Dart annotation, same reason"
    "sqlSourceDispatch|sqlSourceDispatch|#926 — the Java builders, same reason"
    "NewAggregateQueryConfig|NewAggregateQueryConfig|#956 — the compiler refuses aggregate_queries; declare the fact table (which gives you the <name>_aggregate root field) or use [[analytics.queries]]"
    "RegisterAggregateQuery|RegisterAggregateQuery|#956 — the registry half of the same removed surface"
    "FraiseQLType|^class FraiseQLType|#1241 — the Dart @FraiseQLType annotation; Dart has no runtime reflection over annotations and the package ships no build_runner generator, so nothing read it. Author with FraiseQLSchema.type(), which takes the same crud: and cascade: flags"
    "FraiseQLField|^class FraiseQLField|#1241 — the Dart @FraiseQLField annotation, same reason. Its computed: flag is now FieldType(computed: true)"
)

# Only SDK authoring code is in scope. Docs are where the removal is *explained*, so a
# prose mention there is correct, not a violation — and this script names each surface
# itself, which is why it excludes its own path (a gate's own prose tripping its own grep
# has now happened five times in this repo).
#
# Discovered with `find`, not `git ls-files`. Two reasons, both learned the hard way:
# mixing git's `:!` exclude pathspecs with a positive pattern silently matched
# *everything*, so the list came back empty; and the Dagger ShellGates container mounts
# the tree with `.git` ignored and runs `git init` on it, so nothing is tracked there and
# `git ls-files` returns nothing at all. Either way the gate would have reported success
# over a tree it never looked at.
#
# Restricted to authoring-source extensions rather than "everything that is not
# Markdown": a bare file sweep picks up build output and test caches — a stale
# `.pytest_cache/v/cache/nodeids` naming a long-deleted test was enough to fail this gate
# — and those are not authoring surfaces.
readarray -t FILES < <(
    find sdks/official \
        \( -name node_modules -o -name .venv -o -name vendor -o -name target \
           -o -name 'target.bak' -o -name .dart_tool -o -name obj -o -name bin \
           -o -name '.*_cache' -o -name __pycache__ -o -name dist -o -name build \) -prune -o \
        -type f \( -name '*.go' -o -name '*.java' -o -name '*.dart' -o -name '*.py' \
           -o -name '*.ts' -o -name '*.tsx' -o -name '*.js' -o -name '*.rb' \
           -o -name '*.ex' -o -name '*.exs' -o -name '*.php' -o -name '*.fs' \
           -o -name '*.fsi' -o -name '*.cs' -o -name '*.scala' -o -name '*.kt' \
           -o -name '*.rs' \) -print
)
if [ ${#FILES[@]} -eq 0 ]; then
    echo "✗ check-sdk-dead-surface: matched no SDK files at all — the gate is not looking"
    echo "  at anything. Fix the file discovery; a clean result here would be meaningless."
    exit 1
fi

status=0
for entry in "${REMOVED[@]}"; do
    IFS='|' read -r name regex reason <<<"$entry"
    # Comment lines are dropped before matching. A file explaining *why* a surface was
    # removed must be able to name it — `registry.go` does exactly that, and matching it
    # would make the honest comment the violation. (Sixth time a gate has tripped on
    # prose in this repo; stripping comments is the established remedy.)
    hits=""
    if [ ${#FILES[@]} -gt 0 ]; then
        hits="$(grep -rn -- "$regex" "${FILES[@]}" 2>/dev/null |
            grep -vE ':[0-9]+:[[:space:]]*(//|#|\*|/\*)' || true)"
    fi
    if [ -n "$hits" ]; then
        echo "✗ '$name' is back in an SDK authoring surface, but it was removed: $reason"
        echo "$hits" | sed 's/^/    /'
        echo "    If the compiler now consumes it, implement that first and remove this"
        echo "    entry from tools/check-sdk-dead-surface.sh in the same change."
        status=1
    fi
done

if [ "$status" -eq 0 ]; then
    echo "✓ check-sdk-dead-surface: no removed SDK surface has been reintroduced (${#REMOVED[@]} pinned)."
fi
exit "$status"
