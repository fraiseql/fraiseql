#!/usr/bin/env bash
# check-compose-references.sh — fail when a tracked file names a Compose file that is not
# in the repository.
#
# Background (#1219): `make federation-up`, `make federation-down` and `make
# test-federation` all drove `docker/federation-ci/docker-compose.yml`. That path is in no
# commit reachable from `dev`, so all three failed on their first line — while `make help`
# and `docs/testing-matrix.md` advertised them as the way to run the federation suite, and
# `docs/testing.md` gave a second, different dead path
# (`docker/docker-compose.federation.yml`). Nothing could have caught it:
# `check-examples-integrity.sh` discovers compose files with `find` and checks what is
# *inside* them, so a compose file that is merely *named* and does not exist is outside
# every check in the repository.
#
# The rule is one-directional on purpose: every `-f <path>` a tracked file hands to
# `docker compose` must resolve. A compose file on disk that nothing names is fine.
#
# ⚠ Paths are resolved from the repository root, which is where every `docker compose -f`
# in this repo is invoked from. A reference under a `cd` into another directory would be
# resolved wrongly — there are none today, and `${...}` interpolation is skipped rather
# than guessed at.
#
# Pure bash, no toolchain, no git history → Dagger ShellGates.
#
# Overrides, for testing:
#   COMPOSE_REFS_ROOT=<dir>   scan this directory instead of the repository root
set -uo pipefail

if [ -n "${COMPOSE_REFS_ROOT:-}" ]; then
  cd "$COMPOSE_REFS_ROOT" || exit 1
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root" || exit 1
fi

status=0
checked=0
skipped=0

# Where a `docker compose -f` can plausibly live. Kept explicit rather than scanning the
# whole tree so a path inside a fenced code block in a vendored file cannot fail the gate.
targets=(Makefile)
for f in docs/*.md docs/**/*.md .github/workflows/*.yml tools/*.sh; do
  [ -f "$f" ] && targets+=("$f")
done

for file in "${targets[@]}"; do
  # `docker compose -f X` and `docker-compose -f X`, one or more times on a line.
  while read -r ref; do
    [ -z "$ref" ] && continue
    case "$ref" in
      *'$'*|*'{{'*)
        skipped=$((skipped + 1))
        continue
        ;;
    esac
    checked=$((checked + 1))
    if [ ! -f "$ref" ]; then
      echo "ERROR: ${file} names a Compose file that does not exist: ${ref}"
      status=1
    fi
  # Comment lines are dropped first. A comment naming a path is prose about history —
  # `check-examples-integrity.sh` explains #1052 by quoting the `make demo-start` command
  # whose compose file was deleted with it — and this gate's own header quotes the form it
  # greps for. Neither is an entry point anybody can run.
  done < <(grep -vE '^[[:space:]]*#' "$file" 2>/dev/null \
             | grep -oE 'docker[- ]compose[^|;&]*-f[[:space:]]+[^[:space:]"'"'"']+' 2>/dev/null \
             | sed -E 's/.*-f[[:space:]]+//')
done

if [ "$checked" -eq 0 ]; then
  # A grep that matches nothing would pass forever — the fabricated-success shape this
  # gate exists to catch. This repository always has at least one.
  echo "ERROR: no 'docker compose -f' references found in any scanned file." >&2
  echo "       The invocation spelling changed and this check went blind." >&2
  exit 1
fi

if [ "$status" -eq 0 ]; then
  echo "OK: all ${checked} Compose file reference(s) resolve (${skipped} interpolated, skipped)."
fi
exit "$status"
