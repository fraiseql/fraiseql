#!/usr/bin/env bash
# Unit tests for tools/check-changelog-issues.sh.
#
# Run directly:  bash tools/tests/changelog_gate_test.sh
# Exits non-zero if any assertion fails.
#
# The gate itself needs real git history, so it CANNOT run in Dagger ShellGates
# (`+ignore=[".git"]` plus `git init -q .` in the container would leave it looking
# at an empty repository, finding zero `Closes #N`, and passing vacuously). This
# self-test builds its own fixture repositories in a temp directory, so it needs no
# ambient history and does belong there — the same split as
# `make test-deadline-gate` and `make test-release-tooling`.
#
# What is worth pinning here, in order of how badly each would hurt:
#
#   1. **Tag-build base resolution.** On the tag build that validates a release,
#      a bare `git describe --tags --abbrev=0` returns the tag being validated, so
#      `<tag>..HEAD` is empty and the gate passes having checked nothing — green at
#      exactly the moment it is the last thing standing between an incomplete
#      changelog and a published release.
#   2. **Both failure directions.** A gate that only fails on a missing entry lets
#      the exemption list rot into an allowlist nobody prunes.
#   3. **Reason-less exemptions.** `#123` with no prose is indistinguishable from
#      an oversight six months later.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-changelog-issues.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ── Fixture repository ───────────────────────────────────────────────────────
# mk_repo <name> <changelog-body> <exempt-body> [extra-tag-at-head]
#
# Layout: one commit + tag v1.0.0, then a commit whose message closes #42 and
# #43. #42 is the "user-facing" one each case decides what to do with.
mk_repo() {
    local name="$1" changelog="$2" exempt="$3" head_tag="${4:-}"
    local dir="$WORK/$name"
    mkdir -p "$dir"
    (
        cd "$dir"
        git init -q .
        # The Dagger container has no global git identity; set it locally or
        # every fixture commit fails and the whole suite errors out.
        git config user.email "gate-test@example.invalid"
        git config user.name "Gate Test"
        git config commit.gpgsign false
        # ⚠ A developer whose global config sets tag.gpgSign / tag.forceSignAnnotated
        # (both are ordinary hardening) turns a lightweight `git tag v1.0.0` into
        # `fatal: no tag message?` and rc=128 — NO TAG IS CREATED. The gate then
        # falls back to the root commit, the range covers everything, and the
        # tag-build assertion below passes for a reason that has nothing to do with
        # tag resolution. Observed on this machine; pinned here so the fixture is
        # not at the mercy of ambient configuration.
        git config tag.gpgSign false
        git config tag.forceSignAnnotated false
        echo "seed" > seed.txt
        git add -A
        git commit -q -m "seed"
        git tag v1.0.0
        printf '%s\n' "$changelog" > CHANGELOG.md
        printf '%s\n' "$exempt" > exempt.txt
        git add -A
        git commit -q -m "fix(x): a change

Closes #42
Closes #43"
        [ -n "$head_tag" ] && git tag "$head_tag"

        # A fixture that failed to build must be loud, not silently degrade the
        # assertion it supports.
        want_tags="v1.0.0${head_tag:+ $head_tag}"
        for t in $want_tags; do
            if [ -z "$(git tag -l "$t")" ]; then
                echo "FIXTURE ERROR: tag $t was not created in $dir" >&2
                exit 1
            fi
        done
        if [ "$(git rev-list --count HEAD)" -ne 2 ]; then
            echo "FIXTURE ERROR: expected 2 commits in $dir" >&2
            exit 1
        fi
    ) || exit 1
    printf '%s' "$dir"
}

# run_gate <dir> → combined output; returns the gate's exit code.
run_gate() {
    local dir="$1"
    (
        cd "$dir"
        CHANGELOG_CHECK_FILE="$dir/CHANGELOG.md" \
        CHANGELOG_CHECK_EXEMPT="$dir/exempt.txt" \
        CHANGELOG_CHECK_BASE="" \
            bash "$GATE" 2>&1
    )
}

# assert <name> <dir> <expected-rc> <expected-substring>
# An empty expected-substring asserts only the exit code.
assert() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local name="$1" dir="$2" want_rc="$3" want_sub="$4"
    local out rc
    set +e
    out="$(run_gate "$dir")"
    rc=$?
    set -e
    if [[ "$rc" -eq "$want_rc" && ( -z "$want_sub" || "$out" == *"$want_sub"* ) ]]; then
        echo "  ok: $name"
    else
        echo "  FAIL: $name — rc=$rc (want $want_rc), output did not contain '$want_sub':" >&2
        printf '%s\n' "$out" | sed 's/^/      /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

BOTH_DOCUMENTED='## [Unreleased]

### Fixed

- a thing (#42)
- another thing (#43)'

ONLY_43='## [Unreleased]

### Fixed

- another thing (#43)'

echo "check-changelog-issues.sh"

# ── Direction 1: a closed issue must be documented ────────────────────────────
d="$(mk_repo documented "$BOTH_DOCUMENTED" '# none')"
assert "documented issues pass" "$d" 0 "OK:"

d="$(mk_repo undocumented "$ONLY_43" '# none')"
assert "an undocumented issue fails, naming it" "$d" 1 "#42"

d="$(mk_repo exempted "$ONLY_43" '#42  internal test-rig change')"
assert "undocumented but exempt passes" "$d" 0 "OK:"

# ── Direction 2: an exemption for a documented issue is stale ─────────────────
# Without this the list silently accumulates entries for issues that were later
# written up, and nobody can tell a live exemption from a dead one.
d="$(mk_repo stale "$BOTH_DOCUMENTED" '#42  internal test-rig change')"
assert "an exemption for a documented issue fails" "$d" 1 "exempts issues that ARE documented"

# ── An exemption must carry a reason ──────────────────────────────────────────
d="$(mk_repo reasonless "$ONLY_43" '#42')"
assert "a reason-less exemption fails" "$d" 1 "not '#N  <reason>'"

d="$(mk_repo unhashed "$ONLY_43" '42  forgot the hash')"
assert "an entry with no leading # fails" "$d" 1 "not '#N  <reason>'"

# Comment prose in the exemption file is not an entry and must not trip the
# format check — the file is meant to be read, so it has to be annotatable.
d="$(mk_repo commented "$ONLY_43" '# why this file exists
#42  internal test-rig change')"
assert "comment lines are not entries" "$d" 0 "OK:"

# ── Tag-build base resolution (the one that matters most) ─────────────────────
# release.yml validates the tag it is about to publish, so HEAD carries v2.0.0.
# A bare `git describe --tags --abbrev=0` returns v2.0.0 → the range v2.0.0..HEAD
# is empty → the gate passes having examined nothing, at exactly the moment it is
# the last check before publish. Base must resolve to v1.0.0 and #42 must fail.
d="$(mk_repo tagbuild "$ONLY_43" '# none' v2.0.0)"
assert "a tag build resolves past the tag being validated" "$d" 1 "#42"

d="$(mk_repo tagbuild_ok "$BOTH_DOCUMENTED" '# none' v2.0.0)"
assert "a tag build passes when the range is documented" "$d" 0 "OK:"

# ── Structure: one heading per type in the newest section ─────────────────────
DUPLICATE_HEADING='## [Unreleased]

### Fixed

- a thing (#42)

### Fixed

- another thing (#43)'
d="$(mk_repo dup_heading "$DUPLICATE_HEADING" '# none')"
assert "a duplicate heading fails" "$d" 1 "appears 2 times"

UNKNOWN_HEADING='## [Unreleased]

### Fixed

- a thing (#42)

### Miscellaneous

- another thing (#43)'
d="$(mk_repo unknown_heading "$UNKNOWN_HEADING" '# none')"
assert "an off-list heading fails" "$d" 1 "unknown heading"

# `Known issues` is on the allow-list because the release phase adds it under the
# released version; it must not be rejected as an off-list heading.
KNOWN_ISSUES='## [Unreleased]

### Fixed

- a thing (#42)
- another thing (#43)

### Known issues

- something that did not make the cut'
d="$(mk_repo known_issues "$KNOWN_ISSUES" '# none')"
assert "\"Known issues\" is allowed" "$d" 0 "OK:"

# Older released sections are left alone: they carry free-form headings and
# genuine duplicates, and rewriting shipped release notes to satisfy a new gate
# would falsify the record. Only the newest section is checked.
HISTORICAL_MESS='## [Unreleased]

### Fixed

- a thing (#42)
- another thing (#43)

## [0.9.0] - 2020-01-01

### Migration

- an old free-form heading

### Fixed

- one

### Fixed

- and again'
d="$(mk_repo historical "$HISTORICAL_MESS" '# none')"
assert "older sections are not structure-checked" "$d" 0 "OK:"

echo
if [[ "$TESTS_FAILED" -gt 0 ]]; then
    echo "FAILED: $TESTS_FAILED of $TESTS_RUN assertions" >&2
    exit 1
fi
echo "PASSED: $TESTS_RUN assertions"
