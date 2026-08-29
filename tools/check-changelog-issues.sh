#!/usr/bin/env bash
# check-changelog-issues.sh — fail when an issue closed since the last release is
# not documented in CHANGELOG.md, and when the changelog's heading structure drifts.
#
# Why this exists: at the v2.15.0 scoping pass, 258 issues had been closed by
# `Closes #N` since v2.14.1 and **48 appeared nowhere in CHANGELOG.md** — including
# an entire security wave whose bullets were production fail-closed boot changes
# (#1127). Nothing had ever checked: `.github/workflows/changelog-check.yml` lost
# its `pull_request` trigger in the Dagger migration and sat `workflow_dispatch:`-only,
# so no push and no PR ever ran a completeness check.
#
# ⚠ THIS GATE NEEDS REAL GIT HISTORY, so it must NOT be added to the Dagger
# `ShellGates` list: `.dagger/main.go` declares `+ignore=[".git"]` and runs
# `git init -q .` in the container, where this script would find zero `Closes #N`
# and pass vacuously — the fabricated-success shape it exists to prevent. It runs
# in a hosted workflow with `fetch-depth: 0`, and in release.yml's validate-release.
# Its *self-test* (fixture repos in a temp dir) is what belongs in ShellGates.
#
# Two directions, deliberately:
#   1. a closed issue that is not documented and not exempt  → fail
#   2. an exemption naming an issue that IS documented       → fail (stale)
# A gate that only fails one way rots into an allowlist nobody prunes.
#
# ⚠ `### Known issues` is NOT documentation (#1203). That section opens "Every number
# below is open at the time of this release", so a number there is a claim about the
# release, not a record of a change. Counting it let an issue FIXED in a release
# satisfy direction 1 by being listed among that release's known issues — the record
# saying the opposite of what shipped, with the gate that exists to keep the record
# honest reporting OK. It happened: at `7aa0d25df`, #1050 #1051 #1052 #1053 were each
# closed in range, had no `### Fixed` entry, and were green because all four appeared
# in one bullet of `### Known issues`. #1071 #1072 #1073 were wrong the same way.
#
# ⚠ The apparent complement — "every number under `### Known issues` must be open" —
# is NOT implemented, and the reason is worth recording so it is not re-added. That
# section legitimately names closed issues in corrective prose ("the eight issues this
# bullet used to list … are fixed in this release"), which is exactly the prose a
# reader needs. A lexical rule cannot tell a claim from a correction, so the strict
# form has false positives whose only fix is deleting useful text. The achievable half
# — a number whose ONLY mention is inside that block — is caught by direction 1 once
# `documented` excludes the block, and is reported with its own message below.
#
# Overrides, for testing:
#   CHANGELOG_CHECK_BASE=<rev>     compare from this rev instead of the last v* tag
#   CHANGELOG_CHECK_FILE=<path>    changelog to read instead of CHANGELOG.md
#   CHANGELOG_CHECK_EXEMPT=<path>  exemption list instead of the default
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

changelog="${CHANGELOG_CHECK_FILE:-CHANGELOG.md}"
exempt_file="${CHANGELOG_CHECK_EXEMPT:-tools/changelog-exempt-issues.txt}"

if [ ! -f "$changelog" ]; then
  echo "ERROR: changelog not found: $changelog" >&2
  exit 1
fi

# ── Base revision ────────────────────────────────────────────────────────────
#
# ⚠ On a tag build (release.yml runs on the tag it is validating) a bare
# `git describe --tags --abbrev=0` returns THE TAG BEING VALIDATED, so the range
# `<tag>..HEAD` is empty and the gate passes on an empty set — green while
# checking nothing. Exclude every v* tag that points at HEAD.
resolve_base() {
  local describe_args=(--tags --abbrev=0 --match 'v*')
  local t
  while IFS= read -r t; do
    [ -n "$t" ] && describe_args+=(--exclude "$t")
  done < <(git tag --points-at HEAD --list 'v*' || true)

  local base
  base="$(git describe "${describe_args[@]}" HEAD 2>/dev/null || true)"
  if [ -z "$base" ]; then
    # No prior release tag (a fresh repo, or a shallow clone with no tags —
    # which is why the callers set fetch-depth: 0). Fall back to the root commit
    # so the range is the whole history rather than silently empty.
    base="$(git rev-list --max-parents=0 HEAD | tail -1)"
  fi
  printf '%s' "$base"
}

base="${CHANGELOG_CHECK_BASE:-$(resolve_base)}"

# ── The three sets ───────────────────────────────────────────────────────────
# `sort -u`, never `sort -un`: numeric order makes `comm` reject its input
# ("input is not in sorted order") and hand back a wrong, longer answer.
closed="$(git log "${base}..HEAD" --pretty=%B \
  | grep -oE 'Closes #[0-9]+' | grep -oE '[0-9]+' | sort -u || true)"

# A number counts as DOCUMENTED only outside a `### Known issues` block. The block
# runs from its heading to the next `### ` or `## ` heading, and the exclusion is
# file-wide: an old release's known-issues list is not documentation of a change
# either. Everything else in the file still counts, so an entry under any change
# heading — or the prose of a section — satisfies the gate exactly as before.
documented="$(awk '
    /^### Known issues/ { in_known = 1; next }
    /^### / || /^## /   { in_known = 0 }
    !in_known           { print }
  ' "$changelog" | grep -oE '#[0-9]+' | grep -oE '[0-9]+' | sort -u || true)"

# What the section under active edit lists as still open, used to tell direction 1's
# two failure shapes apart. Scoped to that section — an issue listed as known in
# 2.13.0 and fixed two releases later is the record working, not a defect. "Under
# active edit" means the first `## [...]` section that HAS `### ` headings: between a
# cut and a tag the newest section sits below an empty `## [Unreleased]` (#1228).
known_claims="$(awk '
    /^## \[/ {
      if (started) exit
      pending = 1; in_known = 0; next
    }
    /^### / {
      if (pending) { started = 1; pending = 0 }
      in_known = ($0 ~ /^### Known issues/)
      next
    }
    started && in_known { print }
  ' "$changelog" | grep -oE '#[0-9]+' | grep -oE '[0-9]+' | sort -u || true)"

exempt=""
if [ -f "$exempt_file" ]; then
  # Format: `#N  <reason>`. A bare number is rejected below — an exemption
  # without a stated reason is indistinguishable from an oversight.
  #
  # Comments also open with `#`, so the discriminator is the character after it:
  # `#` + digit is an entry and must carry a reason; `#` + anything else is prose.
  bad_lines="$(awk '
    /^[[:space:]]*$/           { next }                       # blank
    /^[[:space:]]*#[0-9]/      { if ($0 !~ /^[[:space:]]*#[0-9]+[[:space:]]+[^[:space:]]/) print; next }
    /^[[:space:]]*#/           { next }                       # comment prose
                               { print }                      # anything else
  ' "$exempt_file")"
  if [ -n "$bad_lines" ]; then
    {
      echo "ERROR: $exempt_file has entries that are not '#N  <reason>':"
      printf '%s\n' "$bad_lines" | sed 's/^/  /'
      echo
      echo "An exemption without a stated reason cannot be reviewed or pruned."
    } >&2
    exit 1
  fi
  exempt="$(grep -oE '^[[:space:]]*#[0-9]+' "$exempt_file" | grep -oE '[0-9]+' | sort -u || true)"
fi

failed=0

# ── Direction 1: closed but undocumented and unexempt ────────────────────────
undocumented="$(comm -23 <(printf '%s\n' "$closed" | grep -vE '^$' || true) \
                         <(printf '%s\n' "$documented" | grep -vE '^$' || true))"
missing="$(comm -23 <(printf '%s\n' "$undocumented" | sort -u | grep -vE '^$' || true) \
                    <(printf '%s\n' "$exempt" | grep -vE '^$' || true))"

if [ -n "$missing" ]; then
  # Split the report: an issue mentioned ONLY inside `### Known issues` is a
  # different mistake from one mentioned nowhere, and it has a different fix. It is
  # also the one that makes the record say the opposite of what shipped (#1203).
  only_known="$(comm -12 <(printf '%s\n' "$missing" | grep -vE '^$' || true) \
                         <(printf '%s\n' "$known_claims" | grep -vE '^$' || true))"
  nowhere="$(comm -23 <(printf '%s\n' "$missing" | grep -vE '^$' || true) \
                      <(printf '%s\n' "$only_known" | grep -vE '^$' || true))"
  {
    if [ -n "$only_known" ]; then
      echo "ERROR: issues closed since ${base} appear in ${changelog} ONLY under"
      echo "'### Known issues', which claims every number below it is still open:"
      printf '%s\n' "$only_known" | sed 's/^/  #/'
      echo
      echo "A fix recorded as a known issue is a fix nobody knows they have, and an"
      echo "operator reading that section may defer an upgrade or apply a workaround"
      echo "for something that shipped. Move each to the change heading that says what"
      echo "happened to it (Added / Changed / Deprecated / Removed / Fixed / Security /"
      echo "Breaking); the known-issues bullet may keep naming it as a correction."
      echo
    fi
    if [ -n "$nowhere" ]; then
      echo "ERROR: issues closed since ${base} are absent from ${changelog}:"
      printf '%s\n' "$nowhere" | sed 's/^/  #/'
      echo
      echo "Document each under the correct existing heading, or add it to"
      echo "${exempt_file} as '#N  <why it is not user-facing>'."
    fi
  } >&2
  failed=1
fi

# ── Direction 2: stale exemptions ────────────────────────────────────────────
stale="$(comm -12 <(printf '%s\n' "$exempt" | grep -vE '^$' || true) \
                  <(printf '%s\n' "$documented" | grep -vE '^$' || true))"
if [ -n "$stale" ]; then
  {
    echo "ERROR: ${exempt_file} exempts issues that ARE documented in ${changelog}:"
    printf '%s\n' "$stale" | sed 's/^/  #/'
    echo
    echo "Remove the exemption. Keeping it lets the list rot into an allowlist"
    echo "nobody prunes, which is how the next omission hides."
  } >&2
  failed=1
fi

# ── Structure: one heading per type in the section under active edit ─────────
#
# Scoped to the first `## [...]` section that HAS `### ` headings — `[Unreleased]`,
# or the released version immediately after a cut. Historical sections are left
# alone: they carry free-form headings ("Migration", "Known limitations") and three
# genuine duplicates, and rewriting shipped release notes to satisfy a new gate would
# be falsifying the record.
#
# ⚠ "first section that has headings", not "first section" (#1228): between a cut and
# a tag the newest section sits below an EMPTY `## [Unreleased]`, and scoping to the
# first section outright examined zero headings and passed unconditionally — a
# duplicate `### Fixed` and an unknown `### Totally Bogus Heading` injected into
# `[2.15.0]` were both green. That is the file's ordinary state during exactly the
# window in which the release record is written.
structure_errors="$(
  awk '
    /^## \[/ { if (seen_section) exit; pending = 1; next }
    pending && /^### / { seen_section = 1; pending = 0 }
    seen_section && /^### / {
      h = $0; sub(/^### +/, "", h); sub(/ +$/, "", h)
      key = tolower(h)
      count[key]++
      if (count[key] == 1) order[++n] = key
      raw[key] = h
    }
    END {
      split("breaking|added|changed|deprecated|removed|fixed|security|known issues", a, "|")
      for (i in a) allowed[a[i]] = 1
      for (i = 1; i <= n; i++) {
        k = order[i]
        if (!(k in allowed)) printf "unknown heading \"### %s\" — allowed: Breaking, Added, Changed, Deprecated, Removed, Fixed, Security, Known issues\n", raw[k]
        else if (count[k] > 1) printf "\"### %s\" appears %d times — merge into one heading per type\n", raw[k], count[k]
      }
    }
  ' "$changelog"
)"

if [ -n "$structure_errors" ]; then
  {
    echo "ERROR: heading structure in the newest ${changelog} section:"
    printf '%s\n' "$structure_errors" | sed 's/^/  /'
  } >&2
  failed=1
fi

if [ "$failed" -eq 0 ]; then
  n_closed="$(printf '%s\n' "$closed" | grep -cE '^[0-9]+$' || true)"
  n_exempt="$(printf '%s\n' "$exempt" | grep -cE '^[0-9]+$' || true)"
  echo "OK: ${n_closed} issues closed since ${base}; all documented in ${changelog} or exempt (${n_exempt})."
fi
exit "$failed"
