#!/usr/bin/env bash
# prose_ratchet_test.sh — the red capability of tools/check-prose-ratchet.py.
#
# Every case is a one-crate tree `crates/c/src/lib.rs` with a baseline of `c 1`. A
# reference the gate must count takes the crate to 2 and goes RED; one it must ignore
# keeps it at 1 and stays GREEN. The ignored shapes are the ones a grep gets wrong:
# doc comments, string literals, raw strings, and test code.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-prose-ratchet.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
cases=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

# new_case <name> <extra lib.rs code>: one counted reference, plus the extra code.
new_case() {
  root="${tmp}/$1"
  mkdir -p "${root}/crates/c/src" "${root}/tools"
  printf '// Counted once (#1001).\npub fn a() {}\n%s\n' "$2" > "${root}/crates/c/src/lib.rs"
  printf 'c 1\n' > "${root}/tools/prose-ratchet-baseline.txt"
}

# expect <rc> <substring> <description>
expect() {
  cases=$((cases + 1))
  PROSE_RATCHET_ROOT="$root" python3 "$gate" > "${tmp}/out" 2>&1
  local rc=$?
  if [[ "$rc" -eq "$1" ]] && grep -qF -- "$2" "${tmp}/out"; then pass "$3"; else fail "$3 (rc=$rc, want $1, '$2')"; fi
}

echo "check-prose-ratchet.py:"

new_case clean ''
expect 0 "prose ratchet: ok" "the baseline count passes"

new_case line '// Added later, see #1234.'
expect 1 "c: 2 issue references" "a plain line comment counts"

new_case trailing 'pub fn b() {} // #1234'
expect 1 "c: 2 issue references" "a trailing comment counts"

new_case four_slash '//// #1234'
expect 1 "c: 2 issue references" "four slashes is a plain comment"

new_case block '/* #1234 */'
expect 1 "c: 2 issue references" "a block comment counts"

new_case nested '/* outer /* inner */ #1234 */'
expect 1 "c: 2 issue references" "a nested block comment counts past its inner close"

new_case doc '/// Behaviour defined by #1234.
//! Crate note (#1235).
/** Block doc #1236 */
/*! Inner block doc #1237 */'
expect 0 "prose ratchet: ok" "doc comments are not counted"

new_case strings 'const A: &str = "// #1234";
const B: &str = r#"say "hi // #1235 "#;
const C: &[u8] = b"// #1236";
const F: &[u8] = br#"say "hi // #1238 "#;
const D: char = '"'"'"'"'"'; // closes nothing
fn e<'"'"'a>(x: &'"'"'a str) -> &'"'"'a str { x }'
expect 0 "prose ratchet: ok" "string, raw-string, byte-string and char literals, and lifetimes, are not comments"

new_case cfg_test '#[cfg(test)]
mod tests {
    // Regression for #1234.
    fn f() { let _ = { 1 }; }
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod more {
    /* #1235 */
}'
expect 0 "prose ratchet: ok" "a #[cfg(test)] module body is not counted"

new_case after_test '#[cfg(test)]
mod tests {
    fn f() { let s = "{"; let c = '"'"'{'"'"'; }
}
// After the test module: #1234.'
expect 1 "c: 2 issue references" "counting resumes after a test module, an opening brace in a literal notwithstanding"

new_case close_in_string '#[cfg(test)]
mod tests {
    fn f() { let s = "}"; }
    // Still inside the test module: #1234.
}'
expect 0 "prose ratchet: ok" "a closing brace in a string does not end the test module"

new_case widths '// #12, #12345, ##1234, x#1234'
expect 0 "prose ratchet: ok" "two- and five-digit numbers and non-references are not counted"

new_case test_files ''
mkdir -p "${root}/crates/c/src/tests" "${root}/crates/c/src/x"
for f in tests/a.rs tests.rs x/b_tests.rs x/c_test.rs; do printf '// #1234\n' > "${root}/crates/c/src/$f"; done
expect 0 "prose ratchet: ok" "tests/ directories, tests.rs, *_tests.rs and *_test.rs are not counted"

new_case module_file ''
printf '// #1234\n' > "${root}/crates/c/src/other.rs"
expect 1 "c: 2 issue references" "every non-test file of the crate counts"

new_case drop ''
printf '// Counted once (no issue).\n' > "${root}/crates/c/src/lib.rs"
expect 1 "c: down to 0 from 1; lock it in: \`c 0\`" "a drop below the baseline must be locked in"

new_case missing ''
mkdir -p "${root}/crates/d/src"; printf '// #1234\n' > "${root}/crates/d/src/lib.rs"
expect 1 "d: not in the baseline; add \`d 1\`" "a crate missing from the baseline fails"

new_case stale ''
printf 'c 1\ngone 3\n' > "${root}/tools/prose-ratchet-baseline.txt"
expect 1 "gone: in the baseline but not a crate" "a baseline line for no crate fails"

root="${tmp}/empty"; mkdir -p "$root"
expect 1 "nothing to check" "a tree with no crates is red, not green"

echo "${cases} cases, ${failures} failed"
[ "$failures" -eq 0 ]
