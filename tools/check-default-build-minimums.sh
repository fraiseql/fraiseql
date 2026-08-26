#!/usr/bin/env bash
# check-default-build-minimums.sh — fail when a crate in the DEFAULT build is older than a
# minimum version we have committed to keeping.
#
# Why this exists: cargo-deny cannot scope an advisory ignore to one crate version.
# `[advisories] ignore` accepts only `id` and `reason` (a `crate = "…"` key there is a
# YANKED-crate ignore and suppresses no vulnerability at all — verified against cargo-deny
# 0.19.0, it leaves `advisories FAILED`). So when an advisory matches two instances of a
# crate and only one of them is acceptable, ignoring it by id silences BOTH.
#
# RUSTSEC-2026-0258 was that case. `h2` appeared twice:
#
#   h2 0.3.27  ← aws-smithy-http-client (hyper 0.14) ← aws-config   opt-in aws-* only,
#              no fix exists in the 0.3 series → accepted in deny.toml with a deadline
#   h2 0.4.15  ← hyper 1.10 ← axum ← fraiseql-server                THE GRAPHQL LISTENER;
#              `axum::serve` speaks HTTP/2 by prior knowledge, so the unbounded empty-DATA-
#              frame DoS was remotely triggerable → FIXED by bumping to 0.4.16
#
# ⚠ THAT ACCEPTANCE IS GONE (#1111): the aws-* stack no longer resolves hyper 0.14, so
# h2 0.3.27 has left the graph and deny.toml ignores RUSTSEC-2026-0258 no longer. The floor
# below is therefore no longer COMPENSATING for an over-broad ignore — `cargo deny check
# advisories` would now catch a regression directly. It is kept anyway, on its own merits:
# the floor is on the shipped GraphQL listener, it was verified load-bearing when it
# mattered, and re-deriving it after the next such acceptance is more expensive than
# holding it. Do not read the paragraph above as a live constraint; read it as why the
# mechanism exists.
#
# Scope note: this is deliberately about the DEFAULT feature set with normal edges — what a
# default deployment actually compiles and ships. A crate that appears only behind an opt-in
# feature is out of scope here; its exposure claim is checked by check-advisory-paths.sh.
#
# This needs cargo, so it runs in the Dagger `security` leg rather than ShellGates.
#
# Overrides, for testing:
#   DEFAULT_BUILD_MINIMUMS="crate@version[ crate@version…]"   check these instead
set -euo pipefail

# ⚠ Do NOT `cd "$(git rev-parse --show-toplevel)"` unconditionally — the Dagger `security`
# leg mounts the source with `+ignore=[".git"]` and never runs `git init`, so `git rev-parse`
# fails there and `set -e` would kill the gate before it checked anything. Same guard as
# check-advisory-paths.sh: use the repo root when git can tell us, otherwise stay put.
if repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root"
fi

# crate@minimum-version, enforced on the default feature set.
#
# Every entry needs a comment naming WHY the floor exists, so a future reader can tell a
# live constraint from a stale one and knows what to re-check before removing it.
DEFAULT_MINIMUMS=(
  # RUSTSEC-2026-0258 (unbounded empty DATA frames, GHSA-q83h-524g-xf6h) on the GraphQL
  # listener. The deny.toml acceptance this once compensated for is gone (#1111); the floor
  # stands on its own — `axum::serve` speaks HTTP/2 by prior knowledge, so an h2 below
  # 0.4.16 in the default build is remotely triggerable.
  "h2@0.4.16"
)

if [ -n "${DEFAULT_BUILD_MINIMUMS:-}" ]; then
  # shellcheck disable=SC2206  # deliberate word-splitting: the override is a space-separated list
  DEFAULT_MINIMUMS=(${DEFAULT_BUILD_MINIMUMS})
fi

# Print the version of $1 as resolved in the default build with normal edges, or an
# `error:<detail>` / `absent` marker.
#
# ⚠ Exit codes alone do not answer this, and neither does grepping for "error" — cargo
# colours its diagnostics, so ANSI escapes precede the word. Discriminate on the tree's ROOT
# LINE (`<crate> v<version>`), exactly as check-advisory-paths.sh does. An UNVERSIONED spec
# that matches two resolved versions exits 101 with "is ambiguous"; that is a gate error, not
# an absence — reading it as absence is how a gate passes on the very thing it guards.
#
# ⚠ No `| grep -q` here. Under `pipefail` a `grep -q` that matches early can leave the
# producer dead of SIGPIPE (141), turning a SUCCESSFUL match into a failed pipeline. Match
# with bash pattern tests against a captured string instead.
resolved_version() {
  local name="$1" out rc line
  set +e
  out="$(cargo tree -i "$name" -e normal 2>&1)"
  rc=$?
  set -e

  if [[ "$out" == *"is ambiguous"* ]]; then
    printf 'error:%s resolves to multiple versions in the default build — this gate cannot tell which one ships' "$name"
    return
  fi
  if [ "$rc" -ne 0 ]; then
    if [[ "$out" == *"did not match any packages"* ]]; then
      printf 'absent'
    else
      printf 'error:cargo tree exited %s for %s' "$rc" "$name"
    fi
    return
  fi

  # Root line is the first line matching `<name> v<version>`; cargo may print warnings first.
  while IFS= read -r line; do
    if [[ "$line" == "$name v"[0-9]* ]]; then
      line="${line#"$name" v}"
      printf '%s' "${line%% *}"
      return
    fi
  done <<<"$out"

  printf 'error:no root line for %s in cargo tree output' "$name"
}

# 0 when $1 >= $2, using version ordering rather than string ordering.
version_ge() {
  local got="$1" min="$2" first
  [ "$got" = "$min" ] && return 0
  # No `head -1`: an early-exiting reader can SIGPIPE the producer under pipefail.
  first="$(printf '%s\n%s\n' "$got" "$min" | sort -V)"
  first="${first%%$'\n'*}"
  [ "$first" = "$min" ]
}

status=0
checked=0

for entry in "${DEFAULT_MINIMUMS[@]}"; do
  name="${entry%@*}"
  min="${entry##*@}"

  if [ "$name" = "$entry" ] || [ -z "$min" ]; then
    echo "ERROR: malformed minimum '$entry' — expected 'crate@version'." >&2
    status=1
    continue
  fi

  checked=$((checked + 1))
  got="$(resolved_version "$name")"

  case "$got" in
    error:*)
      echo "ERROR: $name — ${got#error:}" >&2
      status=1
      ;;
    absent)
      # A floor on a crate that is no longer in the default build is stale, not satisfied.
      # Failing loudly is the point: silently passing would let the entry rot until the
      # crate returns and the floor is quietly not enforced.
      echo "ERROR: $name is not in the default build, but a minimum of $min is declared." >&2
      echo "       Either the dependency was removed — drop the entry from DEFAULT_MINIMUMS" >&2
      echo "       and re-check whatever advisory acceptance depends on it — or it moved" >&2
      echo "       behind a feature, in which case check-advisory-paths.sh owns it now." >&2
      status=1
      ;;
    *)
      if version_ge "$got" "$min"; then
        echo "OK: $name $got in the default build (minimum $min)."
      else
        echo "ERROR: $name $got is in the DEFAULT build, below the required minimum $min." >&2
        echo "       This floor exists because deny.toml accepts an advisory for a DIFFERENT," >&2
        echo "       non-default instance of this crate, and cargo-deny cannot scope an ignore" >&2
        echo "       to one version — so cargo-deny will NOT catch this. Bump it:" >&2
        echo "         cargo update -p $name@$got --precise $min" >&2
        status=1
      fi
      ;;
  esac
done

if [ "$checked" -eq 0 ] && [ "$status" -eq 0 ]; then
  echo "ERROR: no minimums checked — DEFAULT_MINIMUMS is empty or every entry was malformed." >&2
  exit 1
fi

if [ "$status" -eq 0 ]; then
  echo "OK: all $checked default-build minimum(s) satisfied."
fi

exit "$status"
