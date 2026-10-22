#!/usr/bin/env bash
# docs_rust_examples_test.sh — the red capability of tools/check-docs-rust-examples.py.
#
# Each case is a tree with crates/fraiseql/src/lib.rs (the doctest module) and docs. A
# doc with a ```rust block that the module does not include is RED, as is a listed doc
# that lost its block, a bare fence anywhere, and a tree with no Rust example at all.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-docs-rust-examples.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
cases=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

# new_case <name>: docs/guide.md with one rust block, listed; README with a text fence.
new_case() {
  root="${tmp}/$1"
  mkdir -p "${root}/crates/fraiseql/src" "${root}/docs"
  printf '# guide\n\n```rust\nlet x = 1;\n```\n' > "${root}/docs/guide.md"
  printf '# readme\n\n```text\ndiagram\n```\n' > "${root}/README.md"
  printf '#[cfg(doctest)]\nmod docs_examples {\n    #[doc = include_str!("../../../docs/guide.md")]\n    struct Guide;\n}\n' \
    > "${root}/crates/fraiseql/src/lib.rs"
}

expect() {
  cases=$((cases + 1))
  DOCS_RUST_ROOT="$root" python3 "$gate" > "${tmp}/out" 2>&1
  local rc=$?
  if [[ "$rc" -eq "$1" ]] && grep -qF -- "$2" "${tmp}/out"; then pass "$3"; else fail "$3 (rc=$rc, want $1, '$2')"; fi
}

echo "check-docs-rust-examples.py:"

new_case clean
expect 0 "docs rust examples: ok (1 documents" "a listed doc and labeled fences pass"

new_case unlisted
mkdir -p "${root}/docs/sub"; printf '```rust,no_run\nfn f() {}\n```\n' > "${root}/docs/sub/new.md"
expect 1 "docs/sub/new.md: has a \`\`\`rust block but is not included" "an unlisted doc with a rust block is red, whatever its attributes"

new_case stale
printf '# guide\n\n```text\nno rust now\n```\n' > "${root}/docs/guide.md"
printf '```rust\nlet y = 2;\n```\n' > "${root}/docs/other.md"
sed -i 's|docs/guide.md|docs/gone.md|' "${root}/crates/fraiseql/src/lib.rs"
printf '#[doc = include_str!("../../../docs/other.md")]\n' >> "${root}/crates/fraiseql/src/lib.rs"
expect 1 "docs/gone.md: included in the doctest module but has no" "a listed doc that is gone is red"

new_case bare
printf '\n```\nwhat is this?\n```\n' >> "${root}/docs/guide.md"
expect 1 "docs/guide.md:7: bare fence" "a bare fence is red"

new_case bare_readme
printf '\n```\nplain\n```\n' >> "${root}/README.md"
expect 1 "README.md:7: bare fence" "README.md is checked"

new_case history
mkdir -p "${root}/docs/history" "${root}/docs/adr"
printf '```\nold\n```\n```rust\nold();\n```\n' > "${root}/docs/history/old.md"
printf '```rust\nsketch();\n```\n' > "${root}/docs/adr/0001.md"
expect 0 "docs rust examples: ok" "history and ADRs are not checked"

new_case nested
printf '\n````markdown\n```\ninner bare fence is content\n```\n````\n' >> "${root}/docs/guide.md"
expect 0 "docs rust examples: ok" "a fence inside a longer fence is content, not a fence"

new_case nolib
rm "${root}/crates/fraiseql/src/lib.rs"
expect 1 "not found" "a missing doctest module is red"

new_case none
printf '# guide\n\n```text\nnone\n```\n' > "${root}/docs/guide.md"
expect 1 "nothing to check" "no rust example anywhere is red"

echo "${cases} cases, ${failures} failed"
[ "$failures" -eq 0 ]
