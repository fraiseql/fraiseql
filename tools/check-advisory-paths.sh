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
resolves() {
  local spec="$1"; shift
  local name="${spec%@*}"
  local out rc
  set +e
  out="$(cargo tree -i "$spec" -e normal "$@" 2>&1)"
  rc=$?
  set -e

  if printf '%s\n' "$out" | grep -qE 'is ambiguous'; then
    printf 'error:ambiguous spec %s — name the exact version' "$spec"
    return
  fi
  if [ "$rc" -eq 0 ] && printf '%s\n' "$out" | grep -qE "^${name} v[0-9]"; then
    printf 'present'
    return
  fi
  if [ "$rc" -eq 0 ] || printf '%s\n' "$out" | grep -qE 'did not match any packages'; then
    printf 'absent'
    return
  fi
  printf 'error:cargo tree exited %s for %s' "$rc" "$spec"
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
    *)
      echo "ERROR: $id has an unrecognised Exposure value '$exposure'."
      echo "       Expected: default-build | feature-gated:<feature> | not-compiled"
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
