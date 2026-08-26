#!/usr/bin/env bash
# check-dockerfile-workspace-members.sh — every Cargo workspace member must be present
# in the release Dockerfile's builder stage.
#
# Background. `cargo build -p fraiseql-server` loads the WHOLE workspace manifest before
# it builds anything, so a member directory that was never COPYed into the builder is a
# hard failure at the first cargo invocation:
#
#     error: failed to load manifest for workspace member `/build/examples/authentication`
#     referenced by workspace at `/build/Cargo.toml`
#
# It is not a partial build or a missing feature — the image cannot be built at all.
#
# This happened the moment six runnable example crates joined `[workspace] members` while
# the Dockerfile still copied only `crates` and `deploy`. Nothing reported it: as #1107
# records for the sibling failure, `docker-build.yml` triggers only on `push: tags` and
# workflow_dispatch, and `release-smoke.yml` builds with `cargo build`, not this file.
# The first witness to a broken release image is the release.
#
# ⚠ Manifests alone are enough for cargo — a member's src/ need not be present for
# `-p fraiseql-server` to resolve. This gate deliberately does NOT accept that: a
# manifest-only COPY is a contract nobody can see, and the next example added would
# break the image again. Whole member directories, or a parent of them.
#
# Pure grep/sed, no toolchain, no git history → Dagger ShellGates.
#
# Overrides, for testing:
#   DOCKERFILE_MEMBERS_ROOT=<dir>
set -euo pipefail

if [ -n "${DOCKERFILE_MEMBERS_ROOT:-}" ]; then
  cd "$DOCKERFILE_MEMBERS_ROOT"
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root"
fi

for f in Cargo.toml Dockerfile; do
  if [ ! -f "$f" ]; then
    echo "ERROR: check-dockerfile-workspace-members: scan target not found: $f" >&2
    exit 1
  fi
done

# ── Workspace members ────────────────────────────────────────────────────────────
# The quoted paths inside `members = [ … ]`. Trailing `# comments` in that array are
# common here, so the quoted strings are extracted rather than the lines.
members="$(sed -n '/^members[[:space:]]*=[[:space:]]*\[/,/^]/p' Cargo.toml \
           | grep -oE '"[^"]+"' | tr -d '"' || true)"

if [ -z "$members" ]; then
  echo "✗ check-dockerfile-workspace-members: read NO workspace members from Cargo.toml." >&2
  echo "  This gate cannot pass vacuously — the parse is broken, or the workspace is." >&2
  exit 1
fi

# ── The builder stage's COPY sources ─────────────────────────────────────────────
# The stage runs from the first `FROM … AS builder` to the next `FROM`. `COPY --from=…`
# copies between stages, not from the build context, so those lines are excluded.
builder="$(awk '
  /^FROM/ {
    if (started) exit
    if (tolower($0) ~ / as builder/) { started = 1 }
    next
  }
  started { print }
' Dockerfile)"

copy_sources="$(echo "$builder" \
  | grep -E '^COPY' \
  | grep -v -- '--from=' \
  | sed -E 's/^COPY[[:space:]]+//; s/--[a-z]+=[^[:space:]]+[[:space:]]+//g' \
  | awk '{ for (i = 1; i < NF; i++) print $i }' \
  | sed 's#^\./##; s#/$##' || true)"

if [ -z "$copy_sources" ]; then
  echo "✗ check-dockerfile-workspace-members: found NO context COPY in the builder stage." >&2
  echo "  This gate cannot pass vacuously — the stage parse is broken." >&2
  exit 1
fi

# ── Anything .dockerignore drops never reaches the COPY ──────────────────────────
# A member listed in a COPY source but excluded by .dockerignore is copied as nothing,
# which is the same failure with none of the evidence.
ignored=""
if [ -f .dockerignore ]; then
  ignored="$(grep -vE '^[[:space:]]*(#|$)' .dockerignore | sed 's#/$##; s#^\./##' || true)"
fi

fail=0
for member in $members; do
  root="${member%%/*}"
  covered=0
  for src in $copy_sources; do
    if [ "$src" = "$member" ] || [ "$src" = "$root" ] || [ "$src" = "." ]; then
      covered=1
      break
    fi
  done

  if [ "$covered" -ne 1 ]; then
    echo "✗ workspace member '$member' is never COPYed into the Dockerfile builder stage." >&2
    echo "    cargo loads the whole workspace manifest before building anything, so this" >&2
    echo "    is not a partial build — the release image cannot be built at all." >&2
    echo "    Add:  COPY $root ./$root" >&2
    fail=1
    continue
  fi

  for ig in $ignored; do
    if [ "$ig" = "$root" ] || [ "$ig" = "$member" ]; then
      echo "✗ workspace member '$member' is COPYed but excluded by .dockerignore ('$ig')." >&2
      echo "    The COPY succeeds and copies nothing; cargo then fails to load the member." >&2
      fail=1
      break
    fi
  done
done

if [ "$fail" -ne 0 ]; then
  echo "" >&2
  echo "  This is the #1205 shape. It is now caught twice: here statically, and by" >&2
  echo "  \`dagger call images\`, which builds this Dockerfile before the tag." >&2
  echo "  Verified: with this COPY removed, the build fails at the cargo step with" >&2
  echo "  'failed to load manifest for workspace member /build/examples/...'." >&2
  exit 1
fi

count="$(echo "$members" | wc -l | tr -d ' ')"
echo "OK: all $count workspace member(s) reach the Dockerfile builder stage."
