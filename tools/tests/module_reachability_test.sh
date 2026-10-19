#!/usr/bin/env bash
# module_reachability_test.sh — the red capability of tools/check-module-reachability.py.
#
# The gate's first green run is the tree it just emptied of orphans, which proves
# nothing about whether it would see the next one. So every resolution rule is proved
# on a synthetic crate: a file the rule reaches must stay GREEN, and a file it does
# not reach must go RED, by name.
#
# Cases 6 and 7 are the shape a disconnected module actually leaves behind: the
# `mod` line commented out, or surviving only inside a string. A gate reading raw
# text would count either as a declaration.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-module-reachability.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

mk() { mkdir -p "$(dirname "$1")"; printf '%s\n' "$2" > "$1"; }

# A fresh one-crate tree per case: <root>/crates/demo/{Cargo.toml,src/lib.rs}.
new_crate() {
  root="${tmp}/case$1"
  crate="${root}/crates/demo"
  mk "${crate}/Cargo.toml" '[package]
name = "demo"'
  mk "${crate}/src/lib.rs" "$2"
}

run_gate() {
  MODULE_REACHABILITY_ROOT="$root" python3 "$gate" > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

expect_green() {
  if [ "$(cat "${tmp}/rc")" = 0 ]; then pass "$1"; else fail "$1"; fi
}

expect_red() { # expect_red <file that must be named> <label>
  if [ "$(cat "${tmp}/rc")" = 1 ] && grep -F "$1" "${tmp}/out" >/dev/null; then
    pass "$2"
  else
    fail "$2"
  fi
}

# ── 1. A file nothing declares fails, by name ───────────────────────────────────────
new_crate 1 'pub mod live;'
mk "${crate}/src/live.rs" 'pub fn f() {}'
mk "${crate}/src/orphan.rs" 'pub fn g() {}'
run_gate
expect_red "crates/demo/src/orphan.rs" "an undeclared file is named"

# ── 2. #[path] reaches a file whose name matches no module ──────────────────────────
new_crate 2 'pub mod live;'
mk "${crate}/src/live.rs" '#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;'
mk "${crate}/src/live_tests.rs" '#[test] fn t() {}'
run_gate
expect_green "#[path] (behind another attribute) reaches its file"

# ── 3. Nested inline modules add their names to the directory ───────────────────────
new_crate 3 'mod outer { pub mod middle { pub mod inner; } }'
mk "${crate}/src/outer/middle/inner.rs" ''
run_gate
expect_green "mod outer { mod middle { mod inner; } } reaches outer/middle/inner.rs"

# ── 4. A non-mod-rs file declares into its own stem's directory ─────────────────────
new_crate 4 'mod foo; mod dir;'
mk "${crate}/src/foo.rs" 'mod bar;'
mk "${crate}/src/foo/bar.rs" 'mod baz;'
mk "${crate}/src/foo/bar/baz.rs" ''
mk "${crate}/src/dir/mod.rs" 'mod leaf;'
mk "${crate}/src/dir/leaf.rs" ''
run_gate
expect_green "foo.rs + foo/ and dir/mod.rs both resolve their children"

# ── 5. A feature-gated declaration still makes its file part of the crate ───────────
new_crate 5 '#[cfg(feature = "extra")]
pub mod gated;'
mk "${crate}/src/gated.rs" ''
run_gate
expect_green "#[cfg(feature)] mod counts as reachable"

# ── 6. A commented-out declaration reaches nothing ──────────────────────────────────
new_crate 6 '// pub mod tracing;
/* mod also_dead; */
pub fn f() {}'
mk "${crate}/src/tracing/mod.rs" 'mod spans;'
mk "${crate}/src/tracing/spans.rs" ''
mk "${crate}/src/also_dead.rs" ''
run_gate
expect_red "crates/demo/src/tracing/spans.rs" "// mod x; does not declare x (nor its children)"
expect_red "crates/demo/src/also_dead.rs" "/* mod x; */ does not declare x"

# ── 7. A declaration inside a string literal reaches nothing ────────────────────────
new_crate 7 'pub const DOC: &str = "mod ghost;";
pub const RAW: &str = r#"mod phantom;"#;'
mk "${crate}/src/ghost.rs" ''
mk "${crate}/src/phantom.rs" ''
run_gate
expect_red "crates/demo/src/ghost.rs" "\"mod x;\" in a string does not declare x"
expect_red "crates/demo/src/phantom.rs" "r#\"mod x;\"# in a raw string does not declare x"

# ── 8. A #[path]-loaded file resolves its own children as a mod-rs file ─────────────
new_crate 8 '#[path = "elsewhere/impl_.rs"]
mod imp;'
mk "${crate}/src/elsewhere/impl_.rs" 'mod helper;'
mk "${crate}/src/elsewhere/helper.rs" ''
run_gate
expect_green "children of a #[path] file resolve beside it"

# ── 9. Binaries, a build script and a manifest-named root are roots ─────────────────
new_crate 9 ''
rm "${crate}/src/lib.rs"
mk "${crate}/Cargo.toml" '[package]
name = "demo"

[lib]
path = "src/entry.rs"'
mk "${crate}/src/entry.rs" 'mod from_entry;'
mk "${crate}/src/from_entry.rs" ''
mk "${crate}/src/bin/tool.rs" 'mod helper;'
mk "${crate}/src/bin/helper.rs" ''
mk "${crate}/src/bin/multi/main.rs" 'mod part;'
mk "${crate}/src/bin/multi/part.rs" ''
run_gate
expect_green "[lib] path, src/bin/*.rs and src/bin/*/main.rs are roots"

# ── 10. A tree with no crates refuses rather than passing vacuously ─────────────────
root="${tmp}/empty"
mkdir -p "$root"
run_gate
if [ "$(cat "${tmp}/rc")" = 1 ] && grep -F "refusing to pass vacuously" "${tmp}/out" >/dev/null; then
  pass "an empty tree is refused"
else
  fail "an empty tree is refused"
fi

if [ "$failures" -ne 0 ]; then
  echo "module_reachability_test: ${failures} case(s) failed"
  exit 1
fi
echo "module_reachability_test: all cases passed"
