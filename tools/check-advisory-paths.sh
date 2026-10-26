#!/usr/bin/env bash
# check-advisory-paths.sh — fail when the published exposure claim for an accepted
# advisory disagrees with the actual dependency graph.
#
# Why this exists: three of the eight acceptances in docs/dependency-risk-policy.md were
# carried by sentences that `cargo tree` contradicts — RUSTSEC-2023-0071 claimed `rsa`
# reached FraiseQL only through `sqlx-mysql` (removed with #374; it arrives via
# jsonwebtoken in the DEFAULT build, #1110), RUSTSEC-2025-0134 claimed dev-dependency-only,
# and RUSTSEC-2026-0204 claimed criterion dev-deps (#1137). Nothing checked, because
# check-audit-lockstep.sh compares advisory IDS and both machine-read files carried the
# same wrong story, while `cargo deny` has no opinion about whether a `reason` is true.
#
# This needs cargo, so it runs in the Dagger `security` leg rather than ShellGates.
#
# The claim lives in the policy table's `Exposure` column:
#
#   default-build        `cargo tree -i <crate>@<ver> -e normal` resolves it on default features
#   feature-gated:<f>    absent on default features; present with --features <f>
#   not-compiled         absent even with --all-features
#   feature-off:<f>      compiled, but the crate's own feature <f>, which holds the flaw,
#                        is enabled by nothing, even with --all-features
#
# `-e normal` excludes dev- and build-dependency edges, so "not in the shipped binary" is
# established by construction rather than asserted in prose.
#
# Overrides, for testing:
#   ADVISORY_PATHS_DOC=<path>   read this policy document instead of the default
set -euo pipefail

# ⚠ Do NOT `cd "$(git rev-parse --show-toplevel)"` unconditionally. This runs in the
# Dagger `security` leg, whose container mounts the source with `+ignore=[".git"]` and —
# unlike ShellGates — never runs `git init`. `git rev-parse` fails there, and under
# `set -e` the whole gate dies before checking anything. The sibling gate that already
# runs in this leg (check-crypto-providers.sh) simply relies on the container workdir.
# So: use the repo root when git can tell us, otherwise stay where we are — and let the
# missing-file check below fail loudly if that turns out to be the wrong place.
if repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root"
fi

doc_file="${ADVISORY_PATHS_DOC:-docs/dependency-risk-policy.md}"

if [ ! -f "$doc_file" ]; then
  echo "ERROR: policy document not found: $doc_file" >&2
  exit 1
fi

# resolves <spec> [extra cargo args…] → prints "present" | "absent" | "error:<detail>"
#
# ⚠ Exit codes alone do not answer this question, and neither does grepping for "error":
#
#   * a crate that is in Cargo.lock but out of the resolved graph exits **0** and prints
#     `warning: nothing to print.`  (this is what sqlx-mysql does)
#   * a crate that is not in the graph at all exits **101** with
#     `did not match any packages`
#   * an UNVERSIONED spec matching two versions also exits 101, with `is ambiguous` — a
#     naive gate reads that as "absent" and passes on precisely the advisory whose
#     non-default status it was supposed to prove. rustls-webpki resolves to both 0.101.7
#     and 0.103.13 in this workspace, so this is not hypothetical.
#   * cargo colours its diagnostics, so ANSI escapes precede the word `error`.
#
# So: discriminate on the tree's ROOT LINE (`<crate> v<version>`), and treat ambiguity as
# a gate error rather than as absence.
#
# ⚠ Never `printf "$out" | grep -q` (or `| head`) here: under `pipefail`, a reader that
# exits on its first match kills the writer with SIGPIPE once the output outgrows the pipe
# buffer, and the pipeline's 141 reads as "no match". Feed grep a here-string instead.
resolves() {
  local spec="$1"; shift
  local name="${spec%@*}"
  local out rc
  set +e
  out="$(cargo tree -i "$spec" -e normal "$@" 2>&1)"
  rc=$?
  set -e

  if grep -qE 'is ambiguous' <<< "$out"; then
    printf 'error:ambiguous spec %s — name the exact version' "$spec"
    return
  fi
  if [ "$rc" -eq 0 ] && grep -qE "^${name} v[0-9]" <<< "$out"; then
    printf 'present'
    return
  fi
  # Absent only on cargo's own words for it. Any other output without a root line is an
  # error, shown, never read as absence: absence is what a `not-compiled` claim passes on.
  if { [ "$rc" -eq 0 ] && grep -qE 'nothing to print' <<< "$out"; } \
    || grep -qE 'did not match any packages' <<< "$out"; then
    printf 'absent'
    return
  fi
  printf 'error:cargo tree exited %s for %s with no root line: %s' "$rc" "$spec" \
    "$(sed -n '1,5p' <<< "$out" | tr '\n' ' ')"
}

# enables <spec> <feature> → prints "on" | "off" | "error:<detail>": whether anything in the
# graph turns on the crate's own <feature>, with every workspace feature enabled. The crate
# must resolve at all, and must declare the feature, or "off" would be vacuous.
enables() {
  local spec="$1" feature="$2"
  local name="${spec%@*}"
  local present out rc
  present="$(resolves "$spec" --all-features)"
  case "$present" in
    present) ;;
    absent) printf 'error:%s is not compiled at all; claim not-compiled instead' "$spec"; return ;;
    *) printf '%s' "$present"; return ;;
  esac
  # A feature the crate does not declare is enabled by nothing, so a misspelt name would
  # read as "off". Read the crate's own manifest and require the feature in [features].
  # The version goes into an extended regex: `.` and the `+` of build metadata
  # (`2.1.2+zstd.1.5.7`) must match themselves.
  local version="${spec#*@}" manifest pattern
  pattern="$(printf '%s' "${name}-${version}" | sed -E 's/[.+]/\\&/g')"
  local metadata
  metadata="$(cargo metadata --format-version 1 --all-features 2>/dev/null || true)"
  manifest="$(grep -oE "\"manifest_path\":\"[^\"]*/${pattern}/Cargo\.toml\"" <<< "$metadata" \
    | sed -nE '1s/^"manifest_path":"(.*)"$/\1/p' || true)"
  if [ -z "$manifest" ] || [ ! -f "$manifest" ]; then
    printf 'error:no manifest found for %s' "$spec"
    return
  fi
  if ! awk -v f="$feature" '
      /^\[/ { in_features = ($0 == "[features]") ; next }
      in_features && $0 ~ "^\"?" f "\"?[ \t]*=" { found = 1 }
      END { exit !found }
    ' "$manifest"; then
    printf 'error:%s declares no feature %s' "$spec" "$feature"
    return
  fi
  # `{f}` prints the features enabled on each package; the first line is the inverted
  # crate itself: `<crate> v<version>|<f1>,<f2>,…`.
  local root
  set +e
  out="$(cargo tree -i "$spec" -e normal --all-features --color never -f '{p}|{f}' 2>&1)"
  rc=$?
  set -e
  root="$(grep -m 1 -E "^${name} v[0-9]" <<< "$out" || true)"
  if [ "$rc" -ne 0 ] || [ -z "$root" ]; then
    printf 'error:cargo tree exited %s for %s' "$rc" "$spec"
    return
  fi
  case ",${root#*|}," in
    *",${feature},"*) printf 'on' ;;
    *) printf 'off' ;;
  esac
}

status=0
checked=0

# Rows are `| RUSTSEC-… | `crate@version` | path | exposure | mitigation | date |`.
while IFS=$'\t' read -r id spec exposure; do
  [ -z "$id" ] && continue
  checked=$((checked + 1))

  case "$exposure" in
    default-build)
      got="$(resolves "$spec")"
      case "$got" in
        present) ;;
        absent)
          echo "ERROR: $id claims 'default-build' but $spec is absent from the default build."
          status=1 ;;
        error:*)
          echo "ERROR: $id — ${got#error:}"
          status=1 ;;
      esac
      ;;
    not-compiled)
      got="$(resolves "$spec" --all-features)"
      case "$got" in
        absent) ;;
        present)
          echo "ERROR: $id claims 'not-compiled' but $spec resolves with --all-features."
          status=1 ;;
        error:*)
          echo "ERROR: $id — ${got#error:}"
          status=1 ;;
      esac
      ;;
    feature-gated:*)
      feature="${exposure#feature-gated:}"
      if [ -z "$feature" ]; then
        echo "ERROR: $id — 'feature-gated:' with no feature named."
        status=1
        continue
      fi
      got_default="$(resolves "$spec")"
      case "$got_default" in
        error:*)
          echo "ERROR: $id — ${got_default#error:}"
          status=1
          continue ;;
        present)
          echo "ERROR: $id claims 'feature-gated:$feature' but $spec is in the DEFAULT build."
          status=1
          continue ;;
      esac
      # The feature is declared on fraiseql-server for every advisory here; the gate
      # enables it there rather than guessing an owner from the feature name.
      got_feature="$(resolves "$spec" --features "fraiseql-server/$feature")"
      case "$got_feature" in
        present) ;;
        absent)
          echo "ERROR: $id claims 'feature-gated:$feature' but $spec does not appear with that feature enabled."
          echo "       Either the feature name is wrong or the acceptance is for a crate nothing pulls."
          status=1 ;;
        error:*)
          echo "ERROR: $id — ${got_feature#error:}"
          status=1 ;;
      esac
      ;;
    feature-off:*)
      feature="${exposure#feature-off:}"
      if [ -z "$feature" ]; then
        echo "ERROR: $id — 'feature-off:' with no feature named."
        status=1
        continue
      fi
      got="$(enables "$spec" "$feature")"
      case "$got" in
        off) ;;
        on)
          echo "ERROR: $id claims 'feature-off:$feature' but $spec has '$feature' enabled with --all-features."
          status=1 ;;
        error:*)
          echo "ERROR: $id — ${got#error:}"
          status=1 ;;
      esac
      ;;
    *)
      echo "ERROR: $id has an unrecognised Exposure value '$exposure'."
      echo "       Expected: default-build | feature-gated:<feature> | not-compiled | feature-off:<feature>"
      status=1
      ;;
  esac
done < <(awk -F'|' '
  /^\| *RUSTSEC-[0-9]{4}-[0-9]{4,}/ {
    id = $2; gsub(/[^A-Z0-9-]/, "", id)
    spec = $3; gsub(/[` \t]/, "", spec)
    # Named `exposure`, not `exp`: exp is an awk builtin and assigning to it is a
    # syntax error. No apostrophes in this block — it is inside a single-quoted string.
    exposure = $5; gsub(/[` \t]/, "", exposure)
    print id "\t" spec "\t" exposure
  }
' "$doc_file")

if [ "$checked" -eq 0 ]; then
  # A parser that silently matches nothing reports success for an empty check — the
  # fabricated-success shape this whole family of gates exists to close.
  echo "ERROR: no accepted-advisory rows found in $doc_file — the table format changed" >&2
  exit 1
fi

if [ "$status" -eq 0 ]; then
  echo "OK: all $checked published advisory exposure claims match the dependency graph."
fi
exit "$status"
