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

# Compile one emitted schema, or record a failure.
compile_schema() {
  local label="$1" path="$2"
  if "$CLI" compile "$path" -o "$WORK/out.json" >"$WORK/compile.log" 2>&1; then
    printf 'ok    %s\n' "$label"
    compiled=$((compiled + 1))
  else
    printf 'FAIL  %s\n' "$label"
    sed 's/^/        /' "$WORK/compile.log" | head -20
    failures=$((failures + 1))
  fi
}

# Run an example, then compile every schema it wrote into its own directory.
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
  while IFS= read -r schema; do
    found=1
    compile_schema "$label -> $(basename "$schema")" "$schema"
  done < <(find "$dir" -maxdepth 1 -name '*.json' -newer "$WORK" 2>/dev/null)
  if [ "$found" -eq 0 ]; then
    # Legitimate: an example may demonstrate an authoring API without exporting — the
    # observer examples do, because `fraiseql compile` refuses declared observers.
    printf 'ok    %s (ran; emitted no schema)\n' "$label"
  fi
}

echo "== Go =="
for dir in "$SDK_ROOT"/fraiseql-go/examples/*/; do
  [ -f "$dir/main.go" ] || continue
  rm -f "$dir"/*.json
  run_and_compile "go/$(basename "$dir")" "$dir" go run .
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
echo "== Not covered =="
# Each entry is a deliberate, reviewed exclusion. Removing one is how coverage grows;
# adding one requires an issue.
cat <<'EXCLUSIONS'
skip  typescript/examples  — all 11 call `fraiseql.type()` / `fraiseql.query()`, which the
                             package has never exported (the decorators are `Type`/`Query`).
                             They do not run at all, and predate this suite. See #925.
skip  php/examples         — BasicSchema.php and EcommerceSchema.php are class definitions
                             with no entry point; there is nothing to execute. See #925.
EXCLUSIONS

echo
if [ "$failures" -gt 0 ]; then
  echo "$failures example check(s) failed ($compiled schema(s) compiled)"
  exit 1
fi
echo "all example checks passed ($compiled schema(s) compiled)"
