package main

// ── Security & Compliance Gates ───────────────────────────────────────────────
//
// Ports the portable subset of security-compliance.yml onto Dagger so the gates
// run on every push(dev) again (Day-0 stripped their triggers to dispatch-only).
// One umbrella `dagger call security` runs:
//   - cargo-deny  (license-scan + dependency-audit jobs): `cargo deny check` over
//     licenses + advisories + bans + sources, governed by deny.toml.
//   - compliance  (compliance-check job): required-file + secret-pattern shell gates.
//
// NOT ported — these bind to GitHub infrastructure, same precedent as the plan
// keeping CodeQL GitHub-native (README "Out of scope"); they stay dispatch-only:
//   - container-security (Trivy 45-min image scan + SARIF upload to GH Code Scanning)
//   - dependency-review (GitHub Dependency-Graph API, PR-only)
// The TruffleHog secrets-scan job was in this list too. It was gated on a
// pull_request its workflow could not receive, so it had never run; it is deleted
// (#1206) and NOTHING replaces it yet — no secret scanner executes anywhere in CI.
// That is #1208, not a deferral this file can lean on.
// See parity-notes.md.

import (
	"context"
	"fmt"
	"strings"

	"dagger/fraiseql-ci/internal/dagger"
)

// denyVersion pins the prebuilt cargo-deny binary fetched into denyBase. Matches
// the local toolchain so `dagger call cargo-deny` and a developer's `cargo deny
// check` agree byte-for-byte (local==CI). (Later: pin by digest — parity-notes.md.)
const denyVersion = "0.19.0"

// Security runs every portable security/compliance gate in cheap-first, fail-fast
// order: the shell compliance checks (instant) before cargo-deny (advisory-db
// fetch + lockfile walk). The first failing gate aborts and its output is returned
// with the error. This is the umbrella the self-hosted `dagger-security.yml` calls;
// contributors can also target one gate (`dagger call cargo-deny --source=.`).
func (m *FraiseqlCi) Security(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	gates := []struct {
		name string
		run  func(context.Context, *dagger.Directory) (string, error)
	}{
		{"compliance", m.Compliance},
		{"crypto-providers", m.CryptoProviders},
		{"advisory-paths", m.AdvisoryPaths},
		{"default-build-minimums", m.DefaultBuildMinimums},
		{"cargo-deny", m.CargoDeny},
		{"cargo-audit", m.CargoAudit},
	}

	var report strings.Builder
	for _, g := range gates {
		out, err := g.run(ctx, source)
		fmt.Fprintf(&report, "\n===== %s =====\n%s\n", g.name, out)
		if err != nil {
			return report.String(), fmt.Errorf("security gate %q failed: %w", g.name, err)
		}
	}
	report.WriteString("\nsecurity OK: all gates passed\n")
	return report.String(), nil
}

// CargoDeny mirrors security-compliance.yml's license-scan + dependency-audit jobs:
// `cargo deny check` over licenses, advisories, bans, and sources, governed by
// deny.toml (which sets `[graph] all-features = true`, so every feature path is
// considered). cargo-deny shells out to `cargo metadata` to resolve the dependency
// graph, so cargo must be on PATH — but nothing compiles. Advisory data is fetched
// into a persistent cache volume so re-runs only pull the incremental RustSec delta.
func (m *FraiseqlCi) CargoDeny(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.denyBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		// -D unmatched-skip-root / -D unmatched-skip: both lints default to WARN, so a
		// [[bans.skip-tree]] or [[bans.skip]] entry whose exact-version pin has gone
		// stale covers NOTHING and this leg still exits 0 (#1020; the #933 duplicate
		// storm is what a missed pin looks like). tools/check-deny-lint-flags.sh keeps
		// the flags in lockstep with the Makefile and security-compliance.yml.
		WithExec([]string{
			"cargo-deny", "check",
			"-D", "unmatched-skip-root",
			"-D", "unmatched-skip",
		}).
		Stdout(ctx)
}

// CargoAudit runs `cargo audit` over Cargo.lock, governed by .cargo/audit.toml
// (kept in lockstep with deny.toml by tools/check-audit-lockstep.sh). It closes
// the gap where Dagger ran cargo-deny but never cargo-audit, so `make audit`
// could disagree with CI. Runs on denyBase (cargo on PATH + the persistent
// RustSec advisory-db cache); cargo-audit is installed from crates.io. Nothing
// in the workspace compiles — only the lockfile is scanned.
func (m *FraiseqlCi) CargoAudit(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.denyBase().
		WithExec([]string{"cargo", "install", "cargo-audit", "--locked"}).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"cargo", "audit"}).
		Stdout(ctx)
}

// CryptoProviders runs tools/check-crypto-providers.sh: the default fraiseql-server
// build must link exactly one rustls crypto provider (ring) and one rustls major
// (M-dual-crypto). Runs on denyBase — `cargo tree` needs only `cargo metadata`
// (cargo on PATH + the warm registry cache), nothing compiles.
func (m *FraiseqlCi) CryptoProviders(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.denyBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "tools/check-crypto-providers.sh"}).
		Stdout(ctx)
}

// AdvisoryPaths runs tools/check-advisory-paths.sh: every accepted advisory in
// docs/dependency-risk-policy.md declares how the vulnerable crate is reached
// (default-build / feature-gated:<f> / not-compiled), and the gate checks that claim
// against `cargo tree` rather than trusting the prose.
//
// This exists because three of the eight acceptances were carried by sentences the
// dependency graph contradicts — RUSTSEC-2023-0071 named `sqlx-mysql`, removed with
// #374 (#1110); RUSTSEC-2025-0134 claimed dev-dependency-only; RUSTSEC-2026-0204
// claimed criterion dev-deps (#1137). check-audit-lockstep.sh compares advisory IDS
// and both machine-read files carried the same wrong story, and `cargo deny` has no
// opinion about whether a `reason` string is true.
//
// It lives here rather than in ShellGates because it needs cargo. Like
// CryptoProviders it runs on denyBase — `cargo tree` needs only `cargo metadata`
// (cargo on PATH plus the warm registry cache); nothing compiles.
func (m *FraiseqlCi) AdvisoryPaths(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.denyBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "tools/check-advisory-paths.sh"}).
		Stdout(ctx)
}

// DefaultBuildMinimums runs tools/check-default-build-minimums.sh: crates in the
// DEFAULT build must not fall below a version floor we have committed to.
//
// This exists because cargo-deny cannot scope an advisory ignore to one crate
// version — `[advisories] ignore` takes only `id` and `reason`, and a `crate = "…"`
// key there is a yanked-crate ignore that suppresses no vulnerability. So when one
// advisory matches two instances of a crate and only one is acceptable, ignoring it
// by id silences both.
//
// RUSTSEC-2026-0258 is that case: h2 0.3.27 (opt-in aws-*, no fix in the 0.3 series)
// is accepted in deny.toml, while h2 0.4.15 was in the default build under
// hyper/axum — the GraphQL listener, where the DoS is remotely triggerable — and was
// bumped to 0.4.16 instead. Verified: with 0.4.15 restored, `cargo deny check
// advisories` reports "advisories ok" and only this gate fails.
//
// Runs on denyBase like CryptoProviders and AdvisoryPaths — `cargo tree` needs only
// `cargo metadata` (cargo on PATH plus the warm registry cache); nothing compiles.
func (m *FraiseqlCi) DefaultBuildMinimums(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.denyBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "tools/check-default-build-minimums.sh"}).
		Stdout(ctx)
}

// Compliance mirrors security-compliance.yml's compliance-check job: required
// security/policy files must exist (hard fail), plus two advisory greps (nginx
// security headers, hardcoded-secret patterns) that warn but never fail. They were
// advisory because TruffleHog was the authoritative secret gate; that job could not
// run and is gone (#1206), so this warn-only grep is currently the ONLY secret check
// in CI and it blocks nothing. #1208.
// Pure shell, so it runs on the lightweight shellBase, not the Rust container.
func (m *FraiseqlCi) Compliance(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	script := strings.Join([]string{
		"set -e",
		// Hard gate: required security & compliance files must be present.
		`for f in SECURITY.md LICENSE CODE_OF_CONDUCT.md; do`,
		`  if [ ! -f "$f" ]; then echo "❌ Required file $f is missing"; exit 1; fi`,
		`done`,
		`echo "✅ All required security and compliance files present"`,
		// Advisory: security headers in the shipped nginx config (warn only).
		`if grep -q "add_header X-Frame-Options" deploy/nginx-fraiseql.conf; then`,
		`  echo "✅ Security headers found in nginx config"`,
		`else`,
		`  echo "⚠️  Security headers not found in nginx config"`,
		`fi`,
		// Advisory: scan source/config for hardcoded secret patterns (warn only;
		// legitimate test/example uses excluded). Verbatim from the legacy job.
		`POTENTIAL_SECRETS=$(grep -rn --include="*.rs" --include="*.toml" --include="*.yml" --include="*.yaml" \`,
		`  -i "password\s*=\s*\"\|secret\s*=\s*\"\|token\s*=\s*\"" \`,
		`  crates/ .github/ \`,
		`  | grep -v "# " \`,
		`  | grep -v "test\|example\|fixture\|mock\|dummy\|placeholder\|changeme\|your_" \`,
		`  || true)`,
		`if [ -n "$POTENTIAL_SECRETS" ]; then`,
		`  echo "⚠️  Potential hardcoded secrets found (manual review needed):"`,
		`  echo "$POTENTIAL_SECRETS"`,
		`else`,
		`  echo "✅ No hardcoded secrets found in source code"`,
		`fi`,
	}, "\n")

	return m.shellBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// denyBase is the container for cargo-deny: the pinned MSRV rust image (cargo on
// PATH for `cargo metadata`, plus git/curl from its buildpack-deps base) with the
// prebuilt cargo-deny binary dropped in. Nothing compiles, so it skips rustBase's
// mold/clang/sccache/native-dep layers. It shares the warm cargo registry cache for
// a fast metadata resolve, and persists the RustSec advisory database (deny.toml
// db-path `~/.cargo/advisory-db`, expanded against HOME=/root) in its own cache
// volume so re-runs skip the cold advisory-db clone.
func (m *FraiseqlCi) denyBase() *dagger.Container {
	const cargoHome = "/usr/local/cargo"
	installDeny := strings.Join([]string{
		"set -euo pipefail",
		"base=cargo-deny-" + denyVersion + "-x86_64-unknown-linux-musl",
		"url=https://github.com/EmbarkStudios/cargo-deny/releases/download/" + denyVersion + "/${base}.tar.gz",
		"curl -fsSL \"$url\" -o /tmp/cargo-deny.tgz",
		"tar -xzf /tmp/cargo-deny.tgz -C /tmp",
		"install -m0755 /tmp/${base}/cargo-deny /usr/local/bin/cargo-deny",
		"rm -rf /tmp/cargo-deny.tgz /tmp/${base}",
		"cargo-deny --version",
	}, "\n")

	return dag.Container().
		From(rustImage).
		WithExec([]string{"bash", "-c", installDeny}).
		WithEnvVariable("CARGO_TERM_COLOR", "always").
		WithEnvVariable("HOME", "/root").
		WithMountedCache(cargoHome+"/registry", dag.CacheVolume("fraiseql-cargo-registry")).
		WithMountedCache("/root/.cargo/advisory-db", dag.CacheVolume("fraiseql-advisory-db"))
}
