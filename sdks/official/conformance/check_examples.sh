#!/usr/bin/env bash
#
# Every schema an SDK example produces must compile.
#
# The conformance suite next door gates each SDK's *authoring API*. This gates its
# *examples* — the code a developer actually copies. Those are a separate failure surface:
# an example can use a real API and still emit something the compiler rejects, and three of
# them did, including one whose committed output was checked into the repository.
#
# Usage:
#   sdks/official/conformance/check_examples.sh target/debug/fraiseql-cli
#
# Coverage is opt-out. Every SDK with an examples directory is listed below, either as a
# runner or as an explicit exclusion with a reason and an issue number. A new SDK with
# examples and no entry fails this script rather than being silently uncovered.

set -euo pipefail

CLI="${1:?usage: check_examples.sh <path-to-fraiseql-cli>}"
CLI="$(cd "$(dirname "$CLI")" && pwd)/$(basename "$CLI")"
SDK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

failures=0
compiled=0
emitted=()

# Compile one emitted schema, or record a failure.
#
# A project declares in `fraiseql.toml` the roles that grant the scopes its schema uses:
# without one, no server loads a scoped schema, and the compiler — which loads what it
# writes — refuses it. Likewise a schema declaring observers compiles only where
# `[observers]` is enabled. Each schema compiles against a scratch `fraiseql.toml` that
# `project_toml.py` derives, named with `--config` (#1387: discovery reads the schema's
# own tree, never the working directory).
compile_schema() {
  local label="$1" path="$2" project="$WORK/project"
  rm -rf "$project" && mkdir -p "$project"
  python3 "$SDK_ROOT/conformance/project_toml.py" "$path" >"$project/fraiseql.toml"
  if "$CLI" compile "$path" --config "$project/fraiseql.toml" -o "$WORK/out.json" >"$WORK/compile.log" 2>&1; then
    printf 'ok    %s\n' "$label"
    compiled=$((compiled + 1))
  else
    printf 'FAIL  %s\n' "$label"
    sed 's/^/        /' "$WORK/compile.log" | head -20
    failures=$((failures + 1))
  fi
}

# Run an example from `dir`, then compile every schema it wrote there. The schemas are
# left in `emitted`.
#
# An example that exits non-zero fails here even if it wrote nothing: "the program does not
# run" is the defect two Python examples had for months, and it is invisible to a check
# that only looks at emitted files.
run_and_compile() {
  local label="$1" dir="$2"
  shift 2
  if ! (cd "$dir" && "$@" >"$WORK/run.log" 2>&1); then
    printf 'FAIL  %s (did not run)\n' "$label"
    sed 's/^/        /' "$WORK/run.log" | tail -10
    failures=$((failures + 1))
    return
  fi
  local found=0 schema
  emitted=()
  while IFS= read -r schema; do
    found=1
    emitted+=("$schema")
    compile_schema "$label -> $(basename "$schema")" "$schema"
  done < <(find "$dir" -maxdepth 1 -name '*.json' -newer "$WORK" 2>/dev/null)
  if [ "$found" -eq 0 ]; then
    # Legitimate: an example may demonstrate an authoring API without exporting.
    printf 'ok    %s (ran; emitted no schema)\n' "$label"
  fi
}

# Run an example that declares observers, and fail unless a schema it wrote declares them.
#
# `run_and_compile` accepts an example that writes nothing. An observer example that
# stopped exporting, or exported without its observers, would then read as passing while
# no observer action reached the compiler.
run_and_compile_observers() {
  local label="$1" before="$failures" schema declared=0
  run_and_compile "$@"
  [ "$failures" -eq "$before" ] || return 0
  for schema in ${emitted[@]+"${emitted[@]}"}; do
    if python3 -c 'import json, sys; sys.exit(0 if json.load(open(sys.argv[1])).get("observers") else 1)' "$schema"; then
      declared=1
    fi
  done
  if [ "$declared" -eq 0 ]; then
    printf 'FAIL  %s (exported no schema declaring observers)\n' "$label"
    failures=$((failures + 1))
  fi
}

echo "== Go =="
for dir in "$SDK_ROOT"/fraiseql-go/examples/*/; do
  [ -f "$dir/main.go" ] || continue
  rm -f "$dir"/*.json
  runner=run_and_compile
  [ "$(basename "$dir")" = ecommerce ] && runner=run_and_compile_observers
  "$runner" "go/$(basename "$dir")" "$dir" go run .
done

echo
echo "== Python =="
for example in "$SDK_ROOT"/fraiseql-python/examples/*.py; do
  [ -f "$example" ] || continue
  workdir="$WORK/py-$(basename "$example" .py)"
  mkdir -p "$workdir"
  run_and_compile \
    "python/$(basename "$example")" "$workdir" \
    uv run --project "$SDK_ROOT/fraiseql-python" --quiet python "$example"
done

echo
echo "== TypeScript =="
# `--tsconfig` is load-bearing, not tidiness: run from a directory with no tsconfig on the
# path and esbuild falls back to TC39 decorators, under which `@Source`/`@Observer` used to
# serialize the decorator's context object as the member's name (#925). The examples run in
# a scratch directory so the schemas they write cannot be confused with the package's own
# package.json / tsconfig.json.
for example in "$SDK_ROOT"/fraiseql-typescript/examples/*.ts; do
  [ -f "$example" ] || continue
  workdir="$WORK/ts-$(basename "$example" .ts)"
  mkdir -p "$workdir"
  runner=run_and_compile
  [ "$(basename "$example")" = ecommerce_with_observers.ts ] && runner=run_and_compile_observers
  "$runner" \
    "typescript/$(basename "$example")" "$workdir" \
    npx --yes tsx --tsconfig "$SDK_ROOT/fraiseql-typescript/tsconfig.json" "$example"
done

echo
echo "== Java =="
# The observer example, run through the `ExamplesExport` test: the Maven build exposes no
# other entry point. `-f` keeps the working directory the scratch one the schema lands in.
workdir="$WORK/java-observers"
mkdir -p "$workdir"
FRAISEQL_EXAMPLES_OUT="$workdir" run_and_compile_observers \
  "java/EcommerceWithObserversSchema" "$workdir" \
  mvn -q -B -f "$SDK_ROOT/fraiseql-java/pom.xml" test -Dtest=ExamplesExport -DfailIfNoTests=false

echo
echo "== PHP =="
for example in "$SDK_ROOT"/fraiseql-php/examples/*.php; do
  [ -f "$example" ] || continue
  workdir="$WORK/php-$(basename "$example" .php)"
  mkdir -p "$workdir"
  run_and_compile "php/$(basename "$example")" "$workdir" php "$example"
done

echo
echo "== Repository examples authored with an SDK =="
# A repository example whose committed export is written by an SDK script: the script must
# run, reproduce the committed file byte for byte (a stale export would let the example's e2e
# test exercise a schema its author can no longer produce), and the example's `fraiseql.toml`,
# which includes it, must compile.
REPO_ROOT="$(cd "$SDK_ROOT/../.." && pwd)"
for example in "$REPO_ROOT"/examples/localized-catalog; do
  label="examples/$(basename "$example")"
  workdir="$WORK/repo-$(basename "$example")"
  mkdir -p "$workdir"
  if ! (cd "$workdir" && uv run --project "$SDK_ROOT/fraiseql-python" --quiet python "$example/schema.py" >"$WORK/run.log" 2>&1); then
    printf 'FAIL  %s (did not run)\n' "$label"
    sed 's/^/        /' "$WORK/run.log" | tail -10
    failures=$((failures + 1))
    continue
  fi
  if ! diff -u "$example/catalog.json" "$workdir/catalog.json" >"$WORK/diff.log"; then
    printf 'FAIL  %s (committed catalog.json is stale; re-run schema.py)\n' "$label"
    sed 's/^/        /' "$WORK/diff.log" | head -20
    failures=$((failures + 1))
    continue
  fi
  if (cd "$example" && "$CLI" compile fraiseql.toml -o "$WORK/out.json") >"$WORK/compile.log" 2>&1; then
    printf 'ok    %s\n' "$label"
    compiled=$((compiled + 1))
  else
    printf 'FAIL  %s\n' "$label"
    sed 's/^/        /' "$WORK/compile.log" | head -20
    failures=$((failures + 1))
  fi
done

echo
echo "== Not covered =="
# Each entry is a deliberate, reviewed exclusion. Removing one is how coverage grows;
# adding one requires an issue.
cat <<'EXCLUSIONS'
skip  typescript/examples/sources/  — poll_orders.connector.ts is a Deno function that runs
                                      inside FraiseQL's runtime against host ops, not
                                      authoring code. There is no `node` that can execute it.
EXCLUSIONS

echo
if [ "$failures" -gt 0 ]; then
  echo "$failures example check(s) failed ($compiled schema(s) compiled)"
  exit 1
fi
echo "all example checks passed ($compiled schema(s) compiled)"
