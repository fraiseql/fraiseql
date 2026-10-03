#!/usr/bin/env bash
# ci-stale-artifacts.sh — did this build reuse a workspace artifact from before the leg began? (#1421)
#
# Every Rust leg stamps the whole source tree with one fresh mtime before its first cargo
# call (`rustSource` in .dagger/main.go) and exports that instant as
# FRAISEQL_SOURCE_TOUCHED_AT. From then on cargo cannot judge a workspace crate fresh on an
# artifact built by an earlier run: the sources are newer than anything that run wrote.
# So every workspace artifact a build reports as cached ("fresh":true) must have been
# written AFTER the stamp, by this leg. One older than the stamp was built from other source
# — by an earlier commit, or another branch's run sharing the target volume — and a test
# binary linked against it does not test this commit.
#
# Usage: ci-stale-artifacts.sh <cargo --message-format=json log> <touched-at epoch secs> <workspace root>
# Prints each stale artifact and exits 1 if there is any; exits 0 otherwise.
# Registry dependencies are not workspace units and are not judged: their source never
# changes under a version, which is what makes them safe to share.

set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: ci-stale-artifacts.sh <build-log> <touched-at> <workspace-root>" >&2
    exit 2
fi
log="$1"
touched_at="$2"
root="${3%/}"

stale=0
# A cached workspace unit: compiler-artifact, fresh, and a package_id that is a path
# under the workspace root. The `filenames` array lists every artifact it produced.
while IFS= read -r line; do
    files="$(grep -o '"filenames":\[[^]]*\]' <<<"${line}" | sed 's/^"filenames":\[//; s/\]$//')"
    [[ -z "${files}" ]] && continue
    while IFS= read -r f; do
        f="${f#\"}"
        f="${f%\"}"
        [[ -z "${f}" || ! -e "${f}" ]] && continue
        if (( $(stat -c %Y "${f}") < touched_at )); then
            echo "STALE: ${f} predates this leg's source stamp (${touched_at})"
            stale=1
        fi
    done < <(sed 's/","/"\n"/g' <<<"${files}")
done < <(grep '"reason":"compiler-artifact"' "${log}" \
    | grep '"fresh":true' \
    | grep -F "\"package_id\":\"path+file://${root}/" || true)

exit "${stale}"
