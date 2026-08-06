#!/usr/bin/env bash
# check-empty-tests.sh — refuse #[test] functions whose body is only comments.
#
# A test with a comment-only body carries a name that promises an assertion,
# contributes to the green count, and asserts nothing. ~90 of them let the
# RBAC subsystem ship DDL that did not parse (#748); 15 more survived
# workspace-wide until #895. This gate keeps the sixteenth from landing:
# write the assertion or delete the test — "it compiles" is what the build
# already proves.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

PATTERN='#\[(tokio::)?test\][^\n]*\n\s*(async )?fn \w+\([^)]*\)\s*\{(\s*//[^\n]*\n)*\s*\}'

found=0
while IFS= read -r file; do
    if grep -Pzoq "${PATTERN}" "${file}"; then
        echo "✗ ${file}: comment-only #[test] body:"
        grep -Pzo "${PATTERN}" "${file}" | tr '\0' '\n' | sed 's/^/    /'
        found=1
    fi
done < <(find crates -name '*.rs' -not -path '*/target/*')

if [[ ${found} -ne 0 ]]; then
    echo ""
    echo "FAIL: write the assertion or delete the test (#895). A body that is"
    echo "only comments reads as green coverage and provides none."
    exit 1
fi

echo "OK: no comment-only test bodies."
