#!/usr/bin/env bash
# ci_source_stamp_test.sh — the red capability of #1596's fix: tools/ci-stamp-sources.sh
# and tools/check-ci-source-stamp.sh.
#
# Runs in the shape a leg and ShellGates have: a source tree with no .git, so nothing here
# can lean on `git ls-files` (empty in both). Each case builds its own tree in a temp dir.
#
#   stamp:
#     * the defect: a stamp taken before the volume was held (t0) cannot see an artifact
#       another leg wrote after it (t1 > t0): ci-stale-artifacts.sh passes it    → exit 0
#     * sourced once the volume is held, the stamp is later than that artifact,
#       which is then refused                                                    → exit 1
#     * every source file carries the stamp, the target volume keeps its mtimes
#     * a nested bash inherits the instant and touches nothing again
#     * without FRAISEQL_STAMP_ROOT (not a Dagger leg) nothing is touched
#   gate:
#     * the rustSource of the defect (touch exec before the mount)               → exit 1
#     * a rustSource running any exec of its own, or not setting BASH_ENV         → exit 1
#     * a Go-side FRAISEQL_SOURCE_TOUCHED_AT                                     → exit 1
#     * a direct `cargo` exec on a Rust leg's container, on one line or two      → exit 1
#     * the fixed rustSource, every leg exec through bash                        → exit 0
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
stamp_script="${repo_root}/tools/ci-stamp-sources.sh"
stale="${repo_root}/tools/ci-stale-artifacts.sh"
gate="${repo_root}/tools/check-ci-source-stamp.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); [[ -f "${tmp}/out" ]] && sed 's/^/       /' "${tmp}/out"; }
mtime() { stat -c %Y "$1"; }

now="$(date +%s)"
t0=$((now - 100))   # when the pipeline was built (the old stamp)
t1=$((now - 50))    # when another branch's leg, holding the volume, wrote its artifact

tree() { # tree <dir>: a workspace with no .git, a target volume holding one cached rlib
    local root="$1"
    mkdir -p "${root}/crates/a/src" "${root}/target/debug/deps"
    echo 'fn main() {}' > "${root}/crates/a/src/lib.rs"
    echo '[package]' > "${root}/crates/a/Cargo.toml"
    # Checked-out sources are older than anything below: a stamp has to move them.
    touch -d "@${t0}" "${root}/crates/a/src/lib.rs" "${root}/crates/a/Cargo.toml"
    : > "${root}/target/debug/deps/liba-0000.rlib"
    touch -d "@${t1}" "${root}/target/debug/deps/liba-0000.rlib"
    printf '{"reason":"compiler-artifact","package_id":"path+file://%s/crates/a#0.1.0","fresh":true,"filenames":["%s/target/debug/deps/liba-0000.rlib"]}\n' \
        "${root}" "${root}" > "${root}/build.json"
}

echo "== ci-stamp-sources.sh"

root="${tmp}/defect"; tree "${root}"
if bash "${stale}" "${root}/build.json" "${t0}" "${root}" >"${tmp}/out" 2>&1; then
    pass "the defect: a stamp taken before the volume was held passes another leg's later artifact"
else
    fail "precondition: the old stamp was expected to miss the artifact"
fi

root="${tmp}/fixed"; tree "${root}"
before="$(mtime "${root}/target/debug/deps/liba-0000.rlib")"
stamped="$(env -u FRAISEQL_SOURCE_TOUCHED_AT FRAISEQL_STAMP_ROOT="${root}" BASH_ENV="${stamp_script}" \
    bash -c 'echo "${FRAISEQL_SOURCE_TOUCHED_AT}"')"
if [[ -n "${stamped}" ]] && (( stamped > t1 )); then
    pass "sourced as the exec starts, the stamp is later than the other leg's artifact"
else
    fail "the stamp '${stamped}' is not later than ${t1}"
fi
if ! bash "${stale}" "${root}/build.json" "${stamped}" "${root}" >"${tmp}/out" 2>&1 \
    && grep -q "STALE" "${tmp}/out"; then
    pass "that artifact is refused as stale"
else
    fail "the artifact written before the stamp was not refused"
fi
if [[ "$(mtime "${root}/crates/a/src/lib.rs")" == "${stamped}" \
    && "$(mtime "${root}/crates/a/Cargo.toml")" == "${stamped}" ]]; then
    pass "every source file carries the stamp, found without git"
else
    fail "a source file does not carry the stamp"
fi
if [[ "$(mtime "${root}/target/debug/deps/liba-0000.rlib")" == "${before}" ]]; then
    pass "the target volume keeps its mtimes"
else
    fail "the stamp touched the target volume"
fi

touch -d "@${t0}" "${root}/crates/a/src/lib.rs"
nested="$(FRAISEQL_SOURCE_TOUCHED_AT="${stamped}" FRAISEQL_STAMP_ROOT="${root}" BASH_ENV="${stamp_script}" \
    bash -c 'bash -c "echo \${FRAISEQL_SOURCE_TOUCHED_AT}"')"
if [[ "${nested}" == "${stamped}" && "$(mtime "${root}/crates/a/src/lib.rs")" == "${t0}" ]]; then
    pass "a nested bash inherits the instant and touches nothing"
else
    fail "a nested bash re-stamped (${nested} vs ${stamped})"
fi

root="${tmp}/local"; tree "${root}"
touch -d "@${t0}" "${root}/crates/a/src/lib.rs"
env -u FRAISEQL_SOURCE_TOUCHED_AT -u FRAISEQL_STAMP_ROOT BASH_ENV="${stamp_script}" bash -c 'true'
if [[ "$(mtime "${root}/crates/a/src/lib.rs")" == "${t0}" ]]; then
    pass "outside a Dagger leg (no FRAISEQL_STAMP_ROOT) nothing is touched"
else
    fail "a run without FRAISEQL_STAMP_ROOT touched sources"
fi

echo "== check-ci-source-stamp.sh"

dagger_dir() { # dagger_dir <name> <main.go content>
    mkdir -p "${tmp}/$1"
    printf '%s\n' "$2" > "${tmp}/$1/main.go"
    echo "${tmp}/$1"
}

fixed_source='func rustSource(base *dagger.Container, source *dagger.Directory, targetVol string) *dagger.Container {
	return base.
		WithMountedDirectory("/src", source).
		WithMountedCache("/src/target", dag.CacheVolume(targetVol)).
		WithEnvVariable("FRAISEQL_STAMP_ROOT", "/src").
		WithEnvVariable("BASH_ENV", "/src/tools/ci-stamp-sources.sh")
}'

expect() { # expect <0|1> <label> <dir>
    bash "${gate}" "$3" >"${tmp}/out" 2>&1
    local rc=$?
    if [[ "${rc}" -eq "$1" ]]; then pass "$2"; else fail "$2 (exit ${rc}, expected $1)"; fi
}

expect 1 "the rustSource of the defect is refused" "$(dagger_dir old 'func rustSource(base *dagger.Container, source *dagger.Directory, targetVol string) *dagger.Container {
	stamp := strconv.FormatInt(time.Now().Unix(), 10)
	return base.
		WithMountedDirectory("/src", source).
		WithEnvVariable("FRAISEQL_SOURCE_TOUCHED_AT", stamp).
		WithExec([]string{"bash", "-c", `find /src -mindepth 1 -exec touch -h -d "@${FRAISEQL_SOURCE_TOUCHED_AT}" {} +`}).
		WithMountedCache("/src/target", dag.CacheVolume(targetVol))
}')"

expect 1 "a rustSource running an exec of its own is refused" "$(dagger_dir ownexec 'func rustSource(base *dagger.Container, source *dagger.Directory, targetVol string) *dagger.Container {
	return base.
		WithMountedDirectory("/src", source).
		WithExec([]string{"bash", "-c", "true"}).
		WithMountedCache("/src/target", dag.CacheVolume(targetVol)).
		WithEnvVariable("FRAISEQL_STAMP_ROOT", "/src").
		WithEnvVariable("BASH_ENV", "/src/tools/ci-stamp-sources.sh")
}')"

expect 1 "a rustSource that does not set BASH_ENV is refused" "$(dagger_dir nobashenv 'func rustSource(base *dagger.Container, source *dagger.Directory, targetVol string) *dagger.Container {
	return base.
		WithMountedDirectory("/src", source).
		WithMountedCache("/src/target", dag.CacheVolume(targetVol)).
		WithEnvVariable("FRAISEQL_STAMP_ROOT", "/src")
}')"

expect 1 "a Go-side FRAISEQL_SOURCE_TOUCHED_AT is refused" "$(dagger_dir goside "${fixed_source}
func (m *FraiseqlCi) Leg() *dagger.Container {
	return m.rustSrc(nil).WithEnvVariable(\"FRAISEQL_SOURCE_TOUCHED_AT\", \"1\")
}")"

expect 1 "a direct cargo exec on a Rust leg is refused" "$(dagger_dir oneline "${fixed_source}
func (m *FraiseqlCi) Clippy() *dagger.Container {
	return m.rustSrc(nil).WithExec([]string{\"cargo\", \"clippy\"})
}")"

expect 1 "a direct cargo exec over two lines is refused" "$(dagger_dir twolines "${fixed_source}
func (m *FraiseqlCi) Clippy() *dagger.Container {
	return m.integrationBase(nil, \"\").
		WithExec([]string{
			\"cargo\", \"clippy\",
		})
}")"

expect 0 "the fixed rustSource, every leg exec through bash, passes" "$(dagger_dir fixed "${fixed_source}
func (m *FraiseqlCi) Clippy() *dagger.Container {
	return m.rustSrc(nil).WithExec([]string{\"bash\", \"-c\", \"cargo clippy\"})
}
func (m *FraiseqlCi) Fmt() *dagger.Container {
	return m.rustBase().WithExec([]string{\"cargo\", \"fmt\"})
}")"

expect 0 "the repository's own .dagger passes" "${repo_root}/.dagger"

if [[ "${failures}" -gt 0 ]]; then
    echo "ci_source_stamp_test: ${failures} failure(s)"
    exit 1
fi
echo "ci_source_stamp_test: all cases pass"
