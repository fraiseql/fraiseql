package main

// ── Release Pre-Tag Validation (Phase 06, minimal scope) ──────────────────────
//
// Local pre-tag validation for the crates.io release, runnable the same way
// locally (`dagger call publish-dry-run`) and — optionally, later — on the runner.
//
// The actual publish + image build/push stay GitHub-native (release.yml /
// docker-build.yml, both still live on `v*` tags): releases are infrequent, so
// their hosted-CI cost is negligible and re-implementing a working crates.io
// publisher is not worth the risk. What this module adds is the ability to catch,
// BEFORE a tag goes out, the two failure classes a release run would otherwise
// discover the expensive way:
//   - "tag shipped but a crate failed to publish" (the v2.3.0 / v2.3.1 regression)
//     → PublishDryRun packages and verifies every crate release.yml publishes.
//   - an accidental breaking public-API change → SemverNamed / SemverWorkspace.
//

import (
	"context"
	"fmt"
	"strings"

	"dagger/fraiseql-ci/internal/dagger"
)

// PublishDryRun packages and verify-builds every publishable crate, as release.yml's
// "Package and verify every publishable crate" step does before anything is
// published. It catches the packaging regressions (gitignored files or build.rs side
// effects in the .crate tarball) that shipped v2.3.0/v2.3.1 tags whose publish then
// failed.
//
// The crates come from tools/publish-order.py, which reads them off the workspace.
// `cargo package` resolves each workspace sibling from the packages it has just built,
// not from crates.io, so a crate whose new sibling version is not published yet
// verifies like any other: no failure is forgiven. No token is needed and nothing is
// uploaded, so this is safe to run before tagging.
//
// Run it after the version bump, as tools/release.sh does. At a version already on
// crates.io, every crate is packaged beside a published copy with another checksum,
// and fraiseql-server, whose tests lock fraiseql-cli, fails with "checksum for
// `fraiseql-cli` changed between lock files" even on the tagged tree.
//
// expectVersion, when set, asserts the workspace `[workspace.package]` version
// matches first — a guard against tagging vX while Cargo.toml still says vY.
func (m *FraiseqlCi) PublishDryRun(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// +optional
	expectVersion string,
) (string, error) {
	script := strings.Join([]string{
		"set -euo pipefail",
		fmt.Sprintf("EXPECT=%q", expectVersion),
		`if [ -n "$EXPECT" ]; then`,
		`  ACTUAL=$(grep -m1 -E '^version[[:space:]]*=' Cargo.toml | sed -E 's/.*"([^"]*)".*/\1/')`,
		`  if [ "$ACTUAL" != "$EXPECT" ]; then`,
		`    echo "❌ version mismatch: [workspace.package] version is $ACTUAL, expected $EXPECT"; exit 1`,
		`  fi`,
		`  echo "✅ workspace version $ACTUAL matches expected"`,
		`fi`,
		`ORDER="$(python3 tools/publish-order.py)"`,
		`args=()`,
		`while read -r crate; do args+=(--package "$crate"); done <<<"$ORDER"`,
		`cargo package "${args[@]}"`,
		`echo "✅ publish-dry-run OK: $(wc -l <<<"$ORDER") crates packaged and verified"`,
	}, "\n")

	return m.rustSrc(source).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// semverChecksVersion pins the prebuilt cargo-semver-checks binary. cargo-semver-checks
// generates rustdoc JSON for the current and baseline APIs and diffs them; it runs on
// the pinned stable toolchain (it unlocks the otherwise-nightly rustdoc JSON via
// RUSTC_BOOTSTRAP internally), matching how ci.yml / semver.yml run it on stable.
const semverChecksVersion = "0.48.0"

// semverBase is rustBase with the prebuilt cargo-semver-checks binary dropped in.
// It needs the full Rust build toolchain (semver-checks compiles rustdoc for every
// crate + its baseline), so it builds on rustBase rather than the minimal denyBase.
func (m *FraiseqlCi) semverBase() *dagger.Container {
	install := strings.Join([]string{
		"set -euo pipefail",
		"base=cargo-semver-checks-x86_64-unknown-linux-musl",
		"url=https://github.com/obi1kenobi/cargo-semver-checks/releases/download/v" +
			semverChecksVersion + "/${base}.tar.gz",
		"curl -fsSL \"$url\" -o /tmp/csc.tgz",
		"tar -xzf /tmp/csc.tgz -C /tmp",
		// cargo-dist tarballs vary (binary at root vs in a target-named subdir) — find it.
		"bin=$(find /tmp -type f -name cargo-semver-checks | head -1)",
		"install -m0755 \"$bin\" /usr/local/bin/cargo-semver-checks",
		"rm -rf /tmp/csc.tgz",
		"cargo-semver-checks --version",
	}, "\n")
	return m.rustBase().WithExec([]string{"bash", "-c", install})
}

// namedSemverCrates mirrors ci.yml's per-PR "API Semver Compatibility" job — the
// five crates whose public API is the load-bearing surface.
var namedSemverCrates = []string{
	"fraiseql-error", "fraiseql-db", "fraiseql-core", "fraiseql-server", "fraiseql-cli",
}

// SemverNamed runs `cargo semver-checks check-release -p <crate>` for the five
// named crates, mirroring ci.yml's per-PR semver job — advisory and NON-gating
// (the legacy job is `|| true`): it reports findings but never fails the call, so
// it is a fast pre-tag smoke that surfaces obvious breakage without blocking.
//
// The source keeps `.git` (unlike the other functions) — cargo-semver-checks needs
// git history to materialise the `--baseline-rev` baseline. baselineRev defaults to
// HEAD~1 when unset.
func (m *FraiseqlCi) SemverNamed(
	ctx context.Context,
	// +ignore=["target", "**/target"]
	source *dagger.Directory,
	// +optional
	baselineRev string,
) (string, error) {
	if baselineRev == "" {
		baselineRev = "HEAD~1"
	}
	script := strings.Join([]string{
		"set -uo pipefail",
		fmt.Sprintf("BASELINE=%q", baselineRev),
		fmt.Sprintf("for c in %s; do", strings.Join(namedSemverCrates, " ")),
		`  echo "===== semver-checks (advisory): $c vs $BASELINE ====="`,
		// 2>&1: cargo-semver-checks writes its report (Checking/Summary/findings)
		// to stderr, so fold it into stdout to make the report visible in the result.
		`  cargo semver-checks check-release -p "$c" --baseline-rev "$BASELINE" 2>&1 || \`,
		`    echo "↑ findings for $c — advisory only, not gating (mirrors ci.yml || true)"`,
		"done",
		`echo "✅ semver-named complete (advisory; review findings above)"`,
	}, "\n")

	return rustSource(m.semverBase(), source, "fraiseql-rust-target").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// SemverWorkspace runs the gating workspace-wide check, mirroring semver.yml:
// `cargo semver-checks check-release --workspace --exclude fraiseql-test-utils
// --baseline-rev <rev>`. Unlike SemverNamed this is GATING — a detected breaking
// change fails the call. baselineRev defaults to HEAD~1 when unset.
func (m *FraiseqlCi) SemverWorkspace(
	ctx context.Context,
	// +ignore=["target", "**/target"]
	source *dagger.Directory,
	// +optional
	baselineRev string,
) (string, error) {
	if baselineRev == "" {
		baselineRev = "HEAD~1"
	}
	// 2>&1 folds cargo-semver-checks' stderr report into stdout so it is visible
	// in the returned string on success; on a detected breaking change the command
	// exits non-zero and Dagger surfaces the same output in the gating error.
	script := fmt.Sprintf(
		"cargo semver-checks check-release --workspace --exclude fraiseql-test-utils --baseline-rev %q 2>&1",
		baselineRev)
	return rustSource(m.semverBase(), source, "fraiseql-rust-target").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}
