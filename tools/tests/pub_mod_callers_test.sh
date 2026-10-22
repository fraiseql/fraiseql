#!/usr/bin/env bash
# pub_mod_callers_test.sh — the red capability of tools/check-pub-mod-callers.py.
#
# Every case is a two-crate workspace: `app` (a binary) depends on `lib`. A module the
# binary reaches must stay GREEN; a module it does not reach must go RED, by name. The
# cases that matter most are the ones a reference counter gets wrong: a module kept
# alive by a `pub use` nobody follows, by its own `#[cfg(test)]` code, by an
# integration test, or by an impl of a trait nobody uses.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-pub-mod-callers.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

mk() { mkdir -p "$(dirname "$1")"; printf '%s\n' "$2" > "$1"; }

# new_case <n> <lib.rs> <main.rs>: a fresh workspace with an empty allowlist.
new_case() {
  root="${tmp}/case$1"
  lib="${root}/crates/lib"
  app="${root}/crates/app"
  allow="${root}/allowlist.txt"
  mk "${lib}/Cargo.toml" '[package]
name = "lib"'
  mk "${lib}/src/lib.rs" "$2"
  mk "${app}/Cargo.toml" '[package]
name = "app"

[dependencies]
lib = { path = "../lib" }'
  mk "${app}/src/main.rs" "$3"
  : > "$allow"
}

run_gate() {
  PUB_MOD_CALLERS_ROOT="$root" PUB_MOD_CALLERS_ALLOWLIST="$allow" python3 "$gate" > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

expect_green() {
  if [ "$(cat "${tmp}/rc")" = 0 ]; then pass "$1"; else fail "$1"; fi
}

expect_red() { # expect_red <text that must be printed> <label>
  if [ "$(cat "${tmp}/rc")" = 1 ] && grep -F -- "$1" "${tmp}/out" >/dev/null; then
    pass "$2"
  else
    fail "$2"
  fi
}

# ── 1. A module no binary names fails, by name; a used sibling does not ─────────────
new_case 1 'pub mod live { pub fn f() {} }
pub mod dead { pub fn g() {} }' 'fn main() { lib::live::f(); }'
run_gate
expect_red "✗ lib::dead (" "an unused pub mod is named"
if grep -F "lib::live" "${tmp}/out" >/dev/null; then fail "a used pub mod is not named"; else pass "a used pub mod is not named"; fi

# ── 2. A `use` tree with groups and aliases is a reference ──────────────────────────
new_case 2 'pub mod a { pub mod b { pub struct S; } pub mod c { pub fn h() {} } }' \
  'use lib::a::{b::S as Renamed, c::{self}};
fn main() { let _ = Renamed; c::h(); }'
run_gate
expect_green "use lib::a::{b::S as X, c::{self}} reaches a::b and a::c"

# ── 3. A `pub use` re-export nobody follows keeps nothing alive ─────────────────────
# The crate root itself is used (main calls lib::api), so its re-export is read.
new_case 3 'pub mod hidden { pub struct Gone; }
pub use hidden::Gone;
pub fn api() {}' 'fn main() { lib::api(); }'
run_gate
expect_red "✗ lib::hidden (" "a re-export with no caller does not count"

# ── 4. …but a caller through the re-export does ─────────────────────────────────────
new_case 4 'pub mod hidden { pub struct Used; }
pub use hidden::Used;' 'fn main() { let _ = lib::Used; }'
run_gate
expect_green "lib::Used resolves through the re-export to lib::hidden"

# ── 5. A bare name reached through a glob re-export ─────────────────────────────────
new_case 5 'pub mod parts { pub struct Part; }
pub use parts::*;
pub struct Whole { pub p: Part }' 'fn main() { let _ = lib::Whole { p: todo!() }; }'
run_gate
expect_green "a field type named through a glob re-export reaches its module"

# ── 6. Use from #[cfg(test)] code only does not count ───────────────────────────────
# The test code sits in a module the binary uses, so only the attribute excludes it.
new_case 6 'pub mod helper { pub fn h() {} }
pub mod in_test_fn { pub fn h() {} }
pub fn api() {}
#[cfg(test)]
fn only_in_tests() { crate::helper::h(); }
#[test]
fn a_test() { crate::in_test_fn::h(); }' 'fn main() { lib::api(); }'
run_gate
expect_red "✗ lib::helper (" "#[cfg(test)] code is not a caller"
expect_red "✗ lib::in_test_fn (" "#[test] code is not a caller"

# ── 7. Use from an integration test only does not count ─────────────────────────────
new_case 7 'pub mod fixtures { pub fn f() {} }' 'fn main() {}'
mk "${lib}/tests/it.rs" '#[test] fn t() { lib::fixtures::f(); }'
run_gate
expect_red "✗ lib::fixtures (" "a tests/ target is not a caller"

# ── 8. …and allowlisting it as test-support turns it green ─────────────────────────
printf '%s\n' 'lib::fixtures test-support shared by the integration suite' > "$allow"
run_gate
expect_green "a test-support module a test uses is accepted"

# ── 9. A test-support module no test uses is still dead ─────────────────────────────
new_case 9 'pub mod fixtures { pub fn f() {} }' 'fn main() {}'
printf '%s\n' 'lib::fixtures test-support nothing uses it' > "$allow"
run_gate
expect_red "allowlisted as test-support, but no test or example uses it" "an unused test-support module fails"

# ── 10. An allowlist entry for a module a binary uses is stale ──────────────────────
new_case 10 'pub mod m { pub fn f() {} }' 'fn main() { lib::m::f(); }'
printf '%s\n' 'lib::m pending #1 wired since' > "$allow"
run_gate
expect_red "allowlist entry lib::m: a binary uses it" "a stale allowlist entry fails"

# ── 11. An allowlist entry for a module that does not exist fails ───────────────────
new_case 11 'pub fn f() {}' 'fn main() { lib::f(); }'
printf '%s\n' 'lib::deleted pending #1 already gone' > "$allow"
run_gate
expect_red "allowlist entry lib::deleted: no such library module" "an entry naming a deleted module fails"

# ── 12. A pending entry cites an issue; a kind must be known ────────────────────────
new_case 12 'pub mod later { pub fn f() {} }
pub mod other { pub fn g() {} }' 'fn main() {}'
printf '%s\n' 'lib::later pending some day' 'lib::other forever because' > "$allow"
run_gate
expect_red "a pending entry cites the issue" "a pending entry without #N fails"
expect_red "kind must be one of" "an unknown kind fails"
printf '%s\n' 'lib::later pending wired by #1642' 'lib::other api documented embedder entry point' > "$allow"
mk "${lib}/examples/ex.rs" 'fn main() { lib::other::g(); }'
run_gate
expect_green "pending #N and an api module an example uses are accepted"

# ── 13. impl of an unused local trait for a used type keeps nothing alive ───────────
new_case 13 'pub mod types { pub struct T; impl T { pub fn new() -> Self { T } } }
pub mod plugin {
    pub trait Plugin { fn run(&self); }
    impl Plugin for crate::types::T { fn run(&self) {} }
}' 'fn main() { let _ = lib::types::T::new(); }'
run_gate
expect_red "✗ lib::plugin (" "impl LocalTrait for UsedType, trait unused, is dead"

# ── 14. impl of an external trait for a used type is reached by method calls ────────
new_case 14 'pub mod types { pub struct T; }
pub mod display { impl std::fmt::Display for crate::types::T {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } } }' \
  'fn main() { println!("{}", lib::types::T); }'
run_gate
expect_green "impl ExternalTrait for UsedType counts as used"

# ── 15. An inherent impl counts only if one of its methods is called ────────────────
new_case 15 'pub mod types { pub struct T; }
pub mod extra { impl crate::types::T { pub fn helper(&self) {} } }
pub mod unused_extra { impl crate::types::T { pub fn never(&self) {} } }' \
  'fn main() { let t = lib::types::T; t.helper(); }'
run_gate
expect_red "✗ lib::unused_extra (" "an inherent impl whose methods nobody calls is dead"
if grep -F "lib::extra " "${tmp}/out" >/dev/null; then fail "an inherent impl whose method is called is used"; else pass "an inherent impl whose method is called is used"; fi

# ── 16. A name with a #[cfg] stub and a real re-export resolves to both ─────────────
new_case 16 'pub mod real { pub struct Client; }
#[cfg(feature = "on")]
pub use real::Client;
#[cfg(not(feature = "on"))]
pub struct Client;
pub mod real_mod { pub struct Conn; }
#[cfg(feature = "on")]
pub use real_mod as facade;
#[cfg(not(feature = "on"))]
pub mod facade { pub struct Conn; }' 'fn main() { let _ = lib::Client; let _ = lib::facade::Conn; }'
run_gate
expect_green "a cfg-alternative stub (item or module) does not hide the re-export"

# ── 17. A macro_rules! invocation reaches the module defining it ────────────────────
new_case 17 'pub mod macros {
    #[macro_export]
    macro_rules! shout { () => {} }
}' 'fn main() { lib::shout!(); }'
run_gate
expect_green "a #[macro_export] macro reaches its module"

# ── 18. Liveness flows through libraries, and only from used modules ────────────────
new_case 18 'pub mod a { pub fn f() { crate::b::g(); } }
pub mod b { pub fn g() {} }
pub mod c { pub fn h() { crate::d::k(); } }
pub mod d { pub fn k() {} }' 'fn main() { lib::a::f(); }'
run_gate
expect_red "✗ lib::c (" "a module only a dead module calls is dead (c)"
expect_red "✗ lib::d (" "a module only a dead module calls is dead (d)"
if grep -F "lib::b " "${tmp}/out" >/dev/null; then fail "a module a used module calls is used"; else pass "a module a used module calls is used"; fi

# ── 19. A renamed dependency is resolved by its manifest key ────────────────────────
new_case 19 'pub mod m { pub fn f() {} }' 'fn main() { other::m::f(); }'
mk "${app}/Cargo.toml" '[package]
name = "app"

[dependencies]
other = { package = "lib", path = "../lib" }'
run_gate
expect_green "other = { package = \"lib\" } resolves other::m"

# ── 20. super::, self:: and file modules resolve ────────────────────────────────────
new_case 20 'pub mod outer;' 'fn main() { lib::outer::entry(); }'
mk "${lib}/src/outer/mod.rs" 'pub mod inner;
pub mod sibling;
pub fn entry() { self::inner::go(); }'
mk "${lib}/src/outer/inner.rs" 'pub fn go() { super::sibling::s(); }'
mk "${lib}/src/outer/sibling.rs" 'pub fn s() {}'
run_gate
expect_green "self::inner and super::sibling resolve across files"

# ── 21. A module under a private parent is rustc's dead_code lint, not ours ─────────
new_case 21 'mod private { pub mod nested { pub fn f() {} } }
pub(crate) mod scoped { pub fn g() {} }' 'fn main() {}'
run_gate
expect_green "a pub mod with a non-pub ancestor is not checked"

# ── 22. A mention in a comment or string is not a reference ─────────────────────────
new_case 22 'pub mod ghost { pub fn f() {} }' '// lib::ghost::f();
fn main() { let _ = "lib::ghost::f()"; }'
run_gate
expect_red "✗ lib::ghost (" "a comment or string naming a module is not a caller"

# ── 23. A dead parent is reported once, not once per child ──────────────────────────
new_case 23 'pub mod tree { pub mod a { pub fn f() {} } pub mod b { pub fn g() {} } }' 'fn main() {}'
run_gate
if [ "$(cat "${tmp}/rc")" = 1 ] && grep -F "✗ lib::tree (" "${tmp}/out" >/dev/null \
   && ! grep -F "lib::tree::a" "${tmp}/out" >/dev/null; then
  pass "a dead subtree is reported at its root"
else
  fail "a dead subtree is reported at its root"
fi

# ── 25. A doctest is a test, not a caller ───────────────────────────────────────────
new_case 25 '//! ```
//! use lib::prelude::*;
//! ```
//! ```text
//! lib::not_code::x();
//! ```
pub mod prelude { pub fn p() {} }
pub mod not_code { pub fn x() {} }' 'fn main() {}'
run_gate
expect_red "✗ lib::prelude (" "a doctest does not make a binary caller"
printf '%s\n' 'lib::prelude api embedder entry point' 'lib::not_code api documented only in prose' > "$allow"
run_gate
if grep -F "lib::prelude (" "${tmp}/out" >/dev/null; then
  fail "a doctest counts as a test use"
else
  pass "a doctest counts as a test use"
fi
expect_red "✗ lib::not_code (crates/lib/src/lib.rs, 1 lines): allowlisted as api, but no test" "a \`\`\`text block is not a doctest"

# ── 26. Declaring a module is not using it ──────────────────────────────────────────
new_case 26 'pub mod parts;
pub mod declared_only;
pub use parts::*;
pub fn api() -> Part { Part }' 'fn main() { lib::api(); }'
mk "${lib}/src/parts.rs" 'pub struct Part;'
mk "${lib}/src/declared_only.rs" 'pub fn never() {}'
run_gate
expect_red "✗ lib::declared_only (" "a pub mod line is not a reference, even beside a glob"

# ── 24. Vacuity: no crates, or no binary, is refused ────────────────────────────────
root="${tmp}/empty"
mkdir -p "$root"
allow="${root}/allowlist.txt"
run_gate
expect_red "no crates/*/src under" "an empty tree is refused"
new_case 24 'pub mod m {}' 'fn main() {}'
rm "${app}/src/main.rs"
run_gate
expect_red "no binary target found" "a workspace with no binary is refused"

if [ "$failures" -ne 0 ]; then
  echo "pub_mod_callers_test: ${failures} case(s) failed"
  exit 1
fi
echo "pub_mod_callers_test: all cases passed"
