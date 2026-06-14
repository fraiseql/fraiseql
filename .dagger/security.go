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
//   - secrets-scan      (TruffleHog, scans the PR diff via GH event SHAs)
//   - container-security (Trivy 45-min image scan + SARIF upload to GH Code Scanning)
//   - dependency-review (GitHub Dependency-Graph API, PR-only)
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
		WithExec([]string{"cargo-deny", "check"}).
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

// Compliance mirrors security-compliance.yml's compliance-check job: required
// security/policy files must exist (hard fail), plus two advisory greps (nginx
// security headers, hardcoded-secret patterns) that warn but never fail — the
// TruffleHog secrets-scan is the authoritative secret gate and stays GitHub-native.
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
