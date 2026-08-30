#!/usr/bin/env bash
# image_context_test.sh — the red capability of tools/check-image-context.sh.
#
# The gate narrows a build context, and a wrong narrowing is silent: the build
# runs against a context missing a path it COPYs. None of these cases is
# observable from a passing run of the real tree.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-image-context.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); }

# A fixture "repo": an image.go with a variant table and +ignore, plus Dockerfiles.
# Run from the fixture dir, so the gate's `git rev-parse || pwd` roots there.
make_fixture() {
  local root="$1" ignore="$2"; shift 2
  mkdir -p "$root/.dagger"
  {
    echo 'var imageVariants = []imageVariant{'
    for df in "$@"; do echo "	{name: \"x\", dockerfile: \"${df}\", buildContext: \".\"},"; done
    echo '}'
    echo 'func (m *FraiseqlCi) Image('
    echo "	// +ignore=${ignore}"
    echo '	source *dagger.Directory,'
    echo ') {}'
  } > "$root/.dagger/image.go"
}

run_gate() { ( cd "$1" && bash "$gate" ) > "${tmp}/out" 2>&1; echo $?; }

# ── 1. a COPY the filter drops is caught ─────────────────────────────────────
root="${tmp}/drops"; make_fixture "$root" '["**", "!crates/**"]' Dockerfile
printf 'COPY crates ./crates\nCOPY docs ./docs\n' > "$root/Dockerfile"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q 'drops `docs`' "${tmp}/out"; then
  pass "a COPY the filter drops fails, naming the path"
else
  fail "a dropped COPY was tolerated (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 2. a SECOND Dockerfile's COPY is checked too ─────────────────────────────
# The defect this gate was written against: narrowing for the root Dockerfile and
# forgetting tutorial/Dockerfile, whose COPY sources the filter then drops.
root="${tmp}/second"; make_fixture "$root" '["**", "!crates/**"]' Dockerfile tutorial/Dockerfile
printf 'COPY crates ./crates\n' > "$root/Dockerfile"
mkdir -p "$root/tutorial"
printf 'COPY tutorial/src ./src\n' > "$root/tutorial/Dockerfile"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q 'tutorial' "${tmp}/out"; then
  pass "a second Dockerfile's COPY is checked, not just the root one"
else
  fail "the second Dockerfile was not checked (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 3. everything admitted passes ────────────────────────────────────────────
root="${tmp}/ok"; make_fixture "$root" '["**", "!Dockerfile", "!crates/**", "!tutorial/**"]' Dockerfile tutorial/Dockerfile
printf 'COPY crates ./crates\n' > "$root/Dockerfile"
mkdir -p "$root/tutorial"
printf 'COPY tutorial/src ./src\nCOPY --from=builder /x ./x\n' > "$root/tutorial/Dockerfile"
rc=$(run_gate "$root")
if [ "$rc" -eq 0 ]; then
  pass "a filter admitting every COPY passes (and --from= is not a context path)"
else
  fail "a correct filter did not pass (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 4. a filter with NO catch-all: an explicit exclude of a COPY is caught ───
root="${tmp}/nocatch"; make_fixture "$root" '["crates", ".git"]' Dockerfile
printf 'COPY crates ./crates\n' > "$root/Dockerfile"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q 'excludes' "${tmp}/out"; then
  pass "without a catch-all, an explicit exclude of a COPYed path is caught"
else
  fail "an explicit exclude was tolerated (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 5. the Dockerfile ITSELF must survive the filter ─────────────────────────
# A filter can admit every COPY source and still drop the file doing the copying.
# DockerBuild reads the Dockerfile from the context, and the failure it gives is
# just "failed to build" — which is how the first narrowing here got through.
root="${tmp}/nodockerfile"; make_fixture "$root" '["**", "!crates/**"]' Dockerfile
printf 'COPY crates ./crates\n' > "$root/Dockerfile"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q 'Dockerfile`' "${tmp}/out"; then
  pass "a filter that drops the Dockerfile itself is caught"
else
  fail "a filter dropping the Dockerfile passed (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 6. an unparseable variant table fails LOUDLY ─────────────────────────────
root="${tmp}/novariants"; mkdir -p "$root/.dagger"
printf 'func (m *FraiseqlCi) Image(\n\t// +ignore=["**"]\n) {}\n' > "$root/.dagger/image.go"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q "parsed no dockerfile" "${tmp}/out"; then
  pass "a variant table it cannot parse fails loudly, not vacuously"
else
  fail "an unparseable variant table passed (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 7. a variant naming a Dockerfile that does not exist ─────────────────────
root="${tmp}/missing"; make_fixture "$root" '["**", "!crates/**"]' Dockerfile
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q "which does not exist" "${tmp}/out"; then
  pass "a variant naming a missing Dockerfile fails"
else
  fail "a missing Dockerfile was tolerated (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

echo
if [ "$failures" -eq 0 ]; then
  echo "OK: check-image-context.sh can fail, in all seven ways."
else
  echo "✗ ${failures} self-test(s) failed."
fi
exit "$failures"
