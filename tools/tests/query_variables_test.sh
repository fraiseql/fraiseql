#!/usr/bin/env bash
# query_variables_test.sh — the red capability of tools/check-query-variables.py.
#
# Each case is a tree with docs/ and examples/. A `fraiseql_query(` call whose first
# argument is a template literal holding `${` is RED, wherever the literal starts and
# whatever file under docs/, examples/, tests/ or a crate's tests/ holds it, as is a tree
# with no call at all.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-query-variables.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
cases=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

# new_case <name>: one doc and one example, each passing its value as a variable.
new_case() {
  root="${tmp}/$1"
  mkdir -p "${root}/docs/architecture" "${root}/examples/fns"
  cat > "${root}/docs/architecture/functions.md" <<'EOF'
# functions

```ts
const rows = await Deno.core.ops.fraiseql_query(
  `query ($sku: String!) { price(sku: $sku) { amount } }`,
  JSON.stringify({ sku }),
);
```
EOF
  cat > "${root}/examples/fns/write.ts" <<'EOF'
export default async (event) => {
  await Deno.core.ops.fraiseql_query(`mutation($id: ID!) { touch(id: $id) { id } }`, JSON.stringify({ id: `${event.id}` }));
};
EOF
}

expect() {
  cases=$((cases + 1))
  QUERY_VARIABLES_ROOT="$root" python3 "$gate" > "${tmp}/out" 2>&1
  local rc=$?
  if [[ "$rc" -eq "$1" ]] && grep -qF -- "$2" "${tmp}/out"; then pass "$3"; else fail "$3 (rc=$rc, want $1, '$2')"; fi
}

echo "check-query-variables.py:"

new_case clean
expect 0 "query variables: ok (2 calls" "values passed as variables pass, a \${ in the variables argument too"

new_case doc_inline
printf '\n```ts\nawait Deno.core.ops.fraiseql_query(`{ price(sku: "${sku}") { amount } }`, "{}");\n```\n' \
  >> "${root}/docs/architecture/functions.md"
expect 1 "docs/architecture/functions.md:11: fraiseql_query interpolates" "an interpolated query in a doc is red"

new_case next_line
cat >> "${root}/examples/fns/write.ts" <<'EOF'
export const other = async (page) =>
  Deno.core.ops.fraiseql_query(
    // the page to record
    `mutation {
      recordPage(page: ${page}) { id }
    }`,
    "{}",
  );
EOF
expect 1 "examples/fns/write.ts:5: fraiseql_query interpolates" "a literal on a later line, after a comment, is red at the call"

new_case example_md
mkdir -p "${root}/examples/guide"
printf 'Call it as `Deno.core.ops.fraiseql_query(`{ user(id: ${id}) { name } }`, "{}")`.\n' \
  > "${root}/examples/guide/README.md"
expect 1 "examples/guide/README.md:1: fraiseql_query interpolates" "a README under examples/ is checked"

new_case js
printf 'Deno.core.ops.fraiseql_query(`{ a(x: ${x}) }`, "{}");\n' > "${root}/examples/fns/a.js"
expect 1 "examples/fns/a.js:1: fraiseql_query interpolates" "a .js example is checked"

new_case crate_test_fixture
mkdir -p "${root}/crates/demo/tests/capabilities/cron/modules"
printf 'Deno.core.ops.fraiseql_query(`mutation { recordTick(zone: ${zone}) { id } }`, "{}");\n' \
  > "${root}/crates/demo/tests/capabilities/cron/modules/every_minute.ts"
expect 1 "crates/demo/tests/capabilities/cron/modules/every_minute.ts:1: fraiseql_query interpolates" \
  "a module under a crate's tests/ is checked"

new_case repo_test_fixture
mkdir -p "${root}/tests/fixtures/functions"
printf 'Deno.core.ops.fraiseql_query(`{ customer(id: "${id}") { name } }`, "{}");\n' \
  > "${root}/tests/fixtures/functions/read.ts"
expect 1 "tests/fixtures/functions/read.ts:1: fraiseql_query interpolates" \
  "a fixture under the repository's tests/ is checked"

new_case crate_source
mkdir -p "${root}/crates/demo/src"
printf 'Deno.core.ops.fraiseql_query(`{ a(x: ${x}) }`, "{}");\n' > "${root}/crates/demo/src/embed.ts"
expect 0 "query variables: ok (2 calls" "a crate's src/ is not in scope"

new_case no_calls
rm "${root}/docs/architecture/functions.md" "${root}/examples/fns/write.ts"
printf '# nothing here\n' > "${root}/docs/readme.md"
expect 1 "no fraiseql_query call found" "a tree with no call is red, not a vacuous pass"

new_case escaped
printf 'Deno.core.ops.fraiseql_query(`{ a(note: "costs \\${5}") }`, "{}");\n' > "${root}/examples/fns/b.ts"
expect 0 "query variables: ok (3 calls" "an escaped \\\${ is text, not interpolation"

echo
if [[ "$failures" -eq 0 ]]; then
  echo "query_variables_test: ${cases} cases, all as expected"
else
  echo "query_variables_test: ${failures} of ${cases} cases FAILED"
  exit 1
fi
