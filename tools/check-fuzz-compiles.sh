#!/usr/bin/env bash
# check-fuzz-compiles.sh — every fuzz crate compiles, and every fuzz target on disk is
# a `[[bin]]` the compiler actually reaches.
#
# Background (#1254): `570baf9b1` changed `WhereClause::from_graphql_json`'s second
# parameter and left two `fraiseql-db` fuzz targets calling the old signature. They sat
# at `error[E0308]` for ten days, through two scheduled fuzz runs, because **nothing in
# the repository compiles a fuzz crate**:
#
#   * each `crates/*/fuzz` declares its own `[workspace]`, so it is outside the main
#     workspace — `cargo check --workspace`, `cargo clippy --all-targets` and every test
#     leg skip them by construction;
#   * `check-fuzz-targets.sh` is existence-only *by design* (pure bash, no toolchain, so
#     it can run in the Dagger ShellGates container) and can never see a type error;
#   * `fuzz.yml` is the only thing that builds them, and it is a weekly schedule whose
#     failures notify nobody — the same blindness that let #1128 keep twelve runs red.
#
# Whether a target *compiles* is deterministic, so it is a merge gate. Whether a
# campaign *finds a crash* is stochastic, and that is what `fuzz.yml`'s "not a merge
# gate" header is about; the two are not the same claim.
#
# The disk → manifest direction is checked here and not in `check-fuzz-targets.sh`
# because it is this gate's own blind spot: `cargo check` compiles the `[[bin]]`
# entries, so a target file with no `[[bin]]` would be skipped silently and this gate
# would report OK over it. That is not hypothetical — `fraiseql-wire`'s
# `connection_string.rs` was written in `b5e6be373`, never given a `[[bin]]`, and had
# therefore never been compiled or fuzzed once in its life.
#
# ⚠ This runs `cargo`, so it belongs to the Rust tier (Makefile `check-fuzz`, Dagger
# `CheckFuzz`) — NOT to ShellGates, which has no toolchain.
#
# Overrides, for testing:
#   FUZZ_COMPILE_ROOT=<dir>   resolve crates/ under this directory
#   FUZZ_COMPILE_NO_CARGO=1   run the manifest/disk checks only, skipping cargo
set -uo pipefail

if [ -n "${FUZZ_COMPILE_ROOT:-}" ]; then
  cd "$FUZZ_COMPILE_ROOT" || exit 1
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root" || exit 1
fi

status=0
discovered=0
checked=0

# A separate target directory: the fuzz crates resolve features independently of the
# workspace, and sharing `target/` would make every `make preflight` invalidate the
# workspace's own cached artifacts. It lives under target/ so .gitignore already covers it.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}/fuzz-check"

shopt -s nullglob
manifests=(crates/*/fuzz/Cargo.toml)
shopt -u nullglob

if [ "${#manifests[@]}" -eq 0 ]; then
  # Discovering nothing is the fabricated-success shape this gate exists to prevent:
  # a glob that matches no manifest would report OK over every fuzz crate in the tree.
  echo "ERROR: no fuzz manifests found under crates/*/fuzz/ — the layout changed and" >&2
  echo "       this check went blind." >&2
  exit 1
fi

for manifest in "${manifests[@]}"; do
  discovered=$((discovered + 1))
  fuzz_dir="$(dirname "$manifest")"
  crate="$(basename "$(dirname "$fuzz_dir")")"

  # Disk → manifest. `path = "fuzz_targets/<name>.rs"` is the spelling cargo-fuzz
  # normalises every [[bin]] to, and matching on it needs no TOML parser.
  shopt -s nullglob
  sources=("$fuzz_dir"/fuzz_targets/*.rs)
  shopt -u nullglob
  for source in "${sources[@]}"; do
    rel="fuzz_targets/$(basename "$source")"
    if ! grep -F "path = \"${rel}\"" "$manifest" >/dev/null; then
      echo "ERROR: ${crate} — ${rel} has no [[bin]] in ${manifest}."
      echo "       cargo never compiles it and cargo fuzz cannot run it, so the target"
      echo "       is dead code that reads as coverage."
      status=1
    fi
  done

  if [ -n "${FUZZ_COMPILE_NO_CARGO:-}" ]; then
    checked=$((checked + 1))
    continue
  fi

  # --keep-going: without it cargo stops scheduling after the first failing unit, so a
  # crate with several broken targets reports one and the next run reports the next.
  echo "=== cargo check ${manifest} ==="
  cargo check --keep-going --manifest-path "$manifest"
  rc=$?
  checked=$((checked + 1))
  if [ "$rc" -ne 0 ]; then
    echo "ERROR: ${crate} — the fuzz crate does not compile (cargo exit ${rc})."
    status=1
  fi
done

if [ "$checked" -ne "$discovered" ]; then
  echo "ERROR: checked ${checked} of ${discovered} fuzz crates — the loop skipped one." >&2
  exit 1
fi

if [ "$status" -eq 0 ]; then
  echo "OK: all ${checked} fuzz crates compile, and every fuzz target on disk is a [[bin]]."
fi
exit "$status"
