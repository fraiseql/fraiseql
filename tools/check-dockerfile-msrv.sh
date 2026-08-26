#!/usr/bin/env bash
# check-dockerfile-msrv.sh — fail when a Dockerfile's Rust base image is older than the
# workspace MSRV.
#
# Background (#1107). The release `Dockerfile` pinned `rust:1.92-slim` as its builder
# while `[workspace.package] rust-version` was `1.94.1` (#933). Nothing coupled the two,
# so the pin rotted silently and the release image could not have been built at all:
#
#     $ cargo +1.92 check -p fraiseql-server
#     error: rustc 1.92.0 is not supported by the following packages:
#       fraiseql-core@… requires rustc 1.94.1  … (every workspace crate)
#
# It stayed invisible because `docker-build.yml` triggers only on `push: tags` and
# workflow_dispatch — no push and no PR builds this file, so the breakage would have
# surfaced at the next release tag, in the release. Same class as #1129 (the OCI version
# label) and the reason this gate is a sibling of check-deploy-versions.sh.
#
# FLOATING TAGS ARE DELIBERATELY ALLOWED. `rust:latest`, `rust:1` and `rust:1-slim…`
# track the newest release and cannot be older than the MSRV; flagging them would make
# this gate wrong on the majority of the tree and get it disabled.
#
# A two-component tag (`rust:1.94`) is a floating PATCH — Docker resolves it to the
# newest 1.94.x — so it is compared on major.minor only. Cargo.toml says as much about
# this exact tag: "Docker's rust:1.94 tracks the newest patch, so a 1.94.0 floor was a
# claim no leg ever tested."
#
# Pure grep/sed/sort -V, no toolchain, no git history → Dagger ShellGates.
#
# Overrides, for testing:
#   DOCKERFILE_MSRV_ROOT=<dir>   treat this directory as the repo root
set -euo pipefail

if [ -n "${DOCKERFILE_MSRV_ROOT:-}" ]; then
  cd "$DOCKERFILE_MSRV_ROOT"
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root"
fi

if [ ! -f Cargo.toml ]; then
  echo "ERROR: check-dockerfile-msrv: no Cargo.toml at $(pwd)" >&2
  exit 1
fi

# `rust-version = "1.94.1"  # MSRV — …` : take the quoted value, not the trailing prose.
msrv="$(sed -nE 's/^rust-version[[:space:]]*=[[:space:]]*"([0-9]+(\.[0-9]+)*)".*/\1/p' Cargo.toml | head -1)"
if [ -z "$msrv" ]; then
  echo "ERROR: check-dockerfile-msrv: could not read rust-version from Cargo.toml." >&2
  exit 1
fi
msrv_mm="$(echo "$msrv" | cut -d. -f1,2)"

# ⚠ `git ls-files` returns NOTHING under Dagger ShellGates (the leg ignores .git and runs
# `git init -q .`), so discovery is `find`. A gate that silently scans an empty file list
# reads as passing — three gates here shipped that way (#1075).
mapfile -t dockerfiles < <(
  find . \( -name 'Dockerfile' -o -name 'Dockerfile.*' -o -name '*.Dockerfile' \) \
       -not -path './target/*' -not -path './.git/*' -not -path '*/node_modules/*' \
       -not -path '*/vendor/*' -type f | sort
)

if [ "${#dockerfiles[@]}" -eq 0 ]; then
  echo "✗ check-dockerfile-msrv: found NO Dockerfile to check." >&2
  echo "  This gate cannot pass vacuously — the scan is broken, or every Dockerfile" >&2
  echo "  was removed. Both need a human." >&2
  exit 1
fi

fail=0
pinned=0
floating=0

# ver_ge <a> <b> — true when a >= b, comparing as versions (sort -V), not as strings.
ver_ge() {
  [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -1)" = "$2" ]
}

for df in "${dockerfiles[@]}"; do
  # FROM [--platform=…] rust:<tag> [AS name]. `as` is case-insensitive in Dockerfiles and
  # both spellings occur in this tree.
  while IFS= read -r entry; do
    [ -z "$entry" ] && continue
    lineno="${entry%%:*}"
    tag="${entry#*:}"

    # An ARG-indirected tag cannot be resolved by inspection. None exist today; if one
    # appears, fail rather than skip — an unresolvable pin that reads as "allowed" is
    # the exact silent fail-open this gate exists to close.
    case "$tag" in
      *'$'*)
        echo "✗ ${df#./}:$lineno — Rust base tag is ARG-indirected ('rust:$tag')." >&2
        echo "    This gate cannot resolve it, and will not pass it silently." >&2
        echo "    Inline the version, or teach this gate to resolve the ARG." >&2
        fail=1
        continue
        ;;
    esac

    # Leading numeric component(s) of the tag: 1.94.1-slim → 1.94.1, 1-slim-bookworm → 1,
    # latest/nightly/bookworm → (empty).
    ver="$(echo "$tag" | sed -nE 's/^([0-9]+(\.[0-9]+){0,2}).*/\1/p')"

    if [ -z "$ver" ]; then
      floating=$((floating + 1))
      echo "  – ${df#./}:$lineno — rust:$tag (floating, tracks newest — allowed)"
      continue
    fi

    dots="$(echo "$ver" | tr -cd '.' | wc -c)"
    case "$dots" in
      0)
        # `rust:1` floats across every 1.x release; it cannot be older than the MSRV.
        floating=$((floating + 1))
        echo "  – ${df#./}:$lineno — rust:$tag (floating major, allowed)"
        continue
        ;;
      1)
        # `rust:1.94` floats the patch — compare major.minor only.
        pinned=$((pinned + 1))
        if ver_ge "$ver" "$msrv_mm"; then
          echo "  ✓ ${df#./}:$lineno — rust:$tag ≥ MSRV $msrv (floating patch)"
        else
          echo "✗ ${df#./}:$lineno — Rust base rust:$tag is older than the workspace MSRV $msrv." >&2
          fail=1
        fi
        ;;
      *)
        pinned=$((pinned + 1))
        if ver_ge "$ver" "$msrv"; then
          echo "  ✓ ${df#./}:$lineno — rust:$tag ≥ MSRV $msrv"
        else
          echo "✗ ${df#./}:$lineno — Rust base rust:$tag is older than the workspace MSRV $msrv." >&2
          fail=1
        fi
        ;;
    esac
  done < <(grep -nE '^FROM[[:space:]]+(--[^[:space:]]+[[:space:]]+)*rust:' "$df" \
            | sed -E 's/^([0-9]+):FROM[[:space:]]+(--[^[:space:]]+[[:space:]]+)*rust:([^[:space:]]+).*/\1:\3/')
done

if [ "$fail" -ne 0 ]; then
  echo "" >&2
  echo "  A Rust base older than rust-version cannot build this workspace at all, and" >&2
  echo "  docker-build.yml is tag-only — so the first witness would be the release." >&2
  exit 1
fi

echo "OK: ${#dockerfiles[@]} Dockerfile(s) scanned; $pinned pinned Rust base(s) ≥ MSRV $msrv, $floating floating."
