#!/usr/bin/env bash
#
# The cross-SDK schema parity gate — one definition, run by CI and by `make test-parity`.
#
# There used to be two: `sdk-parity.yml` compared Python against TypeScript and Go
# strictly and against seven more SDKs behind `continue-on-error`, while `make
# test-parity` compared a different five plus the golden fixture and ran in a workflow
# that had been failing for unrelated reasons. Between them they read as full coverage.
# What was actually happening is that the soft comparison had been crashing on PHP's
# field shape — so PHP, Java, Ruby, Dart, C#, F#, Rust and the golden had never been
# compared even once, and the job was green (#952).
#
# Two rules follow from that, and both are enforced here rather than in a workflow file:
#
#   1. A missing toolchain is a FAILURE, not a skip. A skipped SDK reads exactly like a
#      passing one in a log. This is the `--require-all` rule the conformance suite next
#      door already learned. `--allow-missing` exists for local runs and says out loud,
#      in the summary, which SDKs went uncompared.
#   2. Every producer is compared in one invocation, so one producer's bad shape cannot
#      cost the producers after it their comparison.
#
# ── What this gate does NOT cover, and why ───────────────────────────────────────────
#
# Elixir, Dart, C#, F#, Java and Ruby are absent on purpose. Their parity "generators"
# built the expected JSON by hand — `%{"name" => "User", "fields" => [...]}` written out
# as a literal, importing only a JSON library and never their SDK — so they could not
# fail no matter what the SDK did. Three of them "disagreed" with Python only because
# someone had typed `"jwt:sub"` where the nested `{source, claim}` form belongs. They
# were removed rather than corrected: making fiction agree with fiction is not coverage.
#
# Six of eleven, which is exactly the count `sdks/official/conformance/manifest.json`
# already records — that comment is why the conformance suite exists. All six remain
# fully covered by `sdk-conformance.yml`, the stronger gate: it authors through each
# SDK's real API, runs the actual compiler, and asserts sixteen constructs against a
# declared-gap manifest. The five below are the generators that genuinely drive their
# SDK, so their comparison means something.
#
# Usage:
#   sdks/official/tests/run_parity.sh                  # every SDK required
#   sdks/official/tests/run_parity.sh --allow-missing  # local: skip absent toolchains

set -euo pipefail

ALLOW_MISSING=0
[ "${1:-}" = "--allow-missing" ] && ALLOW_MISSING=1

SDK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "$SDK_ROOT/../.." && pwd)"
GOLDEN="$REPO_ROOT/tests/fixtures/golden/parity-schema.json"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

missing=()
generated=()
failures=()

# Generate one SDK's parity schema.
#
#   emit <name> <types-only?> <dir> <required-binary> <command...>
#
# A command that writes to stdout is redirected; one that honours SCHEMA_OUTPUT_FILE
# gets it in the environment. Either way the result must be a non-empty file, because
# "the generator ran and wrote nothing" is a failure that a `$?` check alone misses.
emit() {
  local name="$1" kind="$2" dir="$3" bin="$4"
  shift 4
  local out="$WORK/schema_$name.json"

  # A generator that cannot run is recorded and the sweep continues, so one broken
  # toolchain does not hide the state of the other ten. In strict mode every recorded
  # problem still fails the run at the end.
  local problem=""

  if ! command -v "$bin" >/dev/null 2>&1; then
    problem="'$bin' is not installed"
  elif [ "$name" = "typescript" ] || [ "$name" = "php" ]; then
    (cd "$SDK_ROOT/$dir" && "$@" >"$out" 2>"$WORK/$name.log") || problem="generator exited non-zero"
  else
    (cd "$SDK_ROOT/$dir" && SCHEMA_OUTPUT_FILE="$out" "$@" >"$WORK/$name.log" 2>&1) \
      || problem="generator exited non-zero"
  fi

  if [ -z "$problem" ] && [ ! -s "$out" ]; then
    problem="generator produced no schema"
  fi

  if [ -n "$problem" ]; then
    if [ "$ALLOW_MISSING" -eq 1 ]; then
      missing+=("$name ($problem)")
    else
      echo "FAIL  $name: $problem" >&2
      [ -s "$WORK/$name.log" ] && sed 's/^/        /' "$WORK/$name.log" | tail -15 >&2
      failures+=("$name")
    fi
    return 0
  fi

  generated+=("$kind:$name:$out")
  echo "ok    generated $name"
}

# The reference. Not optional under any flag: there is nothing to compare against.
if ! (cd "$SDK_ROOT/fraiseql-python" && uv run --quiet python tests/generate_parity_schema.py \
        >"$WORK/schema_python.json"); then
  echo "FAIL  python (reference) generator exited non-zero" >&2
  exit 1
fi
[ -s "$WORK/schema_python.json" ] || { echo "FAIL  python (reference) produced no schema" >&2; exit 1; }
echo "ok    generated python (reference)"

emit typescript full fraiseql-typescript npx  npx --yes tsx tests/generate-parity-schema.ts
emit go         full fraiseql-go         go   go test -count=1 -run TestGenerateParitySchema ./fraiseql/
emit php        full fraiseql-php        php  php tests/GenerateParitySchema.php

# The Rust authoring SDK is RBAC-focused and declares no queries or mutations, so it is
# compared on types only. That is a declared scope, not a silent one.
emit rust types-only fraiseql-rust cargo cargo test --test generate_parity_schema

compare=()
types_only=()
for entry in "${generated[@]:-}"; do
  [ -n "$entry" ] || continue
  case "$entry" in
    full:*)       compare+=("${entry##*:}") ;;
    types-only:*) types_only+=("${entry##*:}") ;;
  esac
done

# The golden fixture is a producer too: it is the committed statement of what the shape
# is, and comparing every SDK against Python while never checking Python against the
# golden lets the whole set drift together.
[ -f "$GOLDEN" ] || { echo "FAIL  golden fixture missing at $GOLDEN" >&2; exit 1; }
compare+=("$GOLDEN")

echo
cmd=(python3 "$SDK_ROOT/tests/compare_schemas.py" --reference "$WORK/schema_python.json")
[ ${#compare[@]} -gt 0 ] && cmd+=(--compare "${compare[@]}")
[ ${#types_only[@]} -gt 0 ] && cmd+=(--types-only "${types_only[@]}")

status=0
"${cmd[@]}" || status=$?

if [ ${#missing[@]} -gt 0 ]; then
  echo
  echo "UNCOMPARED (--allow-missing): ${missing[*]}"
  echo "This run did not gate those SDKs. CI runs without --allow-missing."
fi

if [ ${#failures[@]} -gt 0 ]; then
  echo
  echo "${#failures[@]} generator(s) failed: ${failures[*]}"
  status=1
fi

echo
echo "Not compared here: elixir, dart, csharp, fsharp, java, ruby — their parity"
echo "generators were hand-written JSON that never called the SDK (six of eleven, the"
echo "count manifest.json records). Covered by sdk-conformance.yml, which compiles."

exit "$status"
