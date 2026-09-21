#!/usr/bin/env bash
# Unit tests for tools/check-release-validation.py.
#
# Run directly:  bash tools/tests/release_validation_gate_test.sh
# Exits non-zero if any assertion fails.
#
# A fixture pair of workflows that pass, then one mutation per rule: continue-on-error back
# on, the install unpinned, the load removed, the version comparison removed, the step renamed
# away, the workflow missing, the workflow unparseable. Each must be red for its own reason.
# shellcheck disable=SC2016  # the ${VERSION} in the sed patterns is meant literally: it is the workflow's text
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-release-validation.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# good_root <dir>: both workflows with blocking, complete validation steps.
good_root() {
    local r="$1"
    mkdir -p "$r/.github/workflows"
    cat > "$r/.github/workflows/release.yml" <<'EOF'
jobs:
  publish-python:
    steps:
      - name: Validate PyPI package
        run: |
          python -m pip install "fraiseql==${VERSION}"
          python -c "import fraiseql; assert fraiseql.__version__ == '${VERSION}'"
EOF
    cat > "$r/.github/workflows/npm-publish.yml" <<'EOF'
jobs:
  publish:
    steps:
      - name: Validate npm package
        run: |
          npm install "fraiseql@${VERSION}" --prefix /tmp/v
          node -e "const p = require('/tmp/v/node_modules/fraiseql/package.json'); if (p.version !== '${VERSION}') throw 1; require('/tmp/v/node_modules/fraiseql')"
EOF
}

# assert_gate <name> <expected-rc> <expected-substring> <root>
assert_gate() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local name="$1" want_rc="$2" want_sub="$3" root="$4"
    local out rc
    set +e
    out="$(RELEASE_VALIDATION_ROOT="$root" python3 "$GATE" 2>&1)"
    rc=$?
    set -e
    if [[ "$rc" -eq "$want_rc" && "$out" == *"$want_sub"* ]]; then
        echo "  ok: $name"
    else
        echo "  FAIL: $name — rc=$rc (want $want_rc), output did not contain '$want_sub':" >&2
        printf '%s\n' "$out" | sed 's/^/      /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "check-release-validation.py:"
r="$WORK/good"; good_root "$r"
assert_gate "blocking, pinned, loading, comparing steps pass" 0 "release validation: ok" "$r"

r="$WORK/coe"; good_root "$r"; sed -i 's/^        run: |$/        continue-on-error: true\n        run: |/' "$r/.github/workflows/release.yml"
assert_gate "continue-on-error is red" 1 "continue-on-error is True" "$r"

r="$WORK/unpinned"; good_root "$r"; sed -i 's/fraiseql==${VERSION}/fraiseql/' "$r/.github/workflows/release.yml"
assert_gate "unpinned install is red" 1 "does not install the release version" "$r"

r="$WORK/noload"; good_root "$r"; sed -i "s/; require('\/tmp\/v\/node_modules\/fraiseql')//" "$r/.github/workflows/npm-publish.yml"
assert_gate "npm step that never loads the package is red" 1 "does not load the package" "$r"

r="$WORK/nocompare"; good_root "$r"; sed -i 's/assert fraiseql.__version__ == .${VERSION}./pass/' "$r/.github/workflows/release.yml"
assert_gate "missing version comparison is red" 1 "does not compare the installed version" "$r"

r="$WORK/renamed"; good_root "$r"; sed -i 's/Validate npm package/Check npm/' "$r/.github/workflows/npm-publish.yml"
assert_gate "renamed step is red (never exercised)" 1 "no step named 'Validate npm package'" "$r"

r="$WORK/missing"; good_root "$r"; rm "$r/.github/workflows/npm-publish.yml"
assert_gate "missing workflow is red" 1 "npm-publish.yml: not found" "$r"

r="$WORK/broken"; good_root "$r"; printf 'jobs: [\n' > "$r/.github/workflows/release.yml"
assert_gate "unparseable workflow is red" 1 "not parseable YAML" "$r"

echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
