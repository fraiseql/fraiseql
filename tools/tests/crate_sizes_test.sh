#!/usr/bin/env bash
# crate_sizes_test.sh — the red capability of tools/check-crate-sizes.py.
#
# One fixture crate exercises every counting rule: each construct sits in a file or at
# a line whose expected count is pinned below, so a rule that stops holding moves a
# line between production, comment, blank and test, and the case fails. Then the
# budget verdicts: within budget, over budget, and a crate with no budget row.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-crate-sizes.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
cases=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

root="${tmp}/ws"
src="${root}/crates/demo/src"
mkdir -p "${src}/pg_schema" "${src}/sub/tests" "${root}/crates/demo/tests"

# lib.rs: docs, blanks, a block comment, braces, a `"` and a `//` inside string literals, a
# test-only item under cfg(all(test, …)), a production item under cfg(any(test, …)),
# test-only `mod` declarations (plain and with #[path]) and a trailing test module.
cat > "${src}/lib.rs" <<'EOF'
//! A crate.

/// A function.
pub fn f() -> usize {
    let s = "{ not a brace";
    let raw = r#"a"
// inside a raw string
"#;
    /* a block
       comment */
    s.len() + raw.len() // trailing
}

#[cfg(any(test, feature = "testing"))]
pub fn helper() {}

#[cfg(all(test, feature = "extra"))]
fn only_in_tests() {
    let _ = "}";
}

#[cfg(test)]
mod pg_schema;

#[cfg(test)]
#[path = "odd_name.rs"]
mod odd;

mod sub;
mod tests;
mod test_util;
mod inner_cfg;
mod not_test;
mod char_brace;

#[cfg(test)]
mod unit {
    #[test]
    fn t() {
        assert_eq!(super::f(), 0);
    }
}
EOF
# Declared by a test-only `mod pg_schema;`, with a submodule in its directory: test,
# though neither name says so.
printf 'pub fn a() {}\n\npub fn b() {}\n' > "${src}/pg_schema.rs"
printf 'pub fn c() {}\npub fn d() {}\n' > "${src}/pg_schema/inner.rs"
# Declared by a test-only `#[path = "odd_name.rs"] mod odd;`.
printf 'pub fn e() {}\npub fn g() {}\n' > "${src}/odd_name.rs"
# Test by name alone: none is declared test-only.
printf 'fn t1() {}\nfn t2() {}\n' > "${src}/tests.rs"
printf 'fn u1() {}\nfn u2() {}\n' > "${src}/test_util.rs"
printf 'mod widget_tests;\npub fn h() {}\n' > "${src}/sub.rs"
printf 'fn w1() {}\nfn w2() {}\n' > "${src}/sub/widget_tests.rs"
printf 'fn c1() {}\nfn c2() {}\n' > "${src}/sub/tests/case.rs"
# A whole file under #![cfg(test)]; and cfg(not(test)), which is production.
printf '#![cfg(test)]\nfn x() {}\n' > "${src}/inner_cfg.rs"
printf '#[cfg(not(test))]\nfn y() {}\n' > "${src}/not_test.rs"
# A `}` and an escaped quote in char literals inside a test module.
printf "#[cfg(test)]\nmod t {\n    const C: char = '}';\n    const Q: char = '\\\\'';\n}\npub fn after() {}\n" \
  > "${src}/char_brace.rs"
printf 'fn it() {}\nfn it2() {}\nfn it3() {}\n' > "${root}/crates/demo/tests/it.rs"

budget() {
  printf '[workspace]\nmembers = []\n\n[workspace.metadata.crate-size-budget]\n%s\n' "$1" > "${root}/Cargo.toml"
}

expect() {
  cases=$((cases + 1))
  CRATE_SIZES_ROOT="$root" python3 "$gate" "${@:3}" > "${tmp}/out" 2>&1
  local rc=$?
  if [[ "$rc" -eq "$1" ]] && grep -qF -- "$2" "${tmp}/out"; then pass "$2"; else fail "$2 (rc=$rc, want $1)"; fi
}

echo "check-crate-sizes.py:"
budget "demo = 100"

# Production in lib.rs: f's 7 code lines (a line inside the raw string is code; one
# with a trailing comment is too), helper's 2 under cfg(any(test, …)), and `mod sub;`
# through `mod char_brace;` (6). Comments: the 2 doc lines and the 2 of the block
# comment. Blanks: the 7 between items. Test: only_in_tests (4), the two test-only
# mod declarations (2 and 3) and the trailing test module (7).
expect 0 "lib.rs production=15 comment=4 blank=7 test=16" --files demo
expect 0 "pg_schema.rs production=0 comment=0 blank=0 test=3" --files demo
expect 0 "pg_schema/inner.rs production=0 comment=0 blank=0 test=2" --files demo
expect 0 "odd_name.rs production=0 comment=0 blank=0 test=2" --files demo
expect 0 "tests.rs production=0 comment=0 blank=0 test=2" --files demo
expect 0 "test_util.rs production=0 comment=0 blank=0 test=2" --files demo
expect 0 "sub/widget_tests.rs production=0 comment=0 blank=0 test=2" --files demo
expect 0 "sub/tests/case.rs production=0 comment=0 blank=0 test=2" --files demo
expect 0 "inner_cfg.rs production=0 comment=0 blank=0 test=2" --files demo
expect 0 "not_test.rs production=2 comment=0 blank=0 test=0" --files demo
expect 0 "char_brace.rs production=1 comment=0 blank=0 test=5" --files demo
expect 0 "sub.rs production=2 comment=0 blank=0 test=0" --files demo

# The crate's production total is 20: lib.rs 15, not_test.rs 2, sub.rs 2, char_brace.rs 1.
# Its 38 test lines and the 3 lines of its tests/ are reported, not gated.
expect 0 "demo                               20      100 ok                4         38        3" demo
budget "demo = 20"
expect 0 "crate sizes: ok (1 crates within budget" demo
budget "demo = 19"
expect 1 "demo: 20 production lines, over its budget of 19 by 1" demo

budget "demo = 100"
mkdir -p "${root}/crates/fresh/src"
printf 'pub fn z() {}\n' > "${root}/crates/fresh/src/lib.rs"
expect 1 "fresh has a src/ directory but no row"
budget "demo = 100
fresh = 10"
expect 0 "crate sizes: ok (2 crates within budget"

echo
if [[ "$failures" -eq 0 ]]; then
  echo "crate_sizes_test: ${cases} cases, all as expected"
else
  echo "crate_sizes_test: ${failures} of ${cases} cases FAILED"
  exit 1
fi
