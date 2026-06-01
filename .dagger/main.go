// Package main is the FraiseQL CI Dagger module.
//
// It hosts the self-hosted CI pipeline that replaces the GitHub-hosted workflows
// (Track 0, see .phases/dagger-adoption/). Phase 01 ports the smallest gate:
// the axum `:param` route-syntax check (issue #316).
package main

import (
	"context"
	"fmt"

	"dagger/fraiseql-ci/internal/dagger"
)

// FraiseqlCi is the FraiseQL CI module root.
type FraiseqlCi struct{}

// LintRoutes fails if any axum 0.7-style `:param` route capture remains in the
// source tree, mirroring tools/check-route-syntax.sh (issue #316). It replaces the
// GitHub-hosted `axum-route-syntax-check` job.
//
// The script runs verbatim inside a pinned container; we only add a throwaway
// `git init` so the script's `cd "$(git rev-parse --show-toplevel)"` resolves to the
// mounted tree, and we use gawk (not Ubuntu's default mawk) so the load-bearing
// multi-line `\s` awk pass actually matches. See parity-notes.md.
//
// The `+ignore` directive keeps the 277 GB / 450k-file `target/` tree and the `.git`
// dir off the upload entirely (the script reads neither — it scans crates/ and
// examples/ and excludes */target/*). This also makes a local `dagger call --source=.`
// behave like the legacy job's clean checkout.
func (m *FraiseqlCi) LintRoutes(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	// Belt-and-suspenders: drop build/VCS dirs server-side too, so the function is
	// correct even when invoked with a source that bypassed the +ignore upload filter.
	src := source.
		WithoutDirectory("target").
		WithoutDirectory(".git")

	return m.lintBase().
		WithMountedDirectory("/src", src).
		WithWorkdir("/src").
		WithExec([]string{
			"bash", "-c",
			"git init -q . >/dev/null && bash tools/check-route-syntax.sh",
		}).
		Stdout(ctx)
}

// LintRoutesSelftest proves the gate actually fails on a bad route, by overlaying a
// synthetic multi-line `:param` capture onto the source and asserting LintRoutes
// returns non-zero. Returns success (exit 0) only when the gate correctly flags it.
//
// This replaces the plan's static .dagger/testdata/bad-route/ tree, which cannot work
// with the verbatim script (the script greps crates//examples/ and runs
// tools/check-route-syntax.sh from one git toplevel, so a standalone fixture dir would
// lack the script and fail for the wrong reason). See parity-notes.md.
func (m *FraiseqlCi) LintRoutesSelftest(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	const badRoute = `// synthetic fixture injected by LintRoutesSelftest — not committed
pub fn __lint_routes_fixture() {
    router.route(
        "/checkpoint/:listener_id",
        get(handler),
    );
}
`
	bad := source.WithNewFile("crates/fraiseql-core/src/__lint_routes_fixture.rs", badRoute)

	out, err := m.LintRoutes(ctx, bad)
	if err == nil {
		return "", fmt.Errorf("lint-routes selftest FAILED: gate did not flag an injected :param route:\n%s", out)
	}
	return "lint-routes selftest OK: injected :param route was correctly flagged", nil
}

// lintBase returns a minimal Ubuntu container carrying exactly the tools
// check-route-syntax.sh needs: bash, git, gawk, grep, findutils.
// (Later: pin ubuntu by digest and cache the apt layer once the Phase-02 rustBase
// cache strategy exists.)
func (m *FraiseqlCi) lintBase() *dagger.Container {
	return dag.Container().
		From("ubuntu:24.04").
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{
			"apt-get", "install", "-y", "--no-install-recommends",
			"git", "gawk", "findutils", "grep", "ca-certificates",
		})
}
