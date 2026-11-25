#!/usr/bin/env bash
# fuzz_compiles_test.sh — the red capability of tools/check-fuzz-compiles.sh.
#
# Both halves have to be able to fail, and neither is observable from a passing run of
# the real tree: a gate whose glob matched nothing, or whose cargo exit code was read
# through a pipe, would print OK forever over the exact defect it exists to catch
# (#1254).
#
# The fixtures are minimal cargo crates with no dependencies — a fuzz crate's shape
# (its own `[workspace]`, `[[bin]]` entries under `fuzz_targets/`) without libfuzzer,
# so the cargo half runs offline in about a second.
set -uo pipefail

# From this file, not git: the CI legs that run it mount the tree without `.git`.
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
gate="${repo_root}/tools/check-fuzz-compiles.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0

pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); }

# Build a fixture tree: <root>/crates/<crate>/fuzz/{Cargo.toml,fuzz_targets/<t>.rs}
make_fixture() {
  local root="$1" crate="$2" target="$3" body="$4" declare_bin="$5"
  local dir="${root}/crates/${crate}/fuzz"
  mkdir -p "${dir}/fuzz_targets"
  {
    if [ "$declare_bin" = "yes" ]; then
      printf '[[bin]]\ndoc = false\nname = "%s"\npath = "fuzz_targets/%s.rs"\n\n' "$target" "$target"
    fi
    printf '[package]\nedition = "2021"\nname = "%s-fuzz"\npublish = false\nversion = "0.0.0"\n\n' "$crate"
    printf '[workspace]\nmembers = ["."]\n'
  } > "${dir}/Cargo.toml"
  printf '%s\n' "$body" > "${dir}/fuzz_targets/${target}.rs"
  # The lock a fuzz crate commits. No dependencies, so this resolves offline.
  cargo generate-lockfile --offline --quiet --manifest-path "${dir}/Cargo.toml"
}

run_gate() {
  local root="$1"
  shift
  ( cd "$root" && env "$@" FUZZ_COMPILE_ROOT="$root" bash "$gate" ) > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

# ── 1. A fuzz crate that does not compile is a failure ────────────────────────────────
root="${tmp}/broken"
make_fixture "$root" "demo" "boom" 'fn main() { let _x: u8 = "not a u8"; }' yes
run_gate "$root"
rc="$(cat "${tmp}/rc")"
if [ "$rc" = "1" ] && grep -F "the fuzz crate does not compile" "${tmp}/out" >/dev/null; then
  pass "a fuzz crate with a type error fails the gate"
else
  fail "a fuzz crate with a type error should fail the gate (rc=${rc})"
  sed 's/^/       /' "${tmp}/out"
fi

# ── 2. The same tree, compiling, passes ───────────────────────────────────────────────
root="${tmp}/ok"
make_fixture "$root" "demo" "fine" 'fn main() { let _x: u8 = 1; }' yes
run_gate "$root"
rc="$(cat "${tmp}/rc")"
if [ "$rc" = "0" ] && grep -F "OK: all 1 fuzz crates compile" "${tmp}/out" >/dev/null; then
  pass "a compiling fuzz crate passes the gate"
else
  fail "a compiling fuzz crate should pass the gate (rc=${rc})"
  sed 's/^/       /' "${tmp}/out"
fi

# ── 3. A target file with no [[bin]] is a failure ─────────────────────────────────────
# The gate's own blind spot: cargo only compiles declared bins, so an undeclared target
# would be skipped silently and the gate would report OK over it. That is not
# hypothetical — fraiseql-wire's connection_string.rs was in exactly this state for
# 25 days.
root="${tmp}/undeclared"
make_fixture "$root" "demo" "orphan" 'fn main() {}' no
run_gate "$root" FUZZ_COMPILE_NO_CARGO=1
rc="$(cat "${tmp}/rc")"
if [ "$rc" = "1" ] && grep -F "has no [[bin]] in" "${tmp}/out" >/dev/null; then
  pass "a fuzz target with no [[bin]] fails the gate"
else
  fail "a fuzz target with no [[bin]] should fail the gate (rc=${rc})"
  sed 's/^/       /' "${tmp}/out"
fi

# ── 4. Discovering nothing is a failure, not a pass ───────────────────────────────────
# The shape that makes a gate a comment: the glob stops matching after a layout change
# and every subsequent run reports success over zero subjects.
root="${tmp}/empty"
mkdir -p "${root}/crates"
run_gate "$root" FUZZ_COMPILE_NO_CARGO=1
rc="$(cat "${tmp}/rc")"
if [ "$rc" = "1" ] && grep -F "no fuzz manifests found" "${tmp}/out" >/dev/null; then
  pass "a tree with no fuzz manifests fails rather than reporting OK"
else
  fail "a tree with no fuzz manifests should fail (rc=${rc})"
  sed 's/^/       /' "${tmp}/out"
fi

# ── 5. A lock that is not what the manifest resolves to is a failure ──────────────────
# Each fuzz crate is its own workspace with its own Cargo.lock, which nothing else
# resolves. When a crate it depends on by path gains a dependency, the lock goes stale,
# and every `cargo fuzz build` rewrites it: the campaign then builds versions nobody
# committed. The fixture's fuzz crate depends on a sibling by path, the lock is written,
# and then the sibling gains a dependency of its own.
root="${tmp}/stale"
make_fixture "$root" "demo" "fine" 'fn main() {}' yes
for lib in base extra; do
  mkdir -p "${root}/crates/${lib}/src"
  printf '[package]\nedition = "2021"\nname = "%s"\npublish = false\nversion = "0.0.0"\n' "$lib" \
    > "${root}/crates/${lib}/Cargo.toml"
  : > "${root}/crates/${lib}/src/lib.rs"
done
printf '\n[dependencies]\nbase = { path = "../../base" }\n' >> "${root}/crates/demo/fuzz/Cargo.toml"
cargo generate-lockfile --offline --quiet --manifest-path "${root}/crates/demo/fuzz/Cargo.toml"
printf '\n[dependencies]\nextra = { path = "../extra" }\n' >> "${root}/crates/base/Cargo.toml"
lock="${root}/crates/demo/fuzz/Cargo.lock"
before="$(cksum < "$lock")"
run_gate "$root"
rc="$(cat "${tmp}/rc")"
if [ "$rc" = "1" ] && grep -F "its Cargo.lock is not what its manifest resolves to" "${tmp}/out" >/dev/null \
  && [ "$(cksum < "$lock")" = "$before" ]; then
  pass "a stale fuzz-crate lock fails the gate, and the gate leaves it as it was"
else
  fail "a stale fuzz-crate lock should fail the gate without rewriting it (rc=${rc})"
  sed 's/^/       /' "${tmp}/out"
fi

# ── 6. A fuzz crate with no lock is a failure ─────────────────────────────────────────
# With no Cargo.lock committed, each build resolves afresh, which is the same drift.
root="${tmp}/unlocked"
make_fixture "$root" "demo" "fine" 'fn main() {}' yes
rm "${root}/crates/demo/fuzz/Cargo.lock"
run_gate "$root"
rc="$(cat "${tmp}/rc")"
if [ "$rc" = "1" ] && grep -F "its Cargo.lock is not what its manifest resolves to" "${tmp}/out" >/dev/null; then
  pass "a fuzz crate with no Cargo.lock fails the gate"
else
  fail "a fuzz crate with no Cargo.lock should fail the gate (rc=${rc})"
  sed 's/^/       /' "${tmp}/out"
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: ${failures} check-fuzz-compiles.sh assertion(s) failed"
  exit 1
fi
echo "OK: check-fuzz-compiles.sh can go red on a broken crate, an undeclared target, an empty tree,"
echo "    a stale lock and a missing one."
