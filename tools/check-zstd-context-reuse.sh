#!/usr/bin/env bash
# check-zstd-context-reuse.sh — fail if anything we build could reach RUSTSEC-2026-0391.
#
# zstd before 0.14.3 let a stream encoder or decoder built with `with_context` reuse a
# borrowed context as it was, so an abandoned stream's buffered input or undelivered output
# leaked into the next one. The fix needs zstd 0.14, which our zstd consumers do not accept
# yet (#1747), so the advisory is accepted in deny.toml on one condition: no code we compile
# calls those constructors. This gate is that condition. If it fails, the acceptance is void.
#
# It scans, for `Encoder::with_context` / `Decoder::with_context` (any generics) and for a
# zstd `Encoder`/`Decoder` imported under an alias, which would hide such a call:
#
#   * the library sources (`src/`, without `src/bin/`) of every crate that depends on zstd
#     directly, read from the dependency graph rather than listed here, so a new consumer
#     is covered the day it arrives;
#   * every workspace crate.
#
# A bare `with_context` cannot be the pattern: anyhow's `Context::with_context` is used in
# the workspace and in wasmtime's cache configuration.
#
# Needs cargo, so it runs in the Dagger `security` leg.
#
# Overrides, for testing:
#   ZSTD_CONTEXT_SCAN_DIRS=<dir>[:<dir>…]   scan these directories instead of the consumers
set -euo pipefail

# Like check-advisory-paths.sh: the security leg mounts the source without `.git`.
if repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root"
fi

calls='(Encoder|Decoder)(<[^>]*>)?::with_context'
aliases='use [^;]*zstd[^;]*(Encoder|Decoder)[^;]* as '

dirs=()
if [ -n "${ZSTD_CONTEXT_SCAN_DIRS:-}" ]; then
  IFS=':' read -r -a dirs <<< "$ZSTD_CONTEXT_SCAN_DIRS"
else
  # stderr stays out of the parsed text: cargo prints warnings there (a shadowed alias,
  # for one), and a warning line would be read as a crate.
  set +e
  tree="$(cargo tree -i zstd -e normal --all-features --depth 1 --prefix none --color never 2>/dev/null)"
  rc=$?
  set -e
  if [ "$rc" -ne 0 ]; then
    echo "ERROR: cargo tree -i zstd exited $rc. Two zstd versions? Name the one to scan." >&2
    exit 1
  fi
  metadata="$(cargo metadata --format-version 1 --all-features --color never)"
  # Every line but the root is a direct dependent: `<crate> v<version>[ (<path>)]`.
  while read -r name version _; do
    [ "$name" = zstd ] && continue
    version="${version#v}"
    # A workspace member is covered by the workspace scan below.
    [ -d "crates/$name" ] && continue
    # A here-string, not `printf | grep | head`: under pipefail an early-exiting reader
    # SIGPIPEs the writer and the 141 reads as "not found".
    manifest="$(grep -oE "\"manifest_path\":\"[^\"]*/${name}-${version//./\\.}/Cargo\\.toml\"" <<< "$metadata" \
      | sed -nE '1s/^"manifest_path":"(.*)"$/\1/p' || true)"
    if [ -z "$manifest" ] || [ ! -d "$(dirname "$manifest")/src" ]; then
      echo "ERROR: no source found for zstd consumer $name $version" >&2
      exit 1
    fi
    dirs+=("$(dirname "$manifest")/src")
  done < <(grep -E '^[a-z0-9_-]+ v[0-9]' <<< "$tree")
  dirs+=("crates")
fi

if [ "${#dirs[@]}" -eq 0 ]; then
  echo "ERROR: nothing to scan — the zstd consumer lookup matched no crate" >&2
  exit 1
fi

status=0
for dir in "${dirs[@]}"; do
  hits="$(grep -rnE --include='*.rs' "$calls|$aliases" "$dir" | grep -v '/src/bin/' || true)"
  if [ -n "$hits" ]; then
    echo "ERROR: zstd stream context reuse is reachable (RUSTSEC-2026-0391) in $dir:"
    printf '%s\n' "$hits"
    status=1
  fi
done

if [ "$status" -eq 0 ]; then
  echo "OK: no zstd \`with_context\` constructor in ${#dirs[@]} scanned source trees."
fi
exit "$status"
