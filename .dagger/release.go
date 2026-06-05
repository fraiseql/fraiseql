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
//     → PublishDryRun walks the same topological order as release.yml.
//   - an accidental breaking public-API change → SemverNamed / SemverWorkspace.
//
// See .phases/dagger-adoption/parity-notes.md for the ported-vs-GitHub-native split.

import (
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"strings"

	"dagger/fraiseql-ci/internal/dagger"
)

// legacyPublishOrder is the topological crate publish order proven by release.yml
// (it shipped all 16 publishable crates at v2.3.2). It is the canonical order the
// dry-run loop iterates; PublishOrderSelftest validates it against the live
// dependency graph, so a newly-added crate or a new cross-crate edge that would
// break publish order is caught here rather than mid-release.
var legacyPublishOrder = []string{
	// Tier 1: leaf crates (depend on none of the others).
	"fraiseql-error", "fraiseql-auth", "fraiseql-webhooks", "fraiseql-wire",
	// Tier 2: depend on tier 1.
	"fraiseql-db", "fraiseql-storage",
	// Tier 3.
	"fraiseql-federation",
	// Tier 4.
	"fraiseql-core",
	// Tier 4.5.
	"fraiseql-codegen",
	// Tier 5.
	"fraiseql-arrow", "fraiseql-secrets", "fraiseql-observers", "fraiseql-functions",
	// Tier 6: top-level binaries + umbrella.
	"fraiseql-server", "fraiseql-cli", "fraiseql",
}

// PublishOrder returns the canonical crates.io publish order, one crate per line —
// the order PublishDryRun (and the legacy release.yml) publish in.
func (m *FraiseqlCi) PublishOrder() string {
	return strings.Join(legacyPublishOrder, "\n")
}

// cargoMetadata is the subset of `cargo metadata --no-deps` output the selftest reads.
type cargoMetadata struct {
	Packages []cargoPackage `json:"packages"`
}

type cargoPackage struct {
	Name string `json:"name"`
	// Publish: null/absent → publishable; [] → publish=false; ["registry"] → restricted.
	Publish      *[]string         `json:"publish"`
	Dependencies []cargoDependency `json:"dependencies"`
}

type cargoDependency struct {
	Name string `json:"name"`
	// Kind: null → normal, "dev", or "build".
	Kind *string `json:"kind"`
}

func (p cargoPackage) publishable() bool {
	return p.Publish == nil || len(*p.Publish) > 0
}

// PublishOrderSelftest validates legacyPublishOrder against the live workspace
// dependency graph (`cargo metadata --no-deps`):
//
//  1. Set equality — the publishable crates the workspace actually has match the
//     embedded order exactly. A newly-added publishable crate, or a removed one,
//     fails here with the diff.
//  2. Topological validity — every crate is listed after all of its in-workspace
//     (non-dev) dependencies. A new cross-crate edge that violates publish order
//     fails here.
//
// This is the Phase-06 CLEANUP gate ("publish order matches release.yml"): the
// embedded order stays canonical (it is what release.yml proved), and this guards
// it against drift.
func (m *FraiseqlCi) PublishOrderSelftest(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	out, err := m.rustBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"cargo", "metadata", "--no-deps", "--format-version", "1"}).
		Stdout(ctx)
	if err != nil {
		return "", fmt.Errorf("cargo metadata failed: %w", err)
	}

	var meta cargoMetadata
	if err := json.Unmarshal([]byte(out), &meta); err != nil {
		return "", fmt.Errorf("parsing cargo metadata JSON: %w", err)
	}

	pos := make(map[string]int, len(legacyPublishOrder))
	for i, name := range legacyPublishOrder {
		pos[name] = i
	}

	// (1) Set equality: live publishable crates vs the embedded order.
	livePublishable := make(map[string]bool)
	for _, pkg := range meta.Packages {
		if pkg.publishable() {
			livePublishable[pkg.Name] = true
		}
	}
	var missing, extra []string
	for name := range livePublishable {
		if _, ok := pos[name]; !ok {
			missing = append(missing, name)
		}
	}
	for _, name := range legacyPublishOrder {
		if !livePublishable[name] {
			extra = append(extra, name)
		}
	}
	sort.Strings(missing)
	sort.Strings(extra)
	if len(missing) > 0 || len(extra) > 0 {
		return "", fmt.Errorf(
			"publish-order drift vs `cargo metadata`:\n"+
				"  missing from legacyPublishOrder (add them in dependency order): %v\n"+
				"  listed but no longer a publishable workspace crate (remove them): %v",
			missing, extra)
	}

	// (2) Topological validity: each crate after its in-workspace non-dev deps.
	var violations []string
	for _, pkg := range meta.Packages {
		if !pkg.publishable() {
			continue
		}
		for _, dep := range pkg.Dependencies {
			if dep.Kind != nil && *dep.Kind == "dev" {
				continue // dev-deps do not constrain publish order
			}
			if !livePublishable[dep.Name] {
				continue // external or non-publishable dependency
			}
			if pos[dep.Name] >= pos[pkg.Name] {
				violations = append(violations, fmt.Sprintf(
					"%s is ordered before its dependency %s", pkg.Name, dep.Name))
			}
		}
	}
	sort.Strings(violations)
	if len(violations) > 0 {
		return "", fmt.Errorf("publish order is not topologically valid:\n  %s",
			strings.Join(violations, "\n  "))
	}

	return fmt.Sprintf("publish-order OK: %d publishable crates, valid topological order\n  %s\n",
		len(legacyPublishOrder), strings.Join(legacyPublishOrder, " → ")), nil
}

// PublishDryRun runs `cargo publish --dry-run` for every publishable crate in
// publish order, mirroring release.yml's "Dry-run publish for every publishable
// crate" gate — the upstream check added after v2.3.0/v2.3.1 shipped tags whose
// publish then failed (packaging-rule regressions: gitignored files or build.rs
// side effects swept into the .crate tarball). Like the legacy gate it does NOT
// fail fast: it dry-runs all crates, prints the tail of any that fail, and exits
// non-zero at the end if any failed, so one run surfaces every packaging problem.
//
// No registry token is needed — `--dry-run` packages and verify-builds but never
// contacts the upload endpoint — so this is safe to run locally before tagging.
//
// The verify build resolves each crate's dependencies from crates.io, NOT from the
// local workspace paths. So during a synchronized version bump the not-yet-published
// new sibling versions are unresolvable, and a crate that depends on one fails its
// dry-run with "no matching package named X" / "failed to select a version for X".
// That is expected and benign — the ordered publish ships those siblings first — so
// dry_run_failure_is_tolerable (tools/lib/, unit-tested) downgrades it to a WARN when
// every unresolved package is one of our own publishable crates and the failure is a
// packaging-prep failure, not a compile error. Everything else still hard-fails. This
// matches release.yml's tolerance (v2.4.0 hit exactly this with fraiseql-codegen and
// the synchronized 2.4.0 bump).
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
		"set -uo pipefail",
		// Plain (uncoloured) cargo output so the tolerance greps match reliably.
		"export CARGO_TERM_COLOR=never",
		// First-publish-sibling tolerance lives in a tested shell lib in the source.
		"source tools/lib/dry_run_tolerance.sh",
		fmt.Sprintf("EXPECT=%q", expectVersion),
		`if [ -n "$EXPECT" ]; then`,
		`  ACTUAL=$(grep -m1 -E '^version[[:space:]]*=' Cargo.toml | sed -E 's/.*"([^"]*)".*/\1/')`,
		`  if [ "$ACTUAL" != "$EXPECT" ]; then`,
		`    echo "❌ version mismatch: [workspace.package] version is $ACTUAL, expected $EXPECT"; exit 1`,
		`  fi`,
		`  echo "✅ workspace version $ACTUAL matches expected"`,
		`fi`,
		fmt.Sprintf("CRATES=%q", strings.Join(legacyPublishOrder, " ")),
		"FAILED=0",
		`for c in $CRATES; do`,
		`  echo "===== dry-run publish: $c ====="`,
		`  log="/tmp/dryrun-$c.log"`,
		`  cargo publish --dry-run -p "$c" > "$log" 2>&1`,
		`  status=$?`,
		`  if [ "$status" -eq 0 ]; then`,
		`    echo "OK: $c"`,
		`  elif tol=$(dry_run_failure_is_tolerable "$log" "$CRATES"); then`,
		`    echo "WARN: $c tolerated — not-yet-published sibling(s): $(echo $tol | tr '\n' ' ')"`,
		`  else`,
		`    echo "FAIL: $c (exit $status)"`,
		`    tail -40 "$log"`,
		`    FAILED=1`,
		`  fi`,
		"done",
		`if [ "$FAILED" -ne 0 ]; then echo "❌ one or more crates failed dry-run publish"; exit 1; fi`,
		fmt.Sprintf(`echo "✅ publish-dry-run OK: all %d crates passed (first-publish siblings tolerated)"`, len(legacyPublishOrder)),
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

	return m.semverBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume("fraiseql-rust-target")).
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
	return m.semverBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume("fraiseql-rust-target")).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}
