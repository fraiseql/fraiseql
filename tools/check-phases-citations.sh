#!/usr/bin/env bash
# check-phases-citations.sh — no shipped file sends a reader to `.phases/`.
#
# Background (#1210): `.gitignore` ignores `.phases/`, so a `.phases/` tree lives
# only in the working copy that created it. Twenty-seven tracked files cited one
# anyway — including sixteen comments in `.dagger/*.go` and three workflows that
# deferred their REASONING to `parity-notes.md`. A contributor in a fresh clone
# got the conclusion with the argument amputated. `docker-build.yml` was the
# concrete cost: its header pointed at a plan nobody could read, and two jobs that
# could never run sat under that comment for three months (#1206).
#
# The rule is structural rather than a link check: because `.phases/` is ignored,
# ANY citation of one from a shipped file is unreachable by construction. There is
# nothing to resolve — publish what is load bearing (as `parity-notes.md` was, to
# docs/contributing/dagger-parity-notes.md) and drop the rest.
#
# ⚠ Deliberately not `git ls-files`: it returns NOTHING in the Dagger ShellGates
# container, which would make this gate pass vacuously exactly where it runs.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

SCAN_ROOT="${PHASES_CITATION_SCAN_ROOT:-.}"

# Files that may name `.phases/` because naming it is their job. Matched on the
# path RELATIVE TO THE SCAN ROOT, not the repo root: keying on the latter would
# make every exemption silently inert under any other root — including the one
# the self-test uses, which is how this was caught.
is_exempt() {
    local rel="${1#./}"
    rel="${rel#"$SCAN_ROOT"/}"
    rel="${rel#./}"
    case "$rel" in
        .gitignore|.dockerignore|.pre-commit-config.yaml) return 0 ;;
        CHANGELOG.md)                                     return 0 ;;  # history
        .phases/*)                                        return 0 ;;  # its own tree
        tools/check-phases-citations.sh)                  return 0 ;;  # this file
        tools/tests/phases_citations_test.sh)             return 0 ;;
        docs/contributing/dagger-parity-notes.md)         return 0 ;;  # says where it came from
        *) return 1 ;;
    esac
}

mapfile -t candidates < <(
    find "$SCAN_ROOT" \
        -path '*/.git' -prune -o \
        -path '*/target' -prune -o \
        -path '*/node_modules' -prune -o \
        -type f -print 2>/dev/null | sort
)

if [ "${#candidates[@]}" -eq 0 ]; then
    echo "✗ found no files under '$SCAN_ROOT' at all — the search is wrong, not the tree."
    exit 1
fi

found=0
scanned=0
for f in "${candidates[@]}"; do
    is_exempt "$f" && continue
    scanned=$((scanned + 1))
    # A CITATION is a path INTO `.phases/` — `.phases/<something>`. A bare mention
    # of the directory is not one: `.gitignore` and `.dockerignore` name it as a
    # pattern, and prose may say the word. Requiring a path component after the
    # slash is what separates "sends a reader somewhere that does not exist" from
    # "talks about the directory". -I skips binaries.
    if hits=$(grep -InIE '\.phases/[A-Za-z0-9_.*-]' "$f" 2>/dev/null); then
        while IFS= read -r line; do
            echo "✗ ${f#./}:${line%%:*} cites a .phases/ path"
            printf '    %s\n' "$(printf '%s' "$line" | cut -d: -f2- | sed 's/^[0-9]*://' | cut -c1-100)"
            found=1
        done <<< "$hits"
    fi
done

if [ "$scanned" -eq 0 ]; then
    echo "✗ every candidate file was exempt — this gate would be checking nothing."
    exit 1
fi

if [ "$found" -ne 0 ]; then
    echo
    echo "  \`.phases/\` is gitignored, so these paths do not exist in a clone."
    echo "  Publish what is load bearing under docs/ and repoint, or drop the citation."
    exit 1
fi

echo "OK: no shipped file cites a .phases/ path ($scanned file(s) scanned)."
