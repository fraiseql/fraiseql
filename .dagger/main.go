// Package main is the FraiseQL CI Dagger module.
//
// It hosts the self-hosted CI pipeline that replaces the GitHub-hosted workflows
// (Track 0). Phase 01 ports the smallest gate:
// the axum `:param` route-syntax check (issue #316).
package main

import (
	"context"
	"fmt"
	"strings"

	"dagger/fraiseql-ci/internal/dagger"
)

// FraiseqlCi is the FraiseQL CI module root.
type FraiseqlCi struct{}

const (
	// rustImage pins the toolchain to the workspace MSRV (rust-toolchain.toml channel = 1.94.1).
	// The PATCH is pinned deliberately: docker.io/library/rust:1.94 tracks the newest 1.94.x, so a
	// floating tag silently stops testing the rust-version we declare the day 1.94.2 ships.
	// The default (non-slim) variant is buildpack-deps-based, so gcc/perl/curl/git are present.
	// (Later: pin by digest — see docs/contributing/dagger-parity-notes.md Phase 02.)
	//
	// Pulled from ghcr.io/fraiseql/* (mirrored by .github/workflows/mirror-base-images.yml),
	// NOT Docker Hub: the self-hosted runner shares one Docker Hub account whose pull-rate
	// limit periodically 429s every leg. The ghcr mirrors are public, so the engine pulls
	// them anonymously. Every ghcr.io/fraiseql/* tag below MUST have a matching entry in that
	// workflow's IMAGES list. (mcr.microsoft.com/* and the Apollo router stay as-is — not
	// Docker Hub, not rate-limited.)
	rustImage = "ghcr.io/fraiseql/rust:1.94.1"
	// ubuntuImage backs shellBase (the toolchain-free shell-gate container).
	ubuntuImage = "ghcr.io/fraiseql/ubuntu:24.04"
	// unwrapAllowLimit: the ShellGates `make lint-unwrap` budget (see Makefile).
	unwrapAllowLimit = "3"
	// sccacheVersion pins the prebuilt sccache binary fetched into rustBase.
	sccacheVersion = "v0.8.2"
	// rustMsrv mirrors Cargo.toml workspace rust-version and rust-toolchain.toml channel.
	rustMsrv = "1.94.1"

	// SYNC:* feature sets — this file is the single authority since the legacy
	// ci.yml was retired (#951); the SYNC tags mark every use site in this file.
	coreTestFeatures = "arrow,audit-syslog,audit-webhook,federation,kafka,postgres,redis-apq,schema-lint,test-utils,wire-backend"
	dbTestFeatures   = "postgres,wire-backend"
	// redis-pkce + redis-rate-limiting are compiled in so the #770/#777 boot-guard
	// lib tests run here (they need no live Redis — closed ports and URL parsing);
	// the Redis-requiring tests stay #[ignore]d and run in the redis integration leg.
	serverTestFeatures = "arrow,auth,aws-s3,federation,grpc,mcp,metrics,observers,redis-apq,redis-pkce,redis-rate-limiting,rest,secrets,storage-transforms,testing,tracing-opentelemetry,webhooks,wire-backend"
	// testWorkspaceSkip: the skip patterns the `Test` shard's workspace sweep
	// carries — the testcontainers lib tests (storage + functions); wire's
	// container tests live in tests/*, so only its lib unit tests run.
	// `uploads::tests` (#369) joins the list for the same reason: it needs a real
	// Postgres for the resumable-upload session table, and without DATABASE_URL
	// the harness spawns a testcontainer, which this engine has no Docker for.
	// It runs for real in the `storage` integration suite.
	//
	// A package const rather than a local in `Test`: `make test-leg` mirrors that
	// invocation, and tools/check-shard-parity.py resolves consts but reports an
	// unresolvable expression as fatal rather than comparing a command it could
	// not read.
	testWorkspaceSkip = "-- --skip metadata::tests --skip migrations::tests --skip routes::tests --skip uploads::tests"
	// serverInProcessTests: every fraiseql-server tests/*.rs binary that runs
	// in-process (no backing service) and is not already named by a dedicated
	// line. Enumerated because fraiseql-server is excluded from the workspace
	// run and DB-backed binaries must NOT run here (they belong to the
	// integration legs, loud-panic on a missing DATABASE_URL by contract).
	// The suite-coverage gate (tools/check-suite-coverage.py, preflight)
	// fails when a new binary lands in no leg, so this list cannot rot (#992).
	serverInProcessTests = " --test admin_api_security_test --test admin_authz_test --test admin_cache_vocabulary_e2e --test admission_control_test --test api_admin_tests --test api_design_audit_tests --test api_design_security_tests --test api_federation_tests --test api_infrastructure_tests --test api_openapi_tests --test api_query_tests --test api_schema_tests --test apq_mutation_e2e_test --test auth_me_integration_test --test auth_regression_test --test backpressure_overload_test --test backpressure_test --test cache_wiring_tests --test changelog_cascade_compose_boot --test config_struct_test --test constructor_drift_test --test endpoint_health_tests --test error_handling_validation_test --test example_validation_test --test federation_saga_validation_test --test functions_platform_pins_test --test graphql_http_layer_test --test graphql_request_validation_test --test grpc_transport_e2e_test --test introspection_gate_e2e_test --test introspection_mutation_authz_test --test introspection_security_test --test metrics_facade_scrape --test metrics_integration_test --test multitenancy_test --test observability_test --test platform_e2e_test --test production_safety_test --test profile_error_redaction_test --test profile_query_limits_test --test rate_limiting_integration_test --test rate_limit_sweep_is_scheduled_test --test rest_transport_e2e_test --test security --test property --test security_config_runtime_test --test security_stack_integration_test --test service_account_conformance_test --test studio_admin_api_test --test studio_auth_users_test --test studio_data_browser_test --test studio_e2e_test --test studio_functions_test --test studio_metrics_test --test studio_shell_test --test studio_storage_test --test tracing_integration_test --test typename_e2e_test --test v230_integration_tests"
)

// suiteCountPrelude makes a shard's per-suite test counts readable from its log.
//
// The program's rule 2 asks that a real-system test be "verified executing (nonzero
// counts in the leg log)". On the `integration (server)` shard that check could not be
// performed: the log carried the echoed script source — Dagger prints every `withExec`
// command — and the shard's final OK marker, but not one `test result: N passed` line
// from the suites themselves (#1124). The `set -e` chain proves every `cargo test`
// exited 0; what it cannot distinguish is a suite that compiled to ZERO tests and
// exited 0, which is exactly the #1082 failure mode this program just fixed one
// instance of.
//
// ⚠ Two traps this has to design around, both cases where a grep matched the harness
// rather than its output:
//
//  1. Grepping the log for a suite name returns ~99 hits that are ALL the echoed
//     script. Any marker written as a literal in the command list has the same problem:
//     it appears in the source Dagger prints, whether or not it ever ran.
//  2. In the feature-matrix leg, grepping for `COMBO-RESULT <combo>: FAIL` matched the
//     script source, which contains both the OK and the FAIL branch literals.
//
// So the marker is CONSTRUCTED AT RUNTIME. The prelude below defines a shell function
// named `cargo`, which shadows the binary for every command in the list — so all ~60
// existing `cargo test …` lines are wrapped with no change to any of them — and prints
//
//	SUITE-RAN <args> :: test result: ok. N passed; …
//
// via printf with `%s` placeholders. The format string in the echoed source therefore
// never matches a search for a resolved line, and `grep '^SUITE-RAN'` on the log yields
// exactly the suites that actually executed, with their counts.
const suiteCountPrelude = `cargo() {
  local out rc
  out="$(command cargo "$@" 2>&1)"; rc=$?
  printf '%s\n' "$out"
  printf 'SUITE-RAN %s :: %s\n' "$*" "$(printf '%s' "$out" | grep '^test result:' | tr '\n' '|')"
  return $rc
}`

// LintRoutes fails if any axum 0.7-style `:param` route capture remains in the
// source tree, mirroring tools/check-route-syntax.sh (issue #316). It replaces the
// GitHub-hosted `axum-route-syntax-check` job.
//
// The script runs verbatim inside a pinned container; we only add a throwaway
// `git init` so the script's `cd "$(git rev-parse --show-toplevel)"` resolves to the
// mounted tree, and we use gawk (not Ubuntu's default mawk) so the load-bearing
// multi-line `\s` awk pass actually matches. See docs/contributing/dagger-parity-notes.md.
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
// lack the script and fail for the wrong reason). See docs/contributing/dagger-parity-notes.md.
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
		From(ubuntuImage).
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{
			"apt-get", "install", "-y", "--no-install-recommends",
			"git", "gawk", "findutils", "grep", "ca-certificates",
		})
}

// ── Phase 02: Fast Gates ──────────────────────────────────────────────────────
//
// The cheap-but-frequent lint/format/doc gates, so every change
// can be checked locally with one `dagger call preflight` before pushing, and the
// same functions back the self-hosted `dagger-preflight.yml` workflow.

// Preflight runs every fast gate in cheap-first, fail-fast order: the shell lint
// gates and `fmt` (seconds) before `rustdoc` and `clippy` (full workspace compile).
// The first failing gate aborts and its output is returned with the error. This is
// the umbrella the self-hosted CI calls; contributors can also target one gate
// (`dagger call clippy --source=.`).
func (m *FraiseqlCi) Preflight(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	gates := []struct {
		name string
		run  func(context.Context, *dagger.Directory) (string, error)
	}{
		{"shell-gates", m.ShellGates},
		{"fmt", m.Fmt},
		{"rustdoc", m.Rustdoc},
		{"rustdoc-default", m.RustdocDefault},
		{"clippy", m.Clippy},
		{"check-default", m.CheckDefault},
		{"check-fuzz", m.CheckFuzz},
		{"check-example-crates", m.CheckExampleCrates},
		{"check-r-examples", m.CheckRExamples},
	}

	var report strings.Builder
	for _, g := range gates {
		out, err := g.run(ctx, source)
		fmt.Fprintf(&report, "\n===== %s =====\n%s\n", g.name, out)
		if err != nil {
			return report.String(), fmt.Errorf("preflight gate %q failed: %w", g.name, err)
		}
	}
	report.WriteString("\npreflight OK: all fast gates passed\n")
	return report.String(), nil
}

// Fmt: `cargo +nightly fmt --all -- --check`. rustfmt's
// advanced options need nightly (rust-toolchain.toml pins stable to the MSRV), so
// rustBase carries a minimal nightly with only the rustfmt component.
func (m *FraiseqlCi) Fmt(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"cargo", "+nightly", "fmt", "--all", "--", "--check"}).
		Stdout(ctx)
}

// Clippy:
// `cargo clippy --workspace --all-features --all-targets -- -D warnings`.
// --all-features is intentional (lints every feature path; the test-* gate features
// only need infra at runtime).
func (m *FraiseqlCi) Clippy(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustSrc(source).
		WithExec([]string{
			"cargo", "clippy", "--workspace", "--all-features", "--all-targets",
			"--", "-D", "warnings",
		}).
		// The async-jobs example subgraph is its own crate outside the workspace;
		// its clippy gate lived only in the retired legacy ci.yml (#951).
		WithWorkdir("/src/examples/async-jobs-subgraph/subgraph").
		WithExec([]string{
			"cargo", "clippy", "--all-targets", "--", "-D", "warnings",
		}).
		Stdout(ctx)
}

// CheckDefault:
// `cargo check --workspace --all-targets` — the DEFAULT feature set.
//
// Every other Rust gate in this leg is `--all-features`, which by construction
// can never compile a `cfg(not(feature = …))` arm. The default configuration is
// not an edge case: it is what a plain `cargo build`, a `cargo install` and the
// slim Docker image produce, and it is the only one in which the server's
// non-wire dispatch arm and the `fraiseql` crate's feature-gated bin resolve the
// way a default deployment resolves them.
//
// #1101 reported this as "--all-targets --all-features does not compile the
// fraiseql-server binary". That mechanism does not reproduce — under both
// feature sets the bin is in cargo's unit graph (its `required-features = ["cli"]`
// is satisfied by `default = ["auth", "cli"]`), and it drops out only under
// `--no-default-features`. What the report's own evidence shows is the feature-OFF
// blindness above: the error appeared under default features and not under
// `--all-features`. The feature matrix leg covers many such arms by combo, but it
// is push-to-dev + dispatch, so a branch never sees it; this gate is in the
// required check.
func (m *FraiseqlCi) CheckDefault(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustSrc(source).
		WithExec([]string{"cargo", "check", "--workspace", "--all-targets"}).
		Stdout(ctx)
}

// CheckFuzz: `cargo check` over every `crates/*/fuzz` crate, plus the assertion that
// every fuzz target on disk is a `[[bin]]` the compiler actually reaches.
//
// Each fuzz crate declares its own `[workspace]`, so it is outside `--workspace` and
// no clippy, rustdoc, check-default or test leg has ever compiled one. The existing
// `lint-fuzz-targets` gate lives in ShellGates and is existence-only by design (no
// toolchain there), so it cannot see a type error. `fuzz.yml` builds them, but it is a
// weekly schedule and explicitly not a merge gate.
//
// That header's "not a merge gate" is about *crash results*, which are stochastic.
// Whether a target compiles is deterministic, and #1254 is what it costs when nothing
// checks: a signature change on 2026-08-20 left two fraiseql-db targets at
// error[E0308] through two red scheduled runs that notify nobody.
func (m *FraiseqlCi) CheckFuzz(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustSrc(source).
		WithExec([]string{"bash", "tools/check-fuzz-compiles.sh"}).
		Stdout(ctx)
}

// CheckExampleCrates: every standalone Cargo project under examples/ compiles.
//
// They declare their own [workspace], so `cargo check --workspace`, clippy and
// every test leg skip them by construction — the same blindness crates/*/fuzz had.
// `examples/rust/flight_client` could have sat broken indefinitely; nothing built
// it (#1200).
func (m *FraiseqlCi) CheckExampleCrates(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustSrc(source).
		WithExec([]string{"bash", "tools/check-example-crates-compile.sh"}).
		Stdout(ctx)
}

// CheckRExamples: every R file under examples/ parses.
//
// Nothing in this repository ran, linted or even parsed `examples/r/
// fraiseql_client.R` (#1260). The example gates reach authoring artifacts,
// compose files and standalone Cargo projects; none of them reaches R. #1200
// rewrote that client to perform the Flight handshake on a machine with no R
// installed, so it shipped unverified in the dimension it was most exposed to.
//
// Parsing is not running — it says nothing about whether the handshake works,
// which needs a live Flight server (Level 3 of #1260).
//
// Built on shellBase (the ghcr-mirrored ubuntu image, not Docker Hub) + apt
// r-base-core, so it adds no new base image and no Docker Hub pull: 35 MB,
// about ten seconds, and the layer caches. With Rscript on PATH the gate never
// reaches its container fallback.
func (m *FraiseqlCi) CheckRExamples(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.shellBase().
		WithExec([]string{
			"apt-get", "install", "-y", "--no-install-recommends", "r-base-core",
		}).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "tools/check-r-examples-parse.sh"}).
		Stdout(ctx)
}

// Rustdoc:
// `RUSTDOCFLAGS=-D warnings cargo doc --workspace --all-features --no-deps`.
func (m *FraiseqlCi) Rustdoc(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustSrc(source).
		WithEnvVariable("RUSTDOCFLAGS", "-D warnings").
		WithExec([]string{"cargo", "doc", "--workspace", "--all-features", "--no-deps"}).
		Stdout(ctx)
}

// RustdocDefault:
// `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps` — the DEFAULT feature set.
//
// The same blindness `CheckDefault` closes for the compiler, closed for rustdoc. The
// `--all-features` pass above resolves every intra-doc link, because every link target's
// feature is on; on the default features, 22 links into feature-gated API were dead and
// nothing saw it (#1199). That is the configuration a bare `cargo doc` produces in a
// consuming workspace, and — until this change added `all-features` to each published
// crate's `[package.metadata.docs.rs]` — the one docs.rs built.
//
// `--keep-going`: without it cargo stops scheduling after the first crate that fails to
// document, so a workspace with several broken crates reports one per run. #1199 was
// filed naming two crates; the whole set was seven.
func (m *FraiseqlCi) RustdocDefault(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustSrc(source).
		WithEnvVariable("RUSTDOCFLAGS", "-D warnings").
		WithExec([]string{"cargo", "doc", "--workspace", "--no-deps", "--keep-going"}).
		Stdout(ctx)
}

// ShellGates runs every non-Rust lint gate, in
// order, in one minimal container — the `make lint-*` policy checks (pure grep/wc
// over src/), plus check-test-imports.sh and the Phase-01 route-syntax gate. These
// need no Rust toolchain, so they stay off the heavy rustBase. `git init` supplies
// the toplevel check-route-syntax.sh cd's to; `set -e` preserves each gate's
// non-zero exit on a policy violation.
func (m *FraiseqlCi) ShellGates(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	script := strings.Join([]string{
		"set -e",
		"git init -q . >/dev/null",
		"make lint-tests-layout",
		"make lint-unwrap UNWRAP_ALLOW_LIMIT=" + unwrapAllowLimit,
		"make lint-expect",
		"make lint-async-trait",
		"make lint-gate-db",
		"make lint-gate-core",
		"make test-release-tooling",
		"make test-deadline-gate",
		// The changelog gate's SELF-TEST only. The gate itself needs real git
		// history and would pass vacuously here — this function ignores `.git`
		// and runs `git init -q .` below, so `Closes #N` over the release range
		// returns nothing. The gate runs in changelog-check.yml and in
		// release.yml's validate-release, both with fetch-depth: 0 (#1127).
		"make test-changelog-gate",
		"bash tools/check-test-imports.sh",
		"bash tools/check-route-syntax.sh",
		"bash tools/check-guard-parity.sh",
		// The test side of the same rule (#1272): a test that asserts a guard's
		// behaviour must take the temp_env lock, or it races the sibling that sets
		// the bypass and reads the guard as disabled. Four vault tests did; the
		// three asserting a refusal reddened this leg at random, and the one
		// asserting Ok passed with the SSRF guard refusing every address.
		"python3 tools/check-guard-test-lock.py",
		"bash tools/tests/guard_test_lock_test.sh",
		"bash tools/check-deploy-security.sh",
		// Deploy artifacts must name the version being released, and the chart's
		// default image must be one this project publishes (#1129).
		"bash tools/check-deploy-versions.sh",
		// Every fuzz.yml matrix row must name a target that exists in the crate it
		// names. One-directional: targets on disk need not be in the matrix (#1128).
		"bash tools/check-fuzz-targets.sh",
		// A Compose file merely NAMED by a make target, and absent, is outside
		// check-examples-integrity.sh — that gate discovers compose files with `find` and
		// checks what is inside them. Three make targets drove a path in no commit
		// reachable from dev and failed on their first line (#1219).
		"bash tools/check-compose-references.sh",
		"bash tools/tests/compose_references_test.sh",
		// A bare `image: fraiseql:...` in a fenced code block resolves to
		// docker.io/library/fraiseql, which this project cannot publish to — #1129's
		// defect, in the one place the file-level gates do not read (#1220).
		"bash tools/check-doc-image-refs.sh",
		"bash tools/check-phases-citations.sh",
		"bash tools/check-image-context.sh",
		"bash tools/tests/doc_image_refs_test.sh",
		// Every publishable crate is published by release.yml, in the order this
		// file's legacyPublishOrder dry-runs and self-tests. fraiseql-cdc-sinks
		// (#382) was in the workspace and in legacyPublishOrder but in neither the
		// publish steps nor the CRATES lists; as an OPTIONAL dependency of
		// fraiseql-server it was tolerated by the pre-tag dry-run and fatal to the
		// real publish. Nothing compared the two lists before this gate.
		"python3 tools/check-publish-parity.py",
		// The pre-tag image leg must build exactly what docker-build.yml publishes,
		// in both directions and across both of its matrices. Static — it reads the
		// two lists; building the images is the heavy leg's job (#1205).
		"python3 tools/check-image-parity.py",
		"make test-image-parity-gate",
		// One level up from check-suite-coverage.py: every artifact this repository
		// SHIPS maps to a leg that executes it, or to an exemption naming the issue
		// that owns the gap. Discovery reads the same sources the four per-class
		// parity gates read, so a new image variant, crate or SDK arrives here too;
		// what this adds is that a row may not claim coverage that does not exist —
		// `dagger:PublishDryRun` is a real function no workflow calls, and a leg is
		// only coverage if some run: step invokes it (#1205's hole, one level up).
		"python3 tools/check-delivery-coverage.py",
		"make test-delivery-coverage-gate",
		// A published SDK's lockfile may not pin a version its manifest no longer
		// claims (#1225): the 2.15.0 bump edited the SDK manifests and left
		// fraiseql-python's uv.lock and fraiseql-rust's Cargo.lock at 2.14.1.
		// fraiseql-typescript stayed correct only because typescript-sdk.yml runs
		// `npm ci`, which refuses a disagreeing lockfile — one SDK gated by accident
		// of tooling and two not. Pure text, so it runs here on every push rather
		// than behind the SDK legs' path filters; dependency drift is covered by the
		// `--locked` flags on those legs, and neither subsumes the other.
		"python3 tools/check-sdk-lockfile-freshness.py",
		"make test-sdk-lockfile-freshness-gate",
		// The local feature-matrix runner's red-capability pin (#1227). The RUNNER is
		// local-only — the leg it mirrors is this repo's `Dagger — feature matrix`, so
		// running it here would be running the matrix twice. What belongs here is the
		// pin: it stubs `cargo`, compiles nothing, needs no toolchain, and asserts the
		// one property that makes the local gate worth trusting — it cannot silently
		// cover fewer combos than .dagger/feature-combos.go declares.
		"make test-feature-matrix-gate",
		"bash tools/check-internal-flag-sites.sh",
		"bash tools/check-value-json-seam.sh",
		"bash tools/check-graphql-parse-sites.sh",
		"bash tools/check-audit-lockstep.sh",
		// The no-orphan-suites gate: every test target × feature combo maps to a
		// leg that executes it (it parses THIS file, so legs and gate cannot
		// drift). Retrospective rule 1 of the 2026-07-27 program.
		"python3 tools/check-suite-coverage.py",
		// …and the red-capability pin for its GitHub Actions side (#1120): a
		// workflow can look like coverage and provide none.
		"make test-suite-coverage-workflows",
		// ...and the pin for its INNER feature-gate side (#1179): a test fn behind
		// `#[cfg(feature = "x")]` in an ungated module was invisible to the gate,
		// counted as covered while compiled out — and a `not(feature)` arm needs a
		// leg with the feature OFF, which `--all-features` can never be.
		"make test-suite-coverage-inner-gates",
		// The conformance harness's own four properties (#1118). `project.py` cited a
		// `selftest.py` that had never existed, so the growth property — a new construct
		// fails every SDK until each implements it or declares the gap — was asserted in
		// prose and pinned by nothing. Synthetic dicts only: no toolchain, no CLI, no
		// network, so it belongs here rather than in the eleven-runtime SDK job.
		"make test-conformance-selftest",
		// Comment-only #[test] bodies read as green coverage (#895/#748).
		"bash tools/check-empty-tests.sh",
		// The rung above: not a comment-only body but a whole test BINARY that
		// references no fraiseql crate, so it can only assert against mocks it
		// defined itself (#1269/#1270). Its red capability is pinned separately —
		// a gate over a tree it just emptied is green for the wrong reason.
		"python3 tools/check-test-subject.py",
		"bash tools/tests/test_subject_test.sh",
		// Snapshot pairing, both directions (#986): every .snap registered, no
		// stale registry rows. Was a pre-commit-only hook that ran nowhere.
		"bash tools/check-snapshot-pairing.sh",
		"bash tools/check-deadlines.sh",
		"bash tools/check-docs-env-vars.sh",
		"bash tools/check-docs-version.sh",
		// Every typed TOML config loader has a coverage manifest naming each
		// key's consumer (#909). Retrospective rule 2: no unconsumed surface.
		"bash tools/check-config-loaders.sh",
		// A published crate's public API must not name a third-party type it does
		// not re-export (#1198): `JwtValidator::new` took a `jsonwebtoken::Algorithm`
		// reachable from nowhere, so the crate's documented first line did not
		// compile. Reads manifests with tomllib — there is no cargo in this container.
		"python3 tools/check-public-api-reexports.py",
		"make test-public-api-reexports-gate",
		// The SDKs the repo says it publishes are exactly the ones it can publish
		// (#1130). Eight sat frozen at 2.1.6 while README called two of them Tier 1
		// (Supported), and six had a live publish job that would have pushed 2.1.6
		// to a registry that will not take a version number back.
		"python3 tools/check-sdk-publication-claims.py",
		"make test-sdk-publication-claims-gate",
		"bash tools/check-sdk-dead-surface.sh",
		// No example provisions or points at a backend #374 removed. The
		// PostgreSQL-only de-scope covered crates/; examples/ kept a running
		// MySQL topology for three phases afterwards (#940).
		"bash tools/check-examples-postgres-only.sh",
		// A shipped example must be able to run at all: compose mounts resolve, COPY
		// sources exist in the build context, no `|| true` around a build step, no
		// health grep that also matches "unhealthy", every documented `cd` lands
		// somewhere. Before this, the whole CI coverage of `examples/` was one clippy
		// run and three greps, and a nine-issue audit found essentially every
		// documented entry point dead (#1050-#1054, #1071-#1073). The two tiers that
		// need a toolchain and a database are the `examples` integration suite.
		"bash tools/check-examples-integrity.sh",
		// A declared-but-unread `feature = []` is a promise the build cannot keep:
		// enabling it changes nothing, so the capability it names is either absent or
		// reachable another way. This gate existed but ran in NO leg while failing on
		// clean dev, so its one real finding (`export-parquet`, deleted in #1012) went
		// unreported for as long as it was there. A gate that is red and unrun is worth
		// less than no gate — it trains the next reader to assume the surface is checked.
		"bash tools/check-feature-chains.sh",
		// A per-crate runaway-growth ratchet, and a check that every crate has a
		// budget at all. Its only caller was the orphaned tools/lint.sh, so it ran
		// nowhere while four crates went over budget and five acquired no budget
		// row — and Cargo.toml claimed all along that CI enforced it (#1055/#990).
		"bash tools/check-crate-sizes.sh",
		// Every official SDK must be gated by a workflow that runs on a branch push.
		// Four of eleven were not: two declared `tags` with no `branches`, which
		// suppresses every branch push; one was post-merge-only; and the official
		// Ruby SDK's tests ran nowhere at all (#1119).
		"python3 tools/check-sdk-workflow-coverage.py",
		// A job `if:` may not name an event or a ref its own workflow cannot
		// receive. The 2026-05-31 migration stripped triggers and left the
		// conditions: docker-build.yml kept two jobs that read as image coverage
		// and had never run, because an unreachable job is absent from the checks
		// list rather than reported as skipped (#1206).
		"python3 tools/check-workflow-job-reachability.py",
		"bash tools/tests/workflow_job_reachability_test.sh",
		// This list and the Makefile's `preflight:` target are two hand-maintained
		// copies of one thing, so they drift, and `make preflight` says "Safe to
		// push" over the difference. It had drifted twice when this landed (#1135).
		"python3 tools/check-preflight-parity.py",
		"make test-preflight-parity",
		// Same shape, two legs over: `make test-integration-postgres` and
		// `make test-leg` are hand-maintained copies of the line lists in
		// integrationPostgres and Test. Before the first existed, the fact that
		// those suites only pass under --test-threads=1 lived ONLY in this file,
		// so `cargo test -p fraiseql-core` was red by construction and
		// `make test-integration` reported success having run 1 test out of 2828
		// (#1169). The second is the only command that runs what this leg's Test
		// function runs — nothing did before a merge to dev, which is how
		// `config_coverage_manifest_test` reddened the test leg on `231c3a25c`
		// after a green preflight and sixteen green branch legs (#1257).
		// Bidirectional — a local-only extra line falsifies the target's claim in
		// the other direction.
		"python3 tools/check-shard-parity.py",
		// cargo-deny's unmatched-skip lints default to WARN, so a stale exact-version
		// [[bans.skip-tree]] pin covers NOTHING while `cargo deny check` still exits 0.
		// The level cannot be set in deny.toml, so `-D` lives on three command lines;
		// this keeps them in lockstep (#1020, #933).
		// A Rust base image older than [workspace.package] rust-version cannot build
		// this workspace at all, and docker-build.yml is tag-only — so without this
		// the first witness to a stale pin is the release itself (#1107).
		// An uncopied [workspace] member means `cargo build -p fraiseql-server` cannot
		// even load the manifest, so the release image cannot be built at all (#1205).
		"bash tools/check-dockerfile-workspace-members.sh",
		"bash tools/tests/dockerfile_workspace_members_test.sh",
		"bash tools/check-dockerfile-msrv.sh",
		"bash tools/tests/dockerfile_msrv_test.sh",
		"python3 tools/check-deny-lint-flags.py",
		"bash tools/tests/deny_lint_flags_test.sh",
		"make test-shard-parity",
		// The bare-DATABASE_URL gate above ran here for its whole life without ever
		// being able to reject anything (#1075). Its red capability is now pinned.
		"make test-imports-gate",
	}, "\n")

	return m.shellBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// rustSrc mounts the source on rustBase with a persistent target cache volume. Used
// by the compiling gates (clippy, rustdoc); fmt skips the target cache (it never
// compiles).
func (m *FraiseqlCi) rustSrc(source *dagger.Directory) *dagger.Container {
	return m.rustBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume("fraiseql-rust-target"))
}

// rustBase is the shared Rust toolchain container for fmt/clippy/rustdoc. It pins the
// MSRV toolchain, installs the native deps a --all-features workspace compile needs
// (openssl→tiberius, cmake/sasl/zlib→rdkafka, protoc→tonic, python3→deno_core/v8),
// wires mold+clang for fast linking and sccache as the rustc wrapper, and shares the
// cargo registry/git and sccache caches across invocations via cache volumes. The
// per-invocation target cache is added by rustSrc.
func (m *FraiseqlCi) rustBase() *dagger.Container {
	const cargoHome = "/usr/local/cargo"
	installSccache := strings.Join([]string{
		"set -euo pipefail",
		"base=sccache-" + sccacheVersion + "-x86_64-unknown-linux-musl",
		"url=https://github.com/mozilla/sccache/releases/download/" + sccacheVersion + "/${base}.tar.gz",
		"curl -fsSL \"$url\" -o /tmp/sccache.tgz",
		"tar -xzf /tmp/sccache.tgz -C /tmp",
		"install -m0755 /tmp/${base}/sccache /usr/local/bin/sccache",
		"rm -rf /tmp/sccache.tgz /tmp/${base}",
		"sccache --version",
	}, "\n")

	return dag.Container().
		From(rustImage).
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{
			"apt-get", "install", "-y", "--no-install-recommends",
			"mold", "clang", "pkg-config", "libssl-dev", "cmake",
			"protobuf-compiler", "python3", "libsasl2-dev", "zlib1g-dev",
			// libxml2-dev + libxmlsec1-dev: samael's `xmlsec` backend (the #381
			// `auth-saml` feature). The shared clippy/rustdoc/test legs compile
			// `--all-features`, which turns auth-saml on, so the C stack must live in
			// the base. Mirrors the local requirement (`pacman -S xmlsec` on Arch).
			"libxml2-dev", "libxmlsec1-dev",
		}).
		// rustfmt + clippy on the pinned stable, plus rust-analyzer to satisfy
		// rust-toolchain.toml (avoids a mid-run auto-install); a minimal nightly
		// carrying only rustfmt for `cargo +nightly fmt`.
		WithExec([]string{"rustup", "component", "add", "clippy", "rustfmt", "rust-analyzer"}).
		WithExec([]string{"rustup", "toolchain", "install", "nightly", "--profile", "minimal", "--component", "rustfmt"}).
		WithExec([]string{"bash", "-c", installSccache}).
		WithEnvVariable("CARGO_TERM_COLOR", "always").
		WithEnvVariable("RUST_BACKTRACE", "1").
		// CARGO_INCREMENTAL=0 is required for sccache to cache; jobs cap mirrors
		// .cargo/config.toml (31 GiB RAM ceiling on this box).
		WithEnvVariable("CARGO_INCREMENTAL", "0").
		WithEnvVariable("CARGO_BUILD_JOBS", "16").
		// mold via clang — the committed .cargo/config.toml keeps this off for
		// GitHub-hosted compat; the self-hosted Dagger container ships mold.
		WithEnvVariable("RUSTFLAGS", "-C linker=clang -C link-arg=-fuse-ld=mold").
		WithEnvVariable("RUSTC_WRAPPER", "sccache").
		WithEnvVariable("SCCACHE_DIR", "/sccache").
		WithMountedCache("/sccache", dag.CacheVolume("fraiseql-sccache")).
		WithMountedCache(cargoHome+"/registry", dag.CacheVolume("fraiseql-cargo-registry")).
		WithMountedCache(cargoHome+"/git", dag.CacheVolume("fraiseql-cargo-git"))
}

// shellBase is the minimal container for the non-Rust lint gates: bash + make + the
// grep/awk/find toolchain the `make lint-*` recipes and check-*.sh scripts use
// (gawk, not mawk, for the load-bearing multi-line route-syntax pass — see lintBase).
func (m *FraiseqlCi) shellBase() *dagger.Container {
	return dag.Container().
		From(ubuntuImage).
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{
			"apt-get", "install", "-y", "--no-install-recommends",
			// python3: the suite-coverage gate (tools/check-suite-coverage.py).
			"make", "git", "gawk", "findutils", "grep", "ca-certificates", "python3",
		})
}

// ── Phase 03: Workspace Test Suite ────────────────────────────────────────────
//
// The workspace test leg: a full `cargo build --all-features`
// followed by the feature-scoped `cargo test -p …` invocations (the SYNC:* lists)
// and the doctest pass. Parameterized by toolchain (stable | MSRV 1.94.1).

// Test runs the workspace test suite for the given toolchain. `rust` is "msrv"
// (default — the pinned floor, == rust-toolchain.toml) or "stable" (latest stable).
//
// Testcontainers-backed tests are SKIPPED here: the Dagger engine has no Docker
// socket, so tests that boot their own Postgres container (storage metadata/
// migrations/routes, functions migrations, all of fraiseql-wire's tests/* binaries)
// cannot run. They fail cleanly (no container leak), and are restored in Phase 04
// via Dagger-native service bindings. The skip is logged explicitly. See
// docs/contributing/dagger-parity-notes.md Phase 03/04.
func (m *FraiseqlCi) Test(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// +optional
	// +default="msrv"
	rust string,
) (string, error) {
	toolchain := resolveToolchain(rust)
	// Per-toolchain target cache: stable and 1.94.1 produce incompatible artifacts,
	// so they must not share a target dir (kept separate from the Phase-02 gates'
	// `fraiseql-rust-target`, which holds clippy/rustdoc check artifacts).
	// `test2-` bump (2026-06-30): the `test-` volume held stale fraiseql-cli/-db
	// artifacts that cargo reused across branches, hiding newly-added public items
	// (#501's `commands::query` / `runtime_probe_checks`) → false E0432 on msrv only.
	targetVol := "fraiseql-rust-target-test2-" + strings.ReplaceAll(toolchain, ".", "-")

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### cargo build --all-features'",
		// No: on a cold run the verbose stream + telemetry can back up the
		// dagger client session and time out the return value ("client session
		// attachables: context deadline exceeded"). Failures still surface (cargo
		// prints them regardless), and warm runs are short.
		// #880 canary wraps the leg's first build: it detects a stale mounted
		// target/ volume (source digest changed, zero units compiled), purges and
		// rebuilds, and prints the fresh-unit diagnostic in the leg output.
		"bash tools/ci-target-canary.sh -- build --all-features",
		"echo '### skipped in-engine (env-incompatible; restored in a later phase):'",
		// (runtime-deno was on this list until #971 — the SIGSEGV was #969's
		// per-process second-isolate crash, not a sandbox limitation.)
		"echo '###   testcontainers (need Docker): storage metadata/migrations/routes::tests, functions migrations::tests, fraiseql-wire tests/*'",
		"echo '### cargo test --workspace (non-DB crates; wire+functions run separately below)'",
		"cargo test --workspace" +
			// fraiseql-arrow is excluded here and run explicitly below, so its
			// line is greppable in the log.
			" --exclude fraiseql-core --exclude fraiseql-db --exclude fraiseql-arrow" +
			" --exclude fraiseql-observers --exclude fraiseql-server --exclude fraiseql-wire" +
			" --exclude fraiseql-functions" +
			" --all-features " + testWorkspaceSkip,
		"echo '### cargo test -p fraiseql-wire --lib (tests/* skipped: testcontainers)'",
		"cargo test -p fraiseql-wire --lib --all-features",
		// fraiseql-arrow was --exclude'd from the workspace run above and named by
		// no other Dagger leg, so nothing in CI had run any of its ~380 tests since
		// the Dagger migration. (The legacy hosted workflow named three of its binaries, but that
		// whole workflow has been workflow_dispatch-only since then.) That is how
		// #716 — the Flight result cache keyed on SQL text alone, serving one
		// principal's rows to another — shipped. Its DB-backed binaries skip
		// gracefully without DATABASE_URL, so the whole crate runs here.
		"echo '### cargo test -p fraiseql-arrow --all-features (DB-backed binaries skip gracefully without DATABASE_URL)'",
		"cargo test -p fraiseql-arrow --all-features",
		// ...and again on the DEFAULT feature set, which is the only configuration that
		// compiles the crate's `#[cfg(not(feature = "parquet"))]` arms — the assertions
		// that the refusal path names the missing feature. `--all-features` compiles
		// them to nothing, so with that as the crate's only lib invocation they had
		// never executed (#1179; the issue had it backwards — the parquet tests DO run,
		// it is the feature-OFF arms that did not). 288 lib tests in ~2s.
		"echo '### cargo test -p fraiseql-arrow --lib (default features: the not(parquet) refusal arms)'",
		"cargo test -p fraiseql-arrow --lib",
		// fraiseql-functions including runtime-deno (#971): the old "V8 SIGSEGVs
		// in the exec sandbox" diagnosis was wrong — the crash was the
		// second-V8-isolate-per-process bug, fixed by #969; the sandbox was never
		// shown to be at fault. `cargo test` runs one process per binary, and the
		// whole deno suite passes in one shared process since #969.
		// migrations::tests skipped (testcontainers).
		"echo '### cargo test -p fraiseql-functions (all features incl. runtime-deno per #971/#969; migrations::tests skipped: testcontainers)'",
		"cargo test -p fraiseql-functions --features 'runtime-deno,runtime-wasm,host-live,host-storage' -- --skip migrations::tests",
		// core/db: --lib only. Their src/ unit tests are Docker-free, but their
		// tests/* integration binaries boot Postgres via tests/common/testcontainer.rs
		// (and the federation/* docker tests) — those belong to Phase 04's integration
		// matrix (Dagger services), not the unit-test phase. server (step below) is
		// already --lib for the same reason.
		"echo '### cargo test -p fraiseql-core --lib (SYNC:CORE_FEATURES; tests/* = testcontainer integration → Phase 04)'",
		"cargo test -p fraiseql-core --lib --features '" + coreTestFeatures + "'",
		// ...and on the DEFAULT feature set, which is the only configuration that
		// compiles the `#[cfg(not(feature = …))]` arms — `test_kafka_stub_fails_loud`
		// asserts the stub refuses loudly when `kafka` is off, and every lib run above
		// turns it on, so it had never executed. Same blind spot as #1227, one level
		// over: a wide `--features` list cannot see a feature-OFF arm (#1179).
		"echo '### cargo test -p fraiseql-core --lib (default features: the feature-OFF arms)'",
		"cargo test -p fraiseql-core --lib",
		"echo '### cargo test -p fraiseql-db --lib (SYNC:DB_FEATURES; tests/* = testcontainer integration → Phase 04)'",
		"cargo test -p fraiseql-db --lib --features '" + dbTestFeatures + "'",
		// #974: server::routing::storage_policy_admin_tests drives the bucket
		// policy admin API against a REAL policy store, so it is skipped BY NAME
		// here (this leg binds no Postgres) and named explicitly in the
		// integration leg below. Skipping by name rather than letting the suite
		// self-skip keeps the split visible in the log instead of turning into a
		// green that asserted nothing.
		"echo '### cargo test -p fraiseql-server --lib (SYNC:SERVER_FEATURES; storage_policy_admin_tests → integration leg)'",
		"cargo test -p fraiseql-server --lib --features '" + serverTestFeatures + "' -- --skip server::routing::storage_policy_admin_tests",
		// ...and on the DEFAULT feature set, for the same reason as the core line above:
		// the `not(federation)` health-status arms and
		// `from_file_names_the_build_feature_for_a_compiled_out_section` — which asserts
		// that a `[observers]` section in a build without the feature is REFUSED by name
		// rather than ignored — compile only where those features are off (#1179).
		"echo '### cargo test -p fraiseql-server --lib (default features: the feature-OFF refusal arms)'",
		"cargo test -p fraiseql-server --lib -- --skip server::routing::storage_policy_admin_tests",
		// The MCP transport's Docker-free test binaries. They ran in
		// feature-flags.yml's `feature-integration-tests` job, which has been
		// dispatch-only since the Dagger migration (2026-05-31) — so no CI leg
		// executed them, and #857 (every tool call fails under the DEFAULT
		// camelCase naming convention) sat undetected. `--lib` above does not
		// reach `tests/*`, so they are named explicitly. The two-tenant dispatch
		// suite needs a database and runs in the integration leg instead.
		"echo '### cargo test -p fraiseql-server mcp test binaries (SYNC:SERVER_FEATURES; not covered by --lib)'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --test mcp_transport_safety_test --test mcp_e2e_test --test mcp_integration_test",
		// The subscription test binaries (P18 verification gate). Docker-free —
		// in-memory schema + manager over real TCP WebSockets. Like the MCP
		// binaries above, none of these had ever run in ANY CI leg (`--lib`
		// does not reach `tests/*`), which is how the #771/#772/#773 lifecycle
		// gaps shipped. `subscription_lifecycle_ws_test` pins mid-stream
		// expiry/revocation (#771), loud broadcast lag (#772), hot-reload
		// policy propagation (#611), graceful drain (#571), and the #758
		// tenant fail-closed gate on the live /ws path.
		"echo '### cargo test -p fraiseql-server subscription test binaries (SYNC:SERVER_FEATURES; not covered by --lib) — P18 verification gate'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --test subscription_ws_e2e_test --test subscription_protocol_test --test subscription_integration_test --test subscription_forwarder_integration_test --test graphql_ws_row_visibility_pin_test --test subscription_lifecycle_ws_test",
		// P19 (#747): the GraphQL Idempotency-Key receiver contract — a saga
		// peer's at-least-once retries deduplicate to one logical effect.
		// Docker-free (mock adapter), but a tests/* binary no other invocation
		// reaches (fraiseql-server is excluded from the workspace sweep).
		"echo '### cargo test -p fraiseql-server graphql idempotency binary (SYNC:SERVER_FEATURES; not covered by --lib) — P19 #747 receiver gate'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --test graphql_idempotency_e2e_test",
		// #992: the in-process test binaries. ~60 of fraiseql-server's tests/*.rs
		// files ran in NO leg (the crate is excluded from the workspace run and
		// only enumerated binaries execute). Everything here passed a service-less
		// run; DB/redis-backed binaries stay with the integration legs.
		"echo '### cargo test -p fraiseql-server in-process test binaries (SYNC:SERVER_FEATURES; #992 — suite-coverage gate holds this list)'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "'" + serverInProcessTests,
		// #992: feature-gated lib test modules whose features serverTestFeatures
		// omits — without these lines they compile out of every leg and read as
		// passing (the #981 class).
		"echo '### cargo test -p fraiseql-server --lib feature-gated modules with no service needs (#992)'",
		"cargo test -p fraiseql-server --features rest,export-csv,export-xlsx --lib -- routes::rest::streaming:: routes::rest::openapi::",
		"cargo test -p fraiseql-server --features functions --lib routes::functions::",
		// The function-runtime subsystem wiring: `functions = []` does not imply
		// `functions-runtime`, so the line above compiles none of it. deno implies the
		// base runtime, so one invocation covers both gates.
		"cargo test -p fraiseql-server --features functions-runtime-deno --lib subsystems::",
		"cargo test -p fraiseql-server --features cdc-outbound --lib cdc_outbound::",
		// #975: the same module again with the Kafka sink compiled in. Both runs
		// are needed and neither substitutes for the other — validate_kind's
		// accept-kafka and refuse-kafka-by-name halves are `cfg`-gated against
		// each other, so each is invisible in the other's leg.
		"cargo test -p fraiseql-server --features cdc-kafka --lib cdc_outbound::",
		// #975's Kinesis sink mount: its own arm of ConfiguredSink/build_one, and
		// the feature-ON half of validate_kind, compile only with this feature on.
		"cargo test -p fraiseql-server --features cdc-kinesis --lib cdc_outbound::",
		// #975: the cdc-sinks crate's own rdkafka-bound unit tests. The
		// always-compiled endpoint guard runs in the default leg above; this is the
		// half that needs the feature.
		"cargo test -p fraiseql-cdc-sinks --features cdc-kafka --lib kafka::",
		// #1102: the [subscription_kafka] mount and its config section. The whole of
		// that issue is a transport no shipped configuration could reach, so a leg
		// that does not compile the feature is a leg that cannot see it — which is
		// exactly how it went four releases setting no `security.protocol` at all.
		// One filter covers both `subscription_kafka::` and
		// `server_config::subscription_kafka::`.
		"cargo test -p fraiseql-server --features subscription-kafka --lib subscription_kafka::",
		// #1102: the shared egress itself — the guard reaching it, the classification,
		// and the timeout contract each caller depends on. rdkafka is not optional in
		// this crate, so there is no feature-OFF arm to miss.
		"cargo test -p fraiseql-kafka",
		// The #612-M config-coverage manifest gate ('every ServerConfig leaf has a
		// named consumer'). Another test binary no leg had ever run — it caught
		// P18's new `subscription_auth_recheck_secs` key only in a LOCAL full test
		// run. It needs the export/sources/inbound features on top of
		// SYNC:SERVER_FEATURES so every feature-gated manifest entry matches a
		// real leaf.
		"echo '### cargo test -p fraiseql-server config coverage manifest + doc config examples (not covered by --lib)'",
		// Every `cfg`-gated `ServerConfig` field must be COMPILED IN here, or the
		// manifest gate's two directions disagree: a leaf that does not exist has
		// no consumer to name, and its manifest entry reads as stale. Grow this
		// list whenever a new feature adds a config section (#381 added `saml`).
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + ",export-csv,export-xlsx,sources,inbound,inbound-email,auth-saml,cdc-outbound,subscription-kafka' --test config_coverage_manifest_test --test doc_config_examples_test",
		// fraiseql-observers --lib: the Docker-free unit tests (config, executor,
		// DLQ, email, CLI). DB/redis/nats tests are #[ignore]d (or skip-on-None)
		// and run in the integration legs; `--features cli` pulls in the CLI
		// subcommand tests. Previously observers was excluded from the workspace
		// run and only its #[ignore]d/name-filtered tests ran (in integration),
		// so these unit tests never executed in CI.
		// arrow/checkpoint/dedup/metrics/search are pure lib features whose unit
		// modules were compiled out of every leg before (#992).
		"echo '### cargo test -p fraiseql-observers --lib --features caching,cli,arrow,checkpoint,dedup,metrics,nats,postgres,search (Docker-free unit tests; DB/redis/nats tests are #[ignore]d → integration legs)'",
		"cargo test -p fraiseql-observers --lib --features 'caching,cli,arrow,checkpoint,dedup,metrics,nats,postgres,search'",
		// #992: observers in-process test binaries — the crate is excluded from
		// the workspace run, so these executed nowhere.
		// queue,metrics,testing: job_queue_integration is cfg-gated on them and
		// runs ZERO tests without (verified — a plain run reads ok. 0 passed).
		"echo '### cargo test -p fraiseql-observers in-process test binaries (#992)'",
		"cargo test -p fraiseql-observers --features 'queue,metrics,testing' --test job_queue_integration --test property_state_machine --test stress_tests --test transport_pipeline_test",
		// #429 wired saga forward executor: the gated path is off in the workspace
		// run above, so run its Docker-free lib tests explicitly — the pure decision
		// helpers and the remote-dispatch/honest-failure lib tests (the real execute_step
		// proof. The Postgres orchestration tests are #[ignore]d → integration leg.
		"echo '### cargo test -p fraiseql-federation --lib --features saga (#429 remote dispatch + honesty; execute_step PG proofs run in the postgres integration leg)'",
		"cargo test -p fraiseql-federation --lib --features saga",
		"echo '### cargo test --doc --all-features'",
		// Cap doctest concurrency: `cargo test --doc` spawns one process per doctest,
		// and the default thread count (= CPU count) OOMs the 31 GiB box on the heavy
		// --all-features doctest set (arrow doctests were OOM-killed). See the leg-level
		// CARGO_BUILD_JOBS note below.
		"cargo test --doc --all-features -- --test-threads=6",
		"echo \"test OK: workspace suite passed (toolchain " + toolchain + ", testcontainers tests skipped)\"",
	}, "\n")

	return m.rustBaseFor(toolchain).
		// The test leg is the only one that runs a full `cargo build --all-features`
		// PLUS the workspace test + doctest suites in one container. Since the functions
		// runtime pulled V8 into --all-features (a very memory-heavy compilation unit),
		// 16 parallel rustc jobs peak over this box's 31 GiB RAM and the OOM killer
		// takes rustc/doctest processes (bare exit-101, no diagnostic). Cap this leg to
		// 8 jobs (the other legs stay at the base 16 — they don't OOM). See #615.
		WithEnvVariable("CARGO_BUILD_JOBS", "8").
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume(targetVol)).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// resolveToolchain maps the user-facing --rust value to a rustup toolchain name.
func resolveToolchain(rust string) string {
	switch rust {
	case "", "msrv", rustMsrv:
		return rustMsrv
	case "stable":
		return "stable"
	default:
		return rust
	}
}

// rustBaseFor returns rustBase pinned to a specific toolchain via RUSTUP_TOOLCHAIN,
// which overrides the repo's rust-toolchain.toml (pinned to the MSRV). "stable" is
// installed on demand; the MSRV toolchain ships in the base image.
func (m *FraiseqlCi) rustBaseFor(toolchain string) *dagger.Container {
	base := m.rustBase()
	if toolchain != rustMsrv {
		base = base.WithExec([]string{"rustup", "toolchain", "install", toolchain, "--profile", "minimal"})
	}
	return base.WithEnvVariable("RUSTUP_TOOLCHAIN", toolchain)
}

// ── Phase 04: Integration Matrix ──────────────────────────────────────────────
//
// The integration shards, on Dagger-native service bindings — NO
// testcontainers, NO DinD. Each backing service is a dag.Container().AsService()
// bound into the test container; the tests read the injected env URL through the
// fraiseql-test-support harness. This makes local == CI: `dagger call
// test-integration` here provisions the same pinned, bound services as the
// self-hosted workflow does. See docs/contributing/dagger-parity-notes.md Phase 04.

const (
	// pgImage pins the integration Postgres.
	// pgvector/pgvector:pg16 = the official postgres:16 plus the pgvector
	// extension (#386's executed vector-similarity suites CREATE and query it);
	// mirrored by mirror-base-images.yml like every other base image.
	pgImage = "ghcr.io/fraiseql/pgvector:pg16"
	// pgUser/pgPassword/pgDatabase are the test-only Postgres credentials.
	pgUser     = "fraiseql_test"
	pgPassword = "fraiseql_test_password"
	pgDatabase = "test_fraiseql"
	// pgBindHost is the service-binding alias; bound callers reach Postgres here on
	// its internal 5432 (not the legacy host-mapped 5433).
	pgBindHost = "postgres"

	// pgStandbyBindHost — a REAL streaming standby of pgService, for the #957
	// bounded-staleness suite. The pre-existing read-replica tests stand a second
	// independent database in for a replica, which shows which server answered but
	// reports no replication lag at all (`pg_is_in_recovery()` is false there), so
	// `max_lag_ms` proven against it would be proven against a server that cannot
	// be stale. Credentials are the replication role `postgres-replication-init.sh`
	// creates on the primary.
	pgStandbyBindHost     = "postgres-standby"
	pgReplicationUser     = "fraiseql_repl"
	pgReplicationPassword = "fraiseql_repl_password"

	// pgFailoverBindHost — a SECOND standby, which exists to be destroyed: the
	// #957 failover test calls pg_promote() on it, and a promoted standby never
	// goes back. Sharing the bounded-staleness standby would make those tests'
	// meaning depend on which one libtest happened to run first.
	pgFailoverBindHost = "postgres-failover"
	pgFailoverSlot     = "fraiseql_failover"

	// redisImage / redisBindHost — the Redis service.
	redisImage    = "ghcr.io/fraiseql/redis:7-alpine"
	redisBindHost = "redis"

	// vaultImage / vaultBindHost / vaultToken — the Vault dev-mode service
	// Dev-mode root token; test-only.
	vaultImage    = "ghcr.io/fraiseql/vault:1.17"
	vaultBindHost = "vault"
	vaultToken    = "fraiseql-test-token"

	// natsImage / natsBindHost — the NATS JetStream service,
	// started with `-js -m 8222`.
	natsImage    = "ghcr.io/fraiseql/nats:2.10-alpine"
	natsBindHost = "nats"

	// kafkaImage / kafkaBindHost — the Apache Kafka broker for the #975 outbound
	// CDC sink. Kafka 4.x is KRaft-only, so this is one container with no
	// ZooKeeper sidecar. The JVM image is preferred over `apache/kafka-native`
	// (~1 s boot vs ~10 s) for fidelity on a durability-critical path.
	kafkaImage    = "ghcr.io/fraiseql/kafka:4.3.1"
	kafkaBindHost = "kafka"

	// localstackImage / localstackBindHost — the AWS emulator backing the #975
	// Kinesis outbound CDC sink. Only the `kinesis` service is enabled; the image
	// starts every service in SERVICES and nothing else here needs the rest.
	// Pinned like every other broker image — only `minio` and `fake-gcs-server`
	// ride `:latest`, and those are weekly-refreshed by design.
	localstackImage    = "ghcr.io/fraiseql/localstack:3.8"
	localstackBindHost = "localstack"

	// mailhogImage / mailhogBindHost — the MailHog SMTP sink for the #349 email
	// happy-path test. Speaks real SMTP on 1025 (plaintext) and exposes an HTTP
	// API on 8025 to inspect captured messages; the test sends through lettre and
	// asserts the message arrived in the sink.
	mailhogImage    = "ghcr.io/fraiseql/mailhog:v1.0.1"
	mailhogBindHost = "mailhog"

	// serverBindHost / e2eMetricsToken — the HTTP E2E server service (legacy
	// integration-http-e2e): the fraiseql-server binary run as a bound service the
	// test container drives over HTTP.
	serverBindHost  = "fraiseql-server"
	e2eMetricsToken = "e2e-test-metrics-token-32chars!"

	// tlsBindHost — the TLS Postgres service. The cert's
	// SAN includes this alias (CERT_HOSTNAME) so rustls servername verification
	// passes when the wire client connects to it.
	tlsBindHost = "postgres-tls"

	// wireBindHost — the Postgres service for the fraiseql-wire integration tests.
	// It enables SCRAM-SHA-256 explicitly so the wire client's auth path (and the
	// auth/scram rejection tests) are exercised exactly as under the old testcontainer.
	wireBindHost = "postgres-wire"

	// azuriteImage / azuriteBindHost — the Azure Blob emulator for the fraiseql-storage
	// azure_emulator test. The backend reaches it at
	// http://<alias>:10000/devstoreaccount1 via AZURE_BLOB_ENDPOINT.
	azuriteImage    = "mcr.microsoft.com/azure-storage/azurite:latest"
	azuriteBindHost = "azurite"
	// fakeGcsImage / fakeGcsBindHost — the GCS emulator for the fraiseql-storage
	// gcs_emulator test. The backend reaches it at http://<alias>:4443 via GCS_ENDPOINT;
	// -external-url must match so the emulator's media links point back at the alias.
	fakeGcsImage    = "ghcr.io/fraiseql/fake-gcs-server:latest"
	fakeGcsBindHost = "fake-gcs"

	// minioImage / minioBindHost / minioUser / minioPass — the S3-compatible MinIO
	// service for fraiseql-server's storage_minio integration test. The test reads
	// MINIO_ENDPOINT (http://<alias>:9000) and authenticates with the constants below.
	minioImage    = "ghcr.io/fraiseql/minio:latest"
	minioBindHost = "minio"
	minioUser     = "minioadmin"
	minioPass     = "minioadmin"

	// Federation suite: two FraiseQL subgraph servers (users + reviews) behind an
	// Apollo Router, each bound to its own seeded Postgres. The fraiseql-server binary
	// is built with the federation feature and run as a bound service.
	fedUsersBindHost     = "fed-pg-users"
	fedReviewsBindHost   = "fed-pg-reviews"
	fedSubgraphABindHost = "fed-subgraph-a"
	fedSubgraphBBindHost = "fed-subgraph-b"
	apolloRouterBindHost = "apollo-router"
	apolloRouterImage    = "ghcr.io/apollographql/router:v1.45.0"
	// Dedicated target cache for the federation-feature build (the subgraph binary +
	// the test compile both link `--features federation`, a distinct artifact set).
	fedTargetVol = "fraiseql-rust-target-fed-1-92"
)

// TestIntegration runs one integration suite against Dagger-bound services. `suite`
// selects which (default "postgres"). The suites come online incrementally as the
// tiers converge onto the harness.
func (m *FraiseqlCi) TestIntegration(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// +optional
	// +default="postgres"
	suite string,
) (string, error) {
	switch suite {
	case "", "postgres":
		return m.integrationPostgres(ctx, source)
	case "nats":
		return m.integrationNats(ctx, source)
	case "observers":
		return m.integrationObservers(ctx, source)
	case "http-e2e":
		return m.integrationHTTPE2e(ctx, source)
	case "tls":
		return m.integrationTLS(ctx, source)
	case "server":
		return m.integrationServer(ctx, source)
	case "redis":
		return m.integrationRedis(ctx, source)
	case "vault":
		return m.integrationVault(ctx, source)
	case "wire":
		return m.integrationWire(ctx, source)
	case "storage":
		return m.integrationStorage(ctx, source)
	case "server-storage":
		return m.integrationServerStorage(ctx, source)
	case "federation":
		return m.integrationFederation(ctx, source)
	case "federation-compose":
		return m.integrationFederationCompose(ctx, source)
	case "saml":
		return m.integrationSaml(ctx, source)
	case "quickstart":
		return m.integrationQuickstart(ctx, source)
	case "examples":
		return m.integrationExamples(ctx, source)
	default:
		return "", fmt.Errorf("unknown integration suite %q (known: postgres, nats, observers, http-e2e, tls, server, redis, vault, wire, storage, server-storage, federation, federation-compose, saml, quickstart, examples)", suite)
	}
}

// integrationQuickstart executes docs/guides/getting-started.md VERBATIM
// against a real PostgreSQL (#734): tools/quickstart-smoke.sh extracts the
// doc's fenced code blocks and runs them in order — schema authored with the
// in-repo Python SDK, `fraiseql-cli compile`, a real `fraiseql-server` boot,
// and one HTTP query whose response must match the doc's expected output. The
// doc IS the fixture, so quickstart drift (phantom APIs, wrong flags, wrong
// ports — the #734 class) reddens this leg. Phase 2 of the script scaffolds a
// project with `fraiseql init` and runs its Python authoring skeleton, the
// regeneration path the cargo suites cannot cover (it needs Python).
func (m *FraiseqlCi) integrationQuickstart(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: quickstart (docs/guides/getting-started.md verbatim, #734)'",
		"bash tools/ci-target-canary.sh -- build -p fraiseql-cli --bin fraiseql-cli", // #880 canary wraps the build
		"cargo build -p fraiseql-server --bin fraiseql-server",
		"export PATH=/src/target/debug:$PATH",
		"bash tools/quickstart-smoke.sh",
		"echo 'test-integration OK: quickstart suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		// The smoke drives the documented user tooling, not the test harness:
		// psql applies the doc's setup.sql, curl issues the doc's query.
		// python3-httpx is the SDK's one third-party import (the doc's
		// `pip install fraiseql` is substituted with the in-repo SDK).
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{"apt-get", "install", "-y", "--no-install-recommends", "postgresql-client", "curl", "python3-httpx"}).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithEnvVariable("SMOKE_DATABASE_URL", dbURL).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationExamples is the examples gate's two executing tiers (Phase 09 of the
// 2026-08-22 program, #1050-#1054, #1071-#1073).
//
// Before this leg, the entire CI coverage of `examples/` was one clippy run over the
// single example that is a Rust crate, plus three greps. A nine-issue audit found
// essentially every documented entry point dead — and nothing would have noticed,
// which is why repairing them without a gate buys a state that rots by the next
// release.
//
// Two tiers here, both of which need a toolchain, so neither can live in ShellGates
// (the static tier does, and runs in preflight):
//
//   - check-examples-compile.sh — every example's schema.py runs and every
//     fraiseql.toml/schema.json compiles, each from its own directory.
//   - examples-smoke.sh — every example with a sql/setup.sql loads it under
//     ON_ERROR_STOP=1, compiles, resolves every one of its queries/*.graphql against
//     a real PostgreSQL, and then a real fraiseql-server boots on it and answers a
//     real query over HTTP.
//
// The HTTP half is the point: compiling the artifact is not testing the example, and
// a healthy container is not a working one. #1071's image built, then refused to
// boot, then booted healthy and answered ordinary queries while refusing the one
// query that made it a subgraph. Only asking it that question finds that.
//
// Neither script skips. A missing binary, a missing psql, an unset DATABASE_URL are
// each a failure — a check that quietly does nothing reads as green.
func (m *FraiseqlCi) integrationExamples(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: examples (compile tier + smoke tier)'",
		"bash tools/ci-target-canary.sh -- build -p fraiseql-cli --bin fraiseql-cli", // #880 canary wraps the build
		"cargo build -p fraiseql-server --bin fraiseql-server",
		"export FRAISEQL_BIN=/src/target/debug/fraiseql-cli",
		"export SERVER_BIN=/src/target/debug/fraiseql-server",
		"bash tools/check-examples-compile.sh",
		"bash tools/examples-smoke.sh",
		"echo 'test-integration OK: examples suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		// The smoke drives the tools a reader drives: psql applies each example's
		// setup.sql, curl asks the booted server a question. python3-httpx is the
		// authoring SDK's one third-party import, and every example authors through
		// it — five example schema.py files still did `from fraiseql import key` when
		// this landed, and nothing had run them since v1.
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{"apt-get", "install", "-y", "--no-install-recommends", "postgresql-client", "curl", "python3-httpx"}).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		// examples-smoke.sh CREATEs and DROPs a database per example, so it needs a
		// URL it can connect to while doing that — the maintenance database, not one
		// of its own.
		WithEnvVariable("DATABASE_URL", dbURL).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationPostgres binds a seeded postgres:16 service — plus a real streaming
// standby of it (#957) — and runs the PostgreSQL integration tests that already
// route through the harness. The harness reads DATABASE_URL (injected below) and
// connects to the bound service; the read-replica lag suite additionally reads
// STANDBY_DATABASE_URL.
func (m *FraiseqlCi) integrationPostgres(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	standbyURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgStandbyBindHost, pgDatabase)
	failoverURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgFailoverBindHost, pgDatabase)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: postgres (Dagger-bound service; tests read DATABASE_URL via harness)'",
		// Broad core/db `--test '*'` sweep (matches the legacy integration-postgres job).
		// The redis/federation-gated tests skip cleanly (only pg is bound).
		"bash tools/ci-target-canary.sh -- test -p fraiseql-core --features '" + coreTestFeatures + ",test-postgres' --test '*'", // #880 canary
		"cargo test -p fraiseql-core --features '" + coreTestFeatures + ",test-postgres' --test '*' -- --test-threads=1",
		"cargo test -p fraiseql-db --features '" + dbTestFeatures + ",test-postgres' --test '*' -- --test-threads=1",
		// `--test '*'` runs only tests/ binaries. `postgres::adapter::integration_tests`
		// is a LIB module gated on `test-postgres`, so it was compiled out of the
		// DB-less test leg (which omits that feature) AND skipped by the line above
		// — 24 live-PostgreSQL tests, including the #832 relay ORDER BY proofs, ran
		// in NO leg. Same shape as P09/P16/P18/P20/P21.
		"echo '### cargo test -p fraiseql-db --lib --features test-postgres (live-PG lib tests)'",
		"cargo test -p fraiseql-db --lib --features '" + dbTestFeatures + ",test-postgres' -- --test-threads=1",
		// Tier-C migrated: fraiseql-functions cron-state migration (lib tests; harness postgres()).
		"cargo test -p fraiseql-functions --lib migrations::tests -- --test-threads=1",
		// #411 durable identity store (PostgresAccountStore: core.tb_user / tb_auth_identity + RLS).
		"cargo test -p fraiseql-auth --test postgres_account_store -- --test-threads=1",
		// #412 local-password authenticator (Argon2id signup/login/rehash/disabled + RLS).
		"cargo test -p fraiseql-auth --test local_password -- --test-threads=1",
		// #367 password reset (selector+verifier tokens, single-use, expiry, RLS).
		"cargo test -p fraiseql-auth --test password_reset -- --test-threads=1",
		// #945 email verification: the token discipline, the session binding that makes a
		// token alone insufficient, and the BIDIRECTIONAL pre-hijack invariant — neither a
		// pre-seeded unverified local account nor a later-verified one may absorb a trusted
		// account holding the same address.
		"cargo test -p fraiseql-auth --test email_verification -- --test-threads=1",
		// #368 social auto-link trust policy ∘ PostgresAccountStore (trusted vs untrusted → merge vs distinct).
		"cargo test -p fraiseql-auth --test social_linking -- --test-threads=1",
		// #984 single-use consume of OTP codes / MFA challenge tokens: a BEFORE DELETE
		// trigger poisons only the consuming statement, proving both stores fail closed
		// instead of reporting a single-use guarantee the failed DELETE did not establish.
		"cargo test -p fraiseql-auth --test postgres_single_use_consume -- --test-threads=1",
		// #950 expiry sweeps for the MFA/OTP tables. Two-sided: the expired row goes and
		// the live one stays, so a sweep that deleted everything would fail here.
		"cargo test -p fraiseql-auth --test postgres_expiry_sweep -- --test-threads=1",
		// #1029 `integration` is a `mod` aggregator: the suite-coverage gate counted
		// only its entry file, scored it zero tests and dropped it from every check,
		// so its 36 tests — several of which self-skip without DATABASE_URL — ran
		// nowhere while the gate printed OK over them.
		"cargo test -p fraiseql-server --features auth --test integration -- --test-threads=1",
		// #389 session-state store (_system.session_state: TTL visibility, upsert, atomic
		// summary collapse, eviction sweep vs real PG).
		"cargo test -p fraiseql-auth --test session_state_integration -- --test-threads=1",
		// #431 inbound webhook pipeline (atomic idempotency claim + transactional handoff + RLS vs real PG).
		"cargo test -p fraiseql-webhooks --test inbound_pipeline_pg -- --test-threads=1",
		// #573 email reshaped as the reference native PullSource: ImapSource + EmailIngestSink
		// driven by the generic source envelope. Pure poll() tests (fetch/normalize/reset/
		// poison-skip) + PG e2e (ingest-once / re-poll dedup / UIDVALIDITY reset / two-poller
		// single-firing — the double-poll bug fix). The email test suite otherwise runs nowhere,
		// so this is also its first Dagger coverage.
		// Both filters go after `--`: cargo accepts a single TESTNAME, libtest ORs several.
		// #981: the whole inbound:: lib tree, not a source/sink filter — the
		// filtered form left tracking/correlation/imap/smtp/admin/config/cursor/
		// probe/warming and the spine/webhook modules executing in NO leg, and
		// the tracking suite's fixture-clock time bomb sat unobserved until it
		// crossed its 30-day TTL.
		"cargo test -p fraiseql-server --features inbound,inbound-email --lib inbound:: -- --test-threads=1",
		// #974: the storage bucket-policy admin API (PUT/GET/DELETE) against a
		// real policy store — the split-token separation, the request-time
		// refusal that leaves the running policy in place, and the wholesale
		// store-over-config precedence. The DB-less test leg skips it by name.
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --lib server::routing::storage_policy_admin_tests -- --test-threads=1",
		// #775: per-mailbox spine scoping + content-digest dedup key. Drives
		// EmailIngestSink against real PG (no IMAP, no real mailbox): the same
		// Message-ID to two mailboxes lands twice; a pre-claimed Message-ID cannot
		// suppress a genuine, differently-bodied message.
		"cargo test -p fraiseql-server --features inbound-email --test inbound_email_dedup_scope_pg -- --test-threads=1",
		// #573 source scheduler (Model B): the SourceQueryExecutor identity/tenant
		// seam, the SourcePoller build_host composition (cursor round-trip vs PG +
		// executor reachable via host.query), and the scheduler's schedulable/config
		// resolution. `--features sources` pulls the Deno path (compiled only). The
		// The fires_a_model_b_connector_end_to_end name-skip is lifted (#971):
		// the "V8 SIGSEGVs in the exec sandbox" diagnosis was #969's
		// second-isolate-per-process crash, fixed there — this run is the
		// sandbox re-test the issue asked for.
		"cargo test -p fraiseql-server --features sources --lib sources:: -- --test-threads=1",
		// P16 functions-runtime gate (#796/#803/#840/#841/#842): the cron
		// fire-window + cross-restart guard vs real PG (`cron::tests` reads
		// back `_fraiseql_cron_state`), and the dispatched host's identity /
		// send_email / env-allowlist wiring (`routes::after_mutation::tests`).
		// These are feature-gated on functions-runtime, which serverTestFeatures
		// deliberately omits — so before this line NO leg executed them.
		// (The #804 watchdog test is runtime-deno and stays local-only:
		// embedded V8 SIGSEGVs in the exec sandbox, see docs/contributing/dagger-parity-notes.md.)
		// #992: widened beyond the original P16 cron/after_mutation filters —
		// query_bridge, subsystems::loader, function_metrics and the
		// pg_function_dlq observers module are functions-runtime-gated too and
		// executed in no leg.
		"cargo test -p fraiseql-server --features functions-runtime,observers --lib -- cron:: routes::after_mutation:: query_bridge:: subsystems::loader:: function_metrics:: observers::pg_function_dlq:: --test-threads=1",
		// #896: the functions subsystem is configured from the schema the server was
		// built with, on BOTH serving entry points. Its own binary, and
		// functions-runtime-gated, so it belongs on this line rather than in
		// serverInProcessTests — which omits the feature and would run zero tests.
		"cargo test -p fraiseql-server --features functions-runtime --test functions_schema_seam_test",
		// #1082: same argument as the line above, and the case that proved the gate
		// blind. This binary is `#![cfg(feature = "functions-runtime")]` at file level,
		// which cargo cannot see, so naming it in serverInProcessTests built an EMPTY
		// binary that reported "test result: ok. 0 passed" and read as covered.
		"cargo test -p fraiseql-server --features functions-runtime --test functions_query_bridge_pin_test",
		// #429 wired saga forward execution + compensation + recovery + coordinator
		// + remote dispatch (saga): orchestration, rollback, and crash
		// recovery against the real Postgres saga store + entity mutations, plus the
		// mixed local/remote coordinator path. --include-ignored runs the #[ignore]d
		// PG tests (including the wired_execution_pg execute_step proofs).
		// test-utils is required by the remote_dispatch_pg module: the SSRF guard
		// blocks a loopback mock peer, so the coordinator's *_for_test / _unchecked
		// builders (compiled only under test-utils) drive the HTTP dispatch path.
		"cargo test -p fraiseql-federation --features saga,test-utils --test saga_integration -- --include-ignored --test-threads=1",
		// (#869: the rich-filter surface — including the #721 sql_templates_execute_pg
		// suite — was removed in P25: the compiler advertised WHERE operators the
		// runtime could never parse. The compiler↔runtime contract test now lives in
		// fraiseql-cli's lib tests: compiled_schema_advertises_no_unservable_where_operators.)
		// #823/#822/#569 — the first-run e2e: `fraiseql init` scaffolds, every
		// printed next step is executed, the scaffold's DDL is applied, and a
		// query + a mutation run through the real executor against PostgreSQL.
		"echo '### cargo test -p fraiseql-cli --test init_first_run_pg (#823/#822 first-run e2e)'",
		"cargo test -p fraiseql-cli --features test-postgres --test init_first_run_pg -- --test-threads=1",
		// #821 — `generate-views --validate` executes the DDL against PostgreSQL
		// in a rolled-back transaction; this suite proves it can PASS and FAIL.
		"echo '### cargo test -p fraiseql-cli --test generate_views_validate_pg (#821 validate can fail)'",
		"cargo test -p fraiseql-cli --features test-postgres --test generate_views_validate_pg -- --test-threads=1",
		// #384 — `compile --database` is a drift linter that can FAIL: error-severity
		// schema↔database drift exits non-zero and writes no artifact; doctor
		// reports the same drift as structured JSON. Proves both directions.
		"echo '### cargo test -p fraiseql-cli --test compile_drift_fail_pg (#384 drift linter can fail)'",
		"cargo test -p fraiseql-cli --features test-postgres --test compile_drift_fail_pg -- --test-threads=1",
		// #384 verification suites that had NEVER run with a database in any leg:
		// each self-skips without DATABASE_URL, and the workspace test leg (which
		// compiles them under --all-features) binds no Postgres — so all three read
		// green while running zero tests. The remaining self-skipping -cli suite
		// (cascade_rls_against_db) is tracked in its own issue.
		"echo '### cargo test -p fraiseql-cli --test mutation_contract_against_db --test doctor_against_db --test source_probe_against_db (#384 against-db gates)'",
		"cargo test -p fraiseql-cli --features test-postgres --test mutation_contract_against_db -- --test-threads=1",
		"cargo test -p fraiseql-cli --features test-postgres --test doctor_against_db -- --test-threads=1",
		"cargo test -p fraiseql-cli --features test-postgres --test source_probe_against_db -- --test-threads=1",
		// #960 — the cascade RLS conformance proof (2-tenant isolation incl. the
		// default-privilege-view LEAK proof) had never executed its assertions in
		// CI: it self-skips without DATABASE_URL and only the DB-less test leg
		// compiled it. One consolidated test — expect `1 passed, 0 skipped` here.
		"echo '### cargo test -p fraiseql-cli --test cascade_rls_against_db (#960 cascade RLS boundary proof)'",
		"cargo test -p fraiseql-cli --features test-postgres --test cascade_rls_against_db -- --test-threads=1",
		// #992: the remaining self-skipping fraiseql-cli against-db suites, same
		// shape as #960/#384 — green-while-running-zero-tests until named here.
		"echo '### cargo test -p fraiseql-cli remaining against-db suites (#992)'",
		"cargo test -p fraiseql-cli --features test-postgres --test setup_against_db -- --test-threads=1",
		"cargo test -p fraiseql-cli --features test-postgres --test sources_against_db -- --test-threads=1",
		"cargo test -p fraiseql-cli --features test-postgres --test validate_sql_sources_gate -- --test-threads=1",
		"cargo test -p fraiseql-cli --features test-postgres --test runtime_smoke -- --test-threads=1",
		"cargo test -p fraiseql-cli --features test-postgres --test perf_against_db -- --test-threads=1",
		// #936: the seed-fixture integrity gate runs LAST — any clobber a suite
		// above introduced fails here, naming the fixture.
		"echo '### cargo test -p fraiseql-db --test seed_fixture_integrity (#936 gate, runs last)'",
		"cargo test -p fraiseql-db --features postgres,wire-backend,test-postgres --test seed_fixture_integrity -- --test-threads=1",
		"echo 'test-integration OK: postgres suite passed'",
	}, "\n")

	// One pgService instance, bound to both the test container and the standby:
	// the suite writes on the primary and reads through the standby, so a second
	// pgService(source) call would leave the standby streaming from a database
	// nobody writes to and its measured lag permanently zero.
	primary := m.pgService(source)

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, primary).
		WithServiceBinding(pgStandbyBindHost, m.pgStandbyService(source, primary, "fraiseql_standby")).
		WithServiceBinding(pgFailoverBindHost, m.pgStandbyService(source, primary, pgFailoverSlot)).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("STANDBY_DATABASE_URL", standbyURL).
		WithEnvVariable("FAILOVER_STANDBY_DATABASE_URL", failoverURL).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationSaml runs the #381 SAML SP-login + ACS suite behind the non-default
// `auth-saml` feature. The libxml2 + xmlsec1 C stack samael needs lives in rustBase (the
// shared legs compile `--all-features`, which turns auth-saml on), so this leg only binds
// Postgres and runs the suite.
//
// The `--lib saml::` line runs the verification core + attack matrix (XSW / XXE /
// comment-truncation / replay / weak-digest) — no database, but it is what makes this leg
// able to go red on a verification regression. The `--test saml_sso` line binds Postgres
// and exercises the tenant-bounded trust policy against the durable account store.
func (m *FraiseqlCi) integrationSaml(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: saml (#381 SP login + ACS; auth-saml feature, xmlsec1 C stack)'",
		"echo \"### xmlsec1: $(xmlsec1-config --version)\"",
		// Verification core + full attack matrix (no DB) — proves the leg can go red.
		"bash tools/ci-target-canary.sh -- test -p fraiseql-auth --features auth-saml --lib saml::", // #880 canary
		"cargo test -p fraiseql-auth --features auth-saml --lib saml:: -- --test-threads=1",
		// Tenant-bounded trust policy ∘ PostgresAccountStore (reads DATABASE_URL via harness).
		"cargo test -p fraiseql-auth --features auth-saml --test saml_sso -- --test-threads=1",
		// #949 cross-replica SAML replay refusal. Two PgSamlReplayStores over two pools
		// stand in for two replicas: an assertion consumed by one must be refused by the
		// other. That assertion is structurally impossible for a single-process suite,
		// which is why the in-process DashMap survived this long. It lives here, not in
		// the `postgres` suite: the store is behind `auth-saml`, which only this leg
		// enables, and this leg binds Postgres too.
		"cargo test -p fraiseql-auth --features auth-saml --test postgres_saml_replay -- --test-threads=1",
		// #947: the per-tenant IdP store and registry — durable IdPs, hot reload, and
		// tenant-scoped resolution, plus the two properties that keep the store from
		// becoming a takeover primitive (a name is never reissued; a stored tenant-bound
		// IdP still cannot email-merge). Same leg for the same reason as the replay store.
		"cargo test -p fraiseql-auth --features auth-saml --test postgres_saml_idp_store -- --test-threads=1",
		// #381 P26: the SERVER mount — [saml] config → boot provisioning →
		// /auth/saml/login redirect with SAMLRequest; configured-but-broken
		// shapes (no pool, dud metadata) refuse to boot.
		// #947 adds the operator's path on top: manage IdPs over /api/saml/idps on a
		// running server and watch /auth/saml/login follow, scoped by tenant. The `--lib`
		// line runs the management router's own DB-free tests (router construction, and
		// that a tenant-bound IdP's inert email opt-in is reported as inert) — this leg is
		// the only one that enables `auth-saml` on fraiseql-server, so without it that
		// module is compiled out everywhere and reads as passing.
		"echo '### cargo test -p fraiseql-server --test saml_mount_e2e_pg (#381 server mount)'",
		"cargo test -p fraiseql-server --features auth-saml,auth --lib api::saml_idp_management -- --test-threads=1",
		"cargo test -p fraiseql-server --features auth-saml,auth --test saml_mount_e2e_pg -- --test-threads=1",
		// #946 SCIM provisioning. The store suite carries the property the feature exists
		// for — `active = false` blocks a local-password login, a credential SCIM never
		// touches — and the e2e drives the mounted surface over HTTP.
		"cargo test -p fraiseql-auth --test postgres_scim_provisioning -- --test-threads=1",
		"cargo test -p fraiseql-server --features auth,observers --test scim_provisioning_e2e_pg -- --test-threads=1",
		// #946's verification gate: a THIRD-PARTY SCIM client, because a suite we wrote
		// ourselves passes on the shapes we thought of. Okta's validator and the Entra
		// agent are hosted services needing a public URL and a vendor tenant, so neither
		// can run here; scim2-tester can, and it found real defects the hand-written tests
		// missed. Vendor validation stays a manual pre-release step.
		"apt-get install -y --no-install-recommends python3-venv >/dev/null",
		"python3 -m venv /tmp/scim-tester",
		"/tmp/scim-tester/bin/pip install --quiet scim2-tester httpx",
		"export FRAISEQL_SCIM_TESTER_PYTHON=/tmp/scim-tester/bin/python",
		"cargo test -p fraiseql-server --features auth,observers --test scim_conformance_e2e_pg -- --test-threads=1 --nocapture",
		"echo 'test-integration OK: saml suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationFederationCompose is the real-composer half of the golden two-subgraph
// federation suite (the #495/#496/#497/#498 cluster). It composes the committed
// FraiseQL-rendered subgraph SDLs with Apollo Federation v2 composition
// (@apollo/composition — the engine `rover supergraph compose` wraps) and asserts the
// positive case composes cleanly and the #497 two-change-log-owner case is rejected with
// INVALID_FIELD_SHARING. Node-only — no Rust, no DB.
//
// This is the step the old federation leg never ran: it routed a *pre-composed, committed*
// single-subgraph supergraph, so a broken subgraph SDL (missing scalar, dropped directive,
// snake_case change-log) sailed through. The committed SDL fixtures here are kept in
// lock-step with live FraiseQL rendering by the hermetic Rust test `federation_compose`,
// which runs in the postgres integration leg's `--test '*'` sweep (federation feature).
//
// Built on shellBase (the ghcr-mirrored ubuntu image, not Docker Hub) + apt nodejs/npm, so
// it adds no new base image. `run-compose-check.sh` restores deps via `npm ci` from the
// committed lockfile.
func (m *FraiseqlCi) integrationFederationCompose(ctx context.Context, source *dagger.Directory) (string, error) {
	script := strings.Join([]string{
		"set -e",
		"echo \"### node: $(node --version) / npm: $(npm --version)\"",
		"echo '### integration: federation-compose (Apollo Federation v2 composition of FraiseQL-rendered subgraph SDLs)'",
		"bash tools/federation/run-compose-check.sh",
		"echo 'test-integration OK: federation-compose suite passed'",
	}, "\n")

	return m.shellBase().
		WithExec([]string{"apt-get", "install", "-y", "--no-install-recommends", "nodejs", "npm"}).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationServer binds a seeded Postgres and runs fraiseql-server's
// database-query integration tests. They use try_database_url() + skip-on-None
// (no #[ignore]), so they run plainly and execute once DATABASE_URL is injected.
func (m *FraiseqlCi) integrationServer(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	standbyURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgStandbyBindHost, pgDatabase)

	script := strings.Join([]string{
		"set -e",
		suiteCountPrelude,
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: server database (Dagger-bound postgres)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-server --test database_query_test", // #880 canary
		"cargo test -p fraiseql-server --test database_query_test -- --test-threads=1",
		// Tier-C migrated (each helper creates + TRUNCATE/DROP its tables for shared-DB isolation).
		"cargo test -p fraiseql-server --test usage_postgres_backend_test -- --test-threads=1",
		"cargo test -p fraiseql-server --features observers --test observer_repository_test -- --test-threads=1",
		// #751: the inbound webhook replay defence is only provable against a real
		// idempotency claim — the defect it guards lived precisely where no
		// real-system test reached.
		"cargo test -p fraiseql-server --features inbound --test webhook_replay_header_dedup_pg -- --test-threads=1",
		// #1046: both dedup layers are scoped per route, not per provider. Two
		// routes on one provider, each sender's own event 1001, driven through the
		// real route + real claim + real spine. Only a live database can show it:
		// the ledger and the spine fail in the same direction, and fixing just the
		// ledger still answers 200 "processed" while dropping the message.
		"cargo test -p fraiseql-server --features inbound --test webhook_route_dedup_scope_pg -- --test-threads=1",
		// #781/#787: genuine provider-shaped deliveries (Slack timestamp threading,
		// Twilio public_url, LemonSqueezy hex) through the real route + real
		// idempotency claim, plus the boot-time route validation.
		"cargo test -p fraiseql-server --features inbound --test webhook_provider_matrix_pg -- --test-threads=1",
		// #794/#795 (CRITICAL): the analytics injection guards. Both holes were reachable
		// by any client that can POST a GraphQL query, and both were invisible to unit
		// tests because the allowlist that "covered" them was only ever consulted by a
		// planner the shipped binary never calls. This drives the real handler against a
		// real database and asserts both that the request is refused and that no catalog
		// data reaches the response.
		"cargo test -p fraiseql-server --test analytics_injection_e2e_pg -- --test-threads=1",
		// #386 — pgvector similarity search, executed: `nearest` row ORDER per
		// metric against real pgvector (the service image ships the extension),
		// threshold WHERE filters returning rows, dimension-mismatch and
		// binary-metric refusals. The first vector suite that runs SQL at all —
		// every prior "vector test" was sql.contains("<=>").
		"cargo test -p fraiseql-server --test graphql_vector_e2e_pg -- --test-threads=1",
		// #387 — GraphQL-over-SSE + root-field @stream, through the real
		// `Server::serve_on_listener` mount: negotiation is opt-in, batches
		// re-enter the full pipeline, auth is enforced before the stream and
		// re-checked per batch, the delivery survives request_timeout_secs, and
		// the compression predicate exempts text/event-stream.
		"cargo test -p fraiseql-server --test graphql_sse_e2e_pg -- --test-threads=1",
		// #938 — the `<name>Count` sibling, through the real mount. A count is a
		// second door onto the rows the list query guards, so most of what this
		// pins is refusals: the total ignores limit/offset but not `where`, and
		// it inherits inject-param tenant scoping, `requires_role` and the
		// anonymous refusal. A count that dropped the tenant filter returns no
		// row and leaks another tenant's row total.
		"cargo test -p fraiseql-server --test graphql_count_e2e_pg -- --test-threads=1",
		// #962 — the operator SQL console (G4: mount the full arbitrary-SQL
		// endpoint, gated). The most powerful endpoint the server has, so what
		// this pins is the containment, and every part of it is a *database*
		// behaviour no unit test can see: the preview really rolls back, the
		// commit opt-in really persists, the read-only token's write is refused
		// by the transaction's mode, the row cap and statement timeout fire, one
		// statement is all the protocol accepts, and impersonation sets the
		// session variables the executor would set.
		"cargo test -p fraiseql-server --features admin-sql --test admin_sql_console_e2e_pg -- --test-threads=1",
		// #966 — `requires_actor` enforced at execution on EVERY transport. The
		// claim is not "the predicate works" (a unit test covers that) but "no
		// door around it exists", so this drives GraphQL query, GraphQL mutation,
		// REST and MCP at the same restricted operation and requires the same
		// refusal from each. Its REST case found a real hole: the direct-read
		// path does not go through the GraphQL entry gates, so a predicate placed
		// only there served every restricted row over REST (#808's shape).
		"cargo test -p fraiseql-server --features rest,mcp --test actor_predicate_e2e_pg -- --test-threads=1",
		// #812/#739/#810: the REST read surface carried no authentication, discarded the
		// resolved tenant filter, and honoured `require_auth` on one route out of six.
		// None of it was visible to the existing REST suite, which builds its router with
		// `rest_router(...)` directly and so never exercises the server's actual mount.
		// This boots the real mount over a real socket against a real database and
		// asserts two tenants' rows stay apart — authenticated and anonymous.
		"cargo test -p fraiseql-server --features rest --test rest_tenant_isolation_e2e_pg -- --test-threads=1",
		// P13 — the REST write surface (#865) and the four defects that had to be green
		// before it could be mounted. Every one of these suites drives real PostgreSQL;
		// three of them drive the real `Server::serve_on_listener` mount rather than
		// calling `rest_router` directly, which is the distinction that made #812 and
		// #865 invisible for two releases.
		//
		// #865/#918/#846/#873: the write half had no production caller while the served
		// OpenAPI advertised every write path (405 on each), and the document was
		// generated from the route table rather than from the router, so the two could
		// disagree in both directions.
		"cargo test -p fraiseql-server --features rest --test rest_write_mount_e2e_pg -- --test-threads=1",
		// #391: durable submit/status/cancel through the production mount, with each of
		// P19's six recovery failure modes pinned (terminal-never-reclaimed, staleness-
		// gated claiming, claim-guarded completion, idempotent submission, truthful
		// cancellation, status-reads-the-row). Owns _system.async_operations + a
		// changelog table → serial.
		"cargo test -p fraiseql-server --test async_operations_e2e_pg -- --test-threads=1",
		// #376: an MCP-originated write is tagged transport=mcp in the change-log and
		// HS256 Bearer tokens authenticate MCP calls (auth parity with /graphql).
		// Recreates core.tb_entity_change_log → serial, changelog-owning binary.
		"cargo test -p fraiseql-server --features mcp --test mcp_transport_stamp_e2e_pg -- --test-threads=1",
		// #390: every authenticated write path (graphql + REST) records a derived,
		// unforgeable actor in the change-log; unauthenticated writes are refused, not
		// recorded unattributed. Recreates core.tb_entity_change_log → serial, and must
		// not share a process with another changelog-owning binary.
		"cargo test -p fraiseql-server --features rest --test actor_attribution_e2e_pg -- --test-threads=1",
		// #862/#913/#914/#916: bulk update/delete reported rows it never touched, and the
		// "at least one filter" guard was satisfied by parameters contributing no WHERE.
		// Asserts the *table*, because `affected_rows` is exactly the number #913 fabricates.
		"cargo test -p fraiseql-server --features rest --test rest_bulk_safety_e2e_pg -- --test-threads=1",
		// #863/#864: a client embedding filter could overwrite the parent join key and
		// return another parent's children, and nested embeddings were depth-validated
		// then silently dropped at execution.
		"cargo test -p fraiseql-server --features rest --test rest_embedding_safety_e2e_pg -- --test-threads=1",
		// #1266: the same surface, but served from a schema `fraiseql compile` produced
		// rather than a hand-built CompiledSchema. Until relationships had an authoring
		// producer, every embed request against a compiled schema was a 400 reading
		// "Available: none" — so the four fixes above were made in code no user reached.
		"cargo test -p fraiseql-server --features rest --test rest_embedding_compiled_schema_e2e_pg -- --test-threads=1",
		// #1271: a declared field name and the stored JSONB key it reads are two
		// different strings. The REST runner reads the whole `data` document and
		// projects in Rust, and that projector took the declared name verbatim
		// while the SQL projection generator and the `where` parser snake_case it
		// — so every multi-word camelCase field was absent from a REST 200 while
		// GraphQL served it correctly. Asserts REST against GraphQL on one seeded
		// row, which is the only way to see a key that is simply not there.
		"cargo test -p fraiseql-server --features rest --test rest_declared_name_stored_key_e2e_pg -- --test-threads=1",
		// #1268: the three export representations accepted a `?select=` naming an embed
		// or a count, validated it, and emitted rows without it — NDJSON dropped the key,
		// CSV and XLSX carried a column named after the relationship that was empty on
		// every row. Same feature requirement as the export-integrity suite below, and
		// the binary refuses to compile without it rather than dropping those cases.
		"cargo test -p fraiseql-server --features rest,export-csv,export-xlsx --test rest_export_embedding_e2e_pg -- --test-threads=1",
		// #811/#917: exports paginated through `variables`, which the executor ignores —
		// so every export either truncated to one page or looped forever emitting
		// duplicates. Needs the export features compiled in: the CSV and XLSX cases are
		// `#[cfg]`-gated, and under `--features rest` alone they would silently not run.
		"cargo test -p fraiseql-server --features rest,export-csv,export-xlsx --test rest_export_integrity_e2e_pg -- --test-threads=1",
		// #809: schema-per-tenant isolation was a single session `SET search_path` on
		// one pooled connection. Every other connection resolved against `public`, so
		// the leak is only visible under concurrency — a single-connection test passes
		// against the broken code, which is why it shipped.
		"cargo test -p fraiseql-server --test tenant_schema_isolation_e2e_pg -- --test-threads=1",
		// #859: DELETE /admin/tenants answered "removed" while every row survived, and
		// `destroy_tenant_schema` had no callers. Needs real DDL against a real database.
		"cargo test -p fraiseql-server --test tenant_lifecycle_e2e_pg -- --test-threads=1",
		// #628: the shipped multi-tenant examples now carry real RLS + session-variable
		// wiring. This applies their SQL, compiles them through the real compile path,
		// and asserts two tenants never cross — connecting as the examples' own
		// unprivileged role, because the harness role bypasses RLS entirely.
		"cargo test -p fraiseql-server --test example_multitenant_rls_e2e_pg -- --test-threads=1",
		// #748/#769/#768: the RBAC management API had never executed one statement
		// against PostgreSQL — its schema DDL did not parse, so setting `admin_token`
		// made the shipped -full binary refuse to boot. Its four test files were ~90
		// empty-bodied `#[test]` functions, which is why `cargo test` was green
		// throughout. Needs a real database (the subject is SQL PostgreSQL either
		// accepts or rejects) and a real boot (the DDL runs in the serve prologue).
		"cargo test -p fraiseql-server --features observers --test rbac_admin_e2e_pg -- --test-threads=1",
		// #749: five Studio admin write endpoints answered `{"success": true}` having
		// performed no side effect, and six reads answered a hard-coded empty
		// collection. The corpus derives its route list from the router source at
		// compile time, so the next handler that drifts from the 501 convention fails
		// here rather than shipping.
		"cargo test -p fraiseql-server --features observers --test studio_admin_no_fabricated_success_e2e -- --test-threads=1",
		// #858: MCP tool calls captured the default executor at session construction
		// and never consulted the tenant registry, so an authenticated caller read the
		// boot database and a suspended tenant kept reading over MCP while /graphql
		// answered 503. Needs two real per-tenant pools against one database — the
		// wrong-database read is silent, so nothing short of distinguishable rows in
		// two schemas can catch it.
		// #627 — the Postgres-backed API-key store: DDL executed against real PG,
		// full lifecycle (create/authenticate/revoke/rotate) plus the server mount
		// (admin REST management + live authenticator on one store).
		"echo '### cargo test -p fraiseql-server --test api_key_postgres_e2e_pg (#627 postgres api keys)'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --test api_key_postgres_e2e_pg -- --test-threads=1",
		// #934 service accounts under [auth_hs256]: the bearer-less x-api-key request
		// must reach the handler's ADR-0018 seam, while a credential-less request, an
		// invalid bearer and an unmatched secret all still 401 at the layer.
		"echo '### cargo test -p fraiseql-server --test hs256_service_account_e2e_pg (#934)'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --test hs256_service_account_e2e_pg -- --test-threads=1",
		// #1112 token revocation under [auth_hs256]: only the OIDC layer ever consulted
		// the revocation store, so a configured [security.token_revocation] was inert and
		// Studio's "revoke all sessions" reported success over tokens that kept working.
		// Both revocation shapes (single jti, revoke-all epoch) plus the accepting half
		// and require_jti, through the real mount.
		"echo '### cargo test -p fraiseql-server --test hs256_revocation_e2e_pg (#1112)'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --test hs256_revocation_e2e_pg -- --test-threads=1",
		// #368 P26 — social login through the shipped mount, against a stub IdP:
		// Google OIDC full loop, the GitHub /user/emails second hop (email-keyed
		// linking), the auth_start path bucket on /auth/v1/authorize (#788), and
		// state/provider refusals.
		"echo '### cargo test -p fraiseql-server --test social_oauth_e2e_pg (#368 social login mount)'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --test social_oauth_e2e_pg -- --test-threads=1",
		// #367 P26 — the [auth.local] reachability tier through the shipped mount:
		// password signup/login/reset, OTP whose identity resolves through the
		// account store, MFA whose Postgres enrollment survives a server restart,
		// anonymous signup, and the disabled-method-is-absent invariant.
		"echo '### cargo test -p fraiseql-server --test local_auth_e2e_pg (#367 local auth mount)'",
		// `inbound-email` is added explicitly: the password/OTP tests need the SMTP
		// transport, and without it `[auth.local]` refuses to boot (by design), so
		// they are feature-gated and would silently vanish from this leg otherwise.
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + ",inbound-email' --test local_auth_e2e_pg -- --test-threads=1",
		"cargo test -p fraiseql-server --features mcp --test mcp_tenant_dispatch_e2e_pg -- --test-threads=1",
		// pipeline_e2e is env-gated (FRAISEQL_PIPELINE_E2E); it compiles a schema and drives a server.
		"cargo test -p fraiseql-server --test pipeline_e2e_test -- --test-threads=1",
		// P18 verification gate (PG half): the FULL subscription delivery path —
		// tb_entity_change_log row → observer runtime loop → EventBridge → manager
		// → real WebSocket client. Proves a CUSTOM/Debezium-'r' snapshot row is
		// never delivered as a phantom create (#773) and a burst beyond the bridge
		// capacity is delivered completely (#772). --test-threads=1: the tests
		// share the observer schema DDL.
		"echo '### P18: subscription pipeline e2e (change log → runtime → bridge → /ws)'",
		"cargo test -p fraiseql-server --features observers --test subscription_pipeline_e2e_pg -- --include-ignored --test-threads=1",
		// #992: DB-needing binaries that ran in NO leg — each self-skips without
		// DATABASE_URL, and only the DB-less test leg compiled them.
		"echo '### cargo test -p fraiseql-server DB-backed strays (#992)'",
		"cargo test -p fraiseql-server --test revocation_pg_test -- --test-threads=1",
		"cargo test -p fraiseql-server --test sql_source_boot_check -- --test-threads=1",
		"cargo test -p fraiseql-server --test storage_wiring_test -- --test-threads=1",
		"cargo test -p fraiseql-server --test tenant_provisioning_test -- --test-threads=1",
		"cargo test -p fraiseql-server --test test_helpers -- --test-threads=1",
		"cargo test -p fraiseql-server --test observer_test_helpers -- --test-threads=1",
		"cargo test -p fraiseql-server --test wire_backend_feature_test -- --test-threads=1",
		// #936: the seed-fixture integrity gate runs LAST (pipeline_e2e ran above).
		"echo '### cargo test -p fraiseql-db --test seed_fixture_integrity (#936 gate, runs last)'",
		"cargo test -p fraiseql-db --features postgres,wire-backend,test-postgres --test seed_fixture_integrity -- --test-threads=1",
		"echo 'test-integration OK: server suite passed'",
	}, "\n")

	// One pgService instance bound to both the test container and the standby:
	// tenant_schema_isolation_e2e_pg's #957 case writes on the primary and reads
	// through the standby, so a second pgService(source) call would leave the
	// standby streaming from a database nobody writes to.
	//
	// Only the one standby — this suite has no promotion case, and the failover
	// standby is `pg_promote()`-destroyed by design (it lives in the postgres
	// suite, which owns that test).
	primary := m.pgService(source)

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, primary).
		WithServiceBinding(pgStandbyBindHost, m.pgStandbyService(source, primary, "fraiseql_standby")).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("STANDBY_DATABASE_URL", standbyURL).
		WithEnvVariable("FRAISEQL_PIPELINE_E2E", "1").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationWire binds a SCRAM-SHA-256 Postgres and runs the fraiseql-wire tests/*
// integration binaries (Tier-C migrated off testcontainers). The shared test helper
// (tests/common) reads DATABASE_URL via the harness, applies the wire test schema
// idempotently, and seeds it only when empty so all binaries share one bound database.
//
// The binaries are run individually (not `--tests`) to exclude `tls_integration`: it
// falls back from TLS_DATABASE_URL to DATABASE_URL, so it would try a TLS handshake
// against this non-TLS service — it has its own `tls` suite. Each binary uses
// --test-threads=1 (the bound database is shared across binaries).
func (m *FraiseqlCi) integrationWire(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, wireBindHost, pgDatabase)

	// Every tests/*.rs binary except tls_integration (own suite) and the common/
	// helper. metrics_recorder_scrape + protocol_decode_robustness were missing
	// from this list despite its claim (#992); the suite-coverage gate now fails
	// when a new fraiseql-wire binary lands in no leg.
	wireBins := []string{
		"client_integration", "config_integration", "integration", "integration_full",
		"integration_operators", "integration_pause_resume", "load_tests", "metrics_integration",
		"metrics_recorder_scrape", "property_protocol", "property_protocol_extended",
		"protocol_decode_robustness", "protocol_robustness_test",
		"rust_predicate_integration", "scram_integration", "sdk_sql_compliance_test",
		"streaming_integration", "stress_tests", "testcontainer_auth", "typed_streaming",
	}

	lines := []string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: wire (Dagger-bound SCRAM postgres; tests read DATABASE_URL via harness)'",
	}
	lines = append(lines, "bash tools/ci-target-canary.sh -- build -p fraiseql-wire --tests") // #880 canary
	for _, bin := range wireBins {
		lines = append(lines, "cargo test -p fraiseql-wire --test "+bin+" -- --test-threads=1")
	}
	lines = append(lines, "echo 'test-integration OK: wire suite passed'")
	script := strings.Join(lines, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(wireBindHost, m.wirePgService()).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// wirePgService is a postgres:16 with SCRAM-SHA-256 forced on (matching the auth
// config the old wire testcontainer used). It is otherwise blank: the wire test
// helper creates the `test` schema and seeds it on first connect (idempotent +
// seed-if-empty), so no initdb fixtures are mounted.
func (m *FraiseqlCi) wirePgService() *dagger.Service {
	return dag.Container().
		From(pgImage).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithEnvVariable("POSTGRES_HOST_AUTH_METHOD", "scram-sha-256").
		WithEnvVariable("POSTGRES_INITDB_ARGS", "--auth-host=scram-sha-256").
		WithExposedPort(5432).
		AsService()
}

// integrationStorage binds Postgres + an Azurite (Azure Blob) emulator + a
// fake-gcs-server (GCS) emulator and runs fraiseql-storage's Tier-C tests:
//   - lib metadata/migrations/routes tests (Postgres; create + TRUNCATE the metadata
//     table per test, --test-threads=1 for shared-DB isolation),
//   - the azure_emulator round-trip (feature azure-blob; reads AZURE_BLOB_ENDPOINT),
//   - the gcs_emulator round-trip (feature gcs; reads GCS_ENDPOINT).
//
// The routes tests use a local-filesystem backend (no S3/minio needed here).
func (m *FraiseqlCi) integrationStorage(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	azureEndpoint := fmt.Sprintf("http://%s:10000/devstoreaccount1", azuriteBindHost)
	gcsEndpoint := fmt.Sprintf("http://%s:4443", fakeGcsBindHost)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: storage (Dagger-bound postgres + azurite + fake-gcs)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-storage --lib", // #880 canary
		// routes::tests carries the #974 policy hot-reload module and policy::tests
		// its parse/round-trip module; both are covered by the filters already here.
		"cargo test -p fraiseql-storage --lib -- metadata::tests migrations::tests routes::tests uploads::tests policy::tests --test-threads=1",
		// #370: the render endpoint + transform hostile-input guards live behind
		// the `transforms` feature, so the line above (default features) compiles
		// them out entirely — without this second run the render route and the
		// decompression-bomb refusals execute in NO leg.
		// #973 widens this to `transforms-retarget` (which implies `transforms`)
		// so the seam-carving mode compiles and its fall-back threshold runs;
		// FRAISEQL_TEST_FONT points the text-watermark rasteriser at a real
		// typeface, because the font is the operator's, never vendored.
		"cargo test -p fraiseql-storage --features transforms-retarget --lib -- routes::tests::render_tests transforms::tests --test-threads=1",
		// #370: the server-side half — presets reaching BucketConfig (feature ON)
		// and the boot refusal when they are configured into a binary that cannot
		// serve them (feature OFF; a cfg(not(...)) test can never run in the
		// all-features leg, so both states are exercised explicitly).
		"cargo test -p fraiseql-server --features storage-transforms --lib -- server_config::tests::resolve_storage server_config::tests::transform_presets --test-threads=1",
		"cargo test -p fraiseql-server --lib -- server_config::tests::transform_presets --test-threads=1",
		// #371: bucket policies reach BucketConfig parsed, and an unparseable
		// policy refuses to boot rather than becoming a silently-denying rule.
		// #1099 adds the metadata half: `set_metadata` and `require_metadata`
		// reach BucketConfig through the TOML door (one spec type is shared with
		// the admin API, so a mismatch there refuses the boot outright), and the
		// one rule that cannot exist — granting `set_metadata` while also
		// carrying `require_metadata`, which would decide itself — is refused.
		"cargo test -p fraiseql-server --lib -- server_config::tests::resolve_storage_section_parses_bucket_policies server_config::tests::unparseable_policies server_config::tests::resolve_storage_section_parses_metadata_grants_and_conditions server_config::tests::a_self_deciding_metadata_rule_refuses_to_boot --test-threads=1",
		// #973: the render keys are validated at BOOT — a misspelt mode or
		// gravity, a quality on a losslessly-encoded format, an unreadable
		// watermark font, a bucket named after a reserved namespace. The
		// feature-OFF half runs in the line below it, because a
		// `cfg(not(feature))` test can never execute in a feature-ON build.
		"cargo test -p fraiseql-server --features storage-transforms --lib -- server_config::tests::misconfigured_render_keys server_config::tests::a_reserved_bucket_name --test-threads=1",
		"cargo test -p fraiseql-server --lib -- server_config::tests::render_keys_without_the_feature server_config::tests::a_reserved_bucket_name --test-threads=1",
		// #972: the resumable path for both emulator-backed backends — GCS
		// resumable sessions and Azure block lists — at the seam and end to end
		// through the Tus routes.
		"cargo test -p fraiseql-storage --features azure-blob --test azure_emulator -- --test-threads=1",
		"cargo test -p fraiseql-storage --features gcs --test gcs_emulator -- --test-threads=1",
		// #972 (from #369's deferred acceptance): drive the Tus endpoints with
		// tus-js-client, the reference client. Everything else in the repo
		// speaks the protocol the way the server does, so a shared misreading
		// reads as agreement. TUS_INTEROP is the provisioned-leg marker: with
		// it set the suite fails loudly instead of skipping.
		"bash tools/tus-interop/install.sh",
		"export TUS_INTEROP=1",
		"cargo test -p fraiseql-storage --test tus_interop -- --test-threads=1",
		"echo 'test-integration OK: storage suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		// nodejs/npm for the tus-js-client interop suite, fonts-dejavu-core for
		// #973's text watermark (whose typeface is the operator's and is
		// therefore never vendored into the crate). Installed here rather than
		// in rustBase: only this leg needs them, and rustBase is shared by
		// every heavy leg.
		WithExec([]string{
			"apt-get", "install", "-y", "--no-install-recommends",
			"nodejs", "npm", "fonts-dejavu-core",
		}).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithServiceBinding(azuriteBindHost, m.azuriteService()).
		WithServiceBinding(fakeGcsBindHost, m.fakeGcsService()).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("AZURE_BLOB_ENDPOINT", azureEndpoint).
		WithEnvVariable("GCS_ENDPOINT", gcsEndpoint).
		WithEnvVariable("FRAISEQL_TEST_FONT", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// azuriteService runs the Azurite blob emulator bound to 0.0.0.0:10000 (the default
// binds 127.0.0.1, unreachable from a client container). Data lives under /tmp (the
// default workdir is not writable).
func (m *FraiseqlCi) azuriteService() *dagger.Service {
	return dag.Container().
		From(azuriteImage).
		WithExposedPort(10000).
		AsService(dagger.ContainerAsServiceOpts{
			Args: []string{"azurite-blob", "--blobHost", "0.0.0.0", "--blobPort", "10000", "-l", "/tmp"},
		})
}

// fakeGcsService runs fake-gcs-server over plain HTTP with an in-memory backend.
// -external-url is the bind alias so the emulator's generated media links resolve from
// the test container.
func (m *FraiseqlCi) fakeGcsService() *dagger.Service {
	return dag.Container().
		From(fakeGcsImage).
		WithExposedPort(4443).
		AsService(dagger.ContainerAsServiceOpts{
			UseEntrypoint: true,
			Args:          []string{"-scheme", "http", "-backend", "memory", "-external-url", "http://" + fakeGcsBindHost + ":4443"},
		})
}

// integrationServerStorage binds a MinIO (S3-compatible) service AND Postgres, and runs
// the S3 backend against them from two angles:
//   - the storage object-safety gate (storage_minio_integration_test), which drives the
//     `fraiseql-storage` HTTP router end to end over MinIO + a real metadata table: the
//     presigned-upload ownership record (#866), the cross-owner overwrite refusal, the
//     orphan-object refusal, and the key-aliasing rejection (#813); and
//   - fraiseql-storage's own backend::s3 unit tests (audit #440), previously triple-gated
//     (aws-s3 not in any CI feature set, #[ignore], skip-if-no-S3_ENDPOINT) and therefore
//     never executed in CI — this is what let H40 (S3 NotFound detection) survive. Those
//     tests read S3_ENDPOINT and create their own per-test bucket.
//
// Both authenticate with the minioadmin/minioadmin dev credentials. Postgres is bound
// because the object-safety gate needs the metadata table: ownership, the overwrite gate
// and orphan refusal are all properties OF the metadata row, so without a database the
// gate would skip and report exactly like a pass. The fraiseql-storage run stays filtered
// to backend::s3 so an --include-ignored sweep does not also pull in that crate's own
// DB-backed metadata/migrations/routes tests, which the storage leg owns.
func (m *FraiseqlCi) integrationServerStorage(ctx context.Context, source *dagger.Directory) (string, error) {
	minioEndpoint := fmt.Sprintf("http://%s:9000", minioBindHost)
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: server-storage (Dagger-bound MinIO + Postgres; tests read MINIO_ENDPOINT / S3_ENDPOINT / DATABASE_URL)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-server --features aws-s3 --test storage_minio_integration_test", // #880 canary
		"cargo test -p fraiseql-server --features aws-s3 --test storage_minio_integration_test -- --test-threads=1",
		"echo '### integration: storage backend::s3 unit tests (audit #440; read S3_ENDPOINT)'",
		"cargo test -p fraiseql-storage --features aws-s3 backend::s3 -- --include-ignored --test-threads=1",
		"echo 'test-integration OK: server-storage suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(minioBindHost, m.minioService()).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithEnvVariable("MINIO_ENDPOINT", minioEndpoint).
		WithEnvVariable("DATABASE_URL", dbURL).
		// fraiseql-storage's s3/tests.rs reads S3_ENDPOINT (or AWS_ENDPOINT_URL), whereas
		// the server test reads MINIO_ENDPOINT; both point at the same bound MinIO.
		WithEnvVariable("S3_ENDPOINT", minioEndpoint).
		// The S3 backend resolves credentials from the AWS env chain at request time
		// (not just when constructed), so inject them as real process env, not only via
		// the test's temp_env scope.
		WithEnvVariable("AWS_ACCESS_KEY_ID", minioUser).
		WithEnvVariable("AWS_SECRET_ACCESS_KEY", minioPass).
		WithEnvVariable("AWS_DEFAULT_REGION", "us-east-1").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationFederation runs fraiseql-server's federation integration tests as a real
// enforcing gate. The in-process tests (SDL, _entities by id, missing-entity null) run
// against a seeded Postgres bound as DATABASE_URL with FEDERATION_TESTS=1. The
// service-backed tests drive two FraiseQL subgraph servers (users + reviews) and an
// Apollo Router routing to subgraph A — all built from the same federation-feature
// binary and bound as Dagger services. A dedicated target cache volume keeps the
// federation-feature artifacts apart from the default integration build.
func (m *FraiseqlCi) integrationFederation(ctx context.Context, source *dagger.Directory) (string, error) {
	usersURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, fedUsersBindHost, pgDatabase)
	routerURL := fmt.Sprintf("http://%s:4000", apolloRouterBindHost)
	subgraphAURL := fmt.Sprintf("http://%s:8815", fedSubgraphABindHost)
	subgraphBURL := fmt.Sprintf("http://%s:8816", fedSubgraphBBindHost)

	binary := m.fedServerBinary(source)
	pgUsers := m.fedPgService(source, "init_users.sql")
	pgReviews := m.fedPgService(source, "init_reviews.sql")
	subgraphA := m.fedSubgraphService(source, binary, "schema_users.json", pgUsers, fedUsersBindHost, "0.0.0.0:8815", 8815)
	subgraphB := m.fedSubgraphService(source, binary, "schema_reviews.json", pgReviews, fedReviewsBindHost, "0.0.0.0:8816", 8816)

	supergraph, err := source.
		File("crates/fraiseql-core/tests/federation/fixtures/supergraph_single.graphql").
		Contents(ctx)
	if err != nil {
		return "", fmt.Errorf("read supergraph fixture: %w", err)
	}
	supergraph = strings.ReplaceAll(supergraph, "__SUBGRAPH_URL__", subgraphAURL+"/graphql")
	router := m.apolloRouterService(supergraph, subgraphA)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: federation (in-process _entities + Apollo Router + cross-subgraph)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-server --features federation --test federation_integration_test", // #880 canary
		"cargo test -p fraiseql-server --features federation --test federation_integration_test -- --test-threads=1",
		"echo 'test-integration OK: federation suite passed'",
	}, "\n")

	return m.fedBase(source).
		WithServiceBinding(fedUsersBindHost, pgUsers).
		WithServiceBinding(fedSubgraphABindHost, subgraphA).
		WithServiceBinding(fedSubgraphBBindHost, subgraphB).
		WithServiceBinding(apolloRouterBindHost, router).
		WithEnvVariable("DATABASE_URL", usersURL).
		WithEnvVariable("FEDERATION_TESTS", "1").
		WithEnvVariable("ROUTER_URL", routerURL).
		WithEnvVariable("SUBGRAPH_A_URL", subgraphAURL).
		WithEnvVariable("SUBGRAPH_B_URL", subgraphBURL).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// fedBase mounts the source on a dedicated federation-feature target cache volume.
func (m *FraiseqlCi) fedBase(source *dagger.Directory) *dagger.Container {
	return m.rustBaseFor(rustMsrv).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume(fedTargetVol)).
		WithEnvVariable("RUST_LOG", "debug")
}

// fedServerBinary builds the fraiseql-server binary with the federation feature and
// returns it as a File (extracted from the cache-mounted target dir to a plain path).
func (m *FraiseqlCi) fedServerBinary(source *dagger.Directory) *dagger.File {
	built := m.rustBaseFor(rustMsrv).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume(fedTargetVol)).
		WithExec([]string{
			"bash", "-c",
			"bash tools/ci-target-canary.sh -- build -p fraiseql-server --features federation && cp target/debug/fraiseql-server /usr/local/bin/fraiseql-server",
		})
	return built.File("/usr/local/bin/fraiseql-server")
}

// fedSubgraphService runs the federation-feature server binary as a bound subgraph
// service: it loads the given compiled schema and binds its own seeded Postgres.
func (m *FraiseqlCi) fedSubgraphService(source *dagger.Directory, binary *dagger.File, schemaFile string, pgSvc *dagger.Service, pgAlias string, bindAddr string, port int) *dagger.Service {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgAlias, pgDatabase)
	schema := source.File("crates/fraiseql-server/tests/fixtures/federation/" + schemaFile)

	return m.rustBase().
		WithFile("/usr/local/bin/fraiseql-server", binary).
		WithFile("/schema.compiled.json", schema).
		WithServiceBinding(pgAlias, pgSvc).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("FRAISEQL_SCHEMA_PATH", "/schema.compiled.json").
		WithEnvVariable("FRAISEQL_BIND_ADDR", bindAddr).
		WithEnvVariable("FRAISEQL_INTROSPECTION_ENABLED", "true").
		WithEnvVariable("FRAISEQL_INTROSPECTION_REQUIRE_AUTH", "false").
		WithEnvVariable("FRAISEQL_ENV", "development").
		WithEnvVariable("RUST_LOG", "info").
		WithExposedPort(port).
		AsService(dagger.ContainerAsServiceOpts{Args: []string{"/usr/local/bin/fraiseql-server"}})
}

// apolloRouterService runs Apollo Router with the given (placeholder-substituted)
// supergraph, serving GraphQL at /graphql on 0.0.0.0:4000. Subgraph A is bound so the
// router can resolve its alias when fetching from the subgraph.
func (m *FraiseqlCi) apolloRouterService(supergraph string, subgraphA *dagger.Service) *dagger.Service {
	const routerConfig = "include_subgraph_errors:\n  all: true\nsupergraph:\n  listen: 0.0.0.0:4000\n  path: /graphql\n"

	return dag.Container().
		From(apolloRouterImage).
		WithServiceBinding(fedSubgraphABindHost, subgraphA).
		WithNewFile("/supergraph.graphql", supergraph).
		WithNewFile("/router.yaml", routerConfig).
		WithEnvVariable("APOLLO_TELEMETRY_DISABLED", "true").
		WithExposedPort(4000).
		AsService(dagger.ContainerAsServiceOpts{
			UseEntrypoint: true,
			Args:          []string{"--config", "/router.yaml", "--supergraph", "/supergraph.graphql"},
		})
}

// fedPgService returns a postgres:16 seeded with a federation fixture
// (tests/fixtures/federation/<initSQL>) mounted into the initdb directory.
func (m *FraiseqlCi) fedPgService(source *dagger.Directory, initSQL string) *dagger.Service {
	initDir := dag.Directory().
		WithFile("00-"+initSQL, source.File("crates/fraiseql-server/tests/fixtures/federation/"+initSQL))

	return dag.Container().
		From(pgImage).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithDirectory("/docker-entrypoint-initdb.d", initDir).
		WithExposedPort(5432).
		AsService()
}

// minioService runs MinIO bound on 0.0.0.0:9000 with dev root credentials.
func (m *FraiseqlCi) minioService() *dagger.Service {
	return dag.Container().
		From(minioImage).
		WithEnvVariable("MINIO_ROOT_USER", minioUser).
		WithEnvVariable("MINIO_ROOT_PASSWORD", minioPass).
		WithExposedPort(9000).
		AsService(dagger.ContainerAsServiceOpts{
			UseEntrypoint: true,
			Args:          []string{"server", "/data", "--address", "0.0.0.0:9000"},
		})
}

// integrationRedis binds Redis + a seeded Postgres and runs the Redis-backed
// suites: fraiseql-core APQ storage and fraiseql-observers queue/lease. Those lib
// tests are #[ignore]d ("requires Redis running") and read REDIS_URL / DATABASE_URL.
func (m *FraiseqlCi) integrationRedis(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	redisURL := fmt.Sprintf("redis://%s:6379", redisBindHost)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: redis (core APQ + observers queue/lease + #428 cache-invalidation) — Dagger-bound redis+postgres'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-core --features redis-apq --lib redis", // #880 canary
		"cargo test -p fraiseql-core --features redis-apq --lib redis -- --ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features 'caching,queue,redis-lease' --lib -- --ignored --test-threads=1",
		// #844: the job-queue worker's dispatch/timeout/DLQ tests. The mock-queue
		// tests are NOT #[ignore]d (no external service) but compile only with the
		// `queue` feature, which no workspace-level step enables — without this
		// line they run in no leg. The #[ignore]d redis variant runs in the
		// `--ignored` line above.
		"cargo test -p fraiseql-observers --features queue --lib job_queue -- --test-threads=1",
		// #428: real cache-invalidation transport. The pure glob/escaping unit
		// tests run non-ignored (the `--ignored` lib line above skips them); the
		// integration binary seeds the bound Redis and asserts real UNLINKs
		// (its Redis-touching tests self-skip when REDIS_URL is unset).
		"cargo test -p fraiseql-observers --features caching --lib cache::tests::glob_tests -- --test-threads=1",
		"cargo test -p fraiseql-observers --features 'caching,testing' --test cache_invalidation_redis -- --test-threads=1",
		// #985: the same transport through the SERVER's boot path. The library
		// slice above passed for years while no fraiseql.toml could reach it —
		// the executor mount was an alternative constructor the server does not
		// call, the [observers.runtime] table rejected a `redis` key, and the
		// server crate never enabled fraiseql-observers/caching. This binary
		// boots ObserverRuntime from config and asserts the key really leaves
		// the bound Redis, so a library-only regression cannot read green.
		"cargo test -p fraiseql-server --features observers-cache --test observer_cache_mount_redis -- --ignored --test-threads=1",
		// #770: cross-replica token revocation through the shipped construction
		// path against the bound Redis (skips when REDIS_URL is unset).
		"cargo test -p fraiseql-server --features 'auth,redis-rate-limiting' --test revocation_redis_test -- --test-threads=1",
		// The #[ignore]d Redis-backed rate-limiter and PKCE-store tests. Before
		// #770/#777 these ran in NO leg (this leg only ran core + observers), so
		// the cross-instance sharing they assert was never CI-verified.
		"cargo test -p fraiseql-server --features redis-rate-limiting --lib middleware::rate_limit -- --ignored --test-threads=1",
		// #992: Redis-needing binaries that ran in NO leg (each self-skips
		// without REDIS_URL, and only service-less legs compiled them).
		"echo '### redis-backed strays (#992)'",
		// --include-ignored: the redis-touching halves are #[ignore]d and this is
		// the leg that binds the Redis they need.
		"cargo test -p fraiseql-server --features redis-apq --test redis_apq_integration_test -- --include-ignored --test-threads=1",
		// redis-rate-limiting: the whole binary is #![cfg]-gated on it and runs
		// ZERO tests without (verified).
		"cargo test -p fraiseql-auth --features redis-rate-limiting --test redis_failover_test -- --include-ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features 'caching,queue,redis-lease,testing' --test integration_test -- --test-threads=1",
		"cargo test -p fraiseql-auth --features redis-pkce --lib redis_pkce -- --ignored --test-threads=1",
		"echo 'test-integration OK: redis suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithServiceBinding(redisBindHost, m.redisService()).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("TEST_DATABASE_URL", dbURL).
		WithEnvVariable("REDIS_URL", redisURL).
		// The #844 job-queue tests dispatch real webhooks to an in-process
		// wiremock on 127.0.0.1, which the SSRF guard blocks. The bypass is
		// honoured only outside a production environment, and an *unset*
		// FRAISEQL_ENV reads as production (#816) — both vars are required.
		WithEnvVariable("FRAISEQL_ENV", "development").
		WithEnvVariable("FRAISEQL_OBSERVERS_ALLOW_INSECURE", "true").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationVault binds a Vault dev-mode service and runs fraiseql-server's
// secrets-manager integration tests (#[ignore]d "requires vault"); they read
// VAULT_ADDR / VAULT_TOKEN.
func (m *FraiseqlCi) integrationVault(ctx context.Context, source *dagger.Directory) (string, error) {
	vaultAddr := fmt.Sprintf("http://%s:8200", vaultBindHost)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: vault secrets manager (Dagger-bound vault dev)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-server --features secrets --test secrets_manager_integration_test", // #880 canary
		"cargo test -p fraiseql-server --features secrets --test secrets_manager_integration_test -- --ignored --test-threads=1",
		"echo 'test-integration OK: vault suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(vaultBindHost, m.vaultService()).
		WithEnvVariable("VAULT_ADDR", vaultAddr).
		WithEnvVariable("VAULT_TOKEN", vaultToken).
		// Every insecure-mode escape hatch below is honoured only outside a
		// production environment, and an *unset* FRAISEQL_ENV now reads as
		// production (secure by default, #816). A leg that opts into a hatch has
		// to declare the environment too, or the hatch is silently refused and
		// the suite fails on a guard it thought it had opted out of.
		WithEnvVariable("FRAISEQL_ENV", "development").
		WithEnvVariable("FRAISEQL_VAULT_ALLOW_INSECURE", "true").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationTLS: a TLS-enabled Postgres and the
// fraiseql-wire TLS integration tests. The CA + server cert are pre-generated once
// (SAN includes the bind alias so rustls servername verification passes); the server
// cert goes into the pg service and the CA cert is injected DIRECTLY into the test
// container as a File (deterministic — Dagger cache volumes don't reliably share a
// running service's writes with a client container). Tests are skip-on-None
// (TLS_DATABASE_URL + TLS_TEST_CA_CERT), not #[ignore]d, so they run without --ignored.
func (m *FraiseqlCi) integrationTLS(ctx context.Context, source *dagger.Directory) (string, error) {
	tlsURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, tlsBindHost, pgDatabase)
	certs := m.tlsCerts()

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: tls (fraiseql-wire over TLS to a Dagger-bound postgres-tls)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-wire --test tls_integration", // #880 canary
		"cargo test -p fraiseql-wire --test tls_integration -- --test-threads=1",
		// #801/#824: the connection pool's own TLS. Proves verify-full rejects an
		// untrusted chain, accepts it once the CA is supplied, and that the session
		// is encrypted according to pg_stat_ssl rather than according to the client.
		"echo '### integration: tls (fraiseql-db connection pool)'",
		"cargo test -p fraiseql-db --features postgres,test-postgres --test postgres_tls_verify_test -- --test-threads=1",
		"echo 'test-integration OK: tls suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(tlsBindHost, m.tlsPgService(certs)).
		WithFile("/ca.crt", certs.File("ca.crt")).
		WithEnvVariable("TLS_DATABASE_URL", tlsURL).
		WithEnvVariable("TLS_TEST_CA_CERT", "/ca.crt").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// tlsCerts pre-generates a CA + server cert chain whose SAN covers the bind alias
// (postgres-tls), localhost, and 127.0.0.1. Returns a directory with ca.crt,
// server.crt, server.key (key world-readable so the pg init can copy it; the init
// re-chmods to 600 under the postgres user).
func (m *FraiseqlCi) tlsCerts() *dagger.Directory {
	gen := strings.Join([]string{
		"set -e",
		"mkdir -p /out && cd /out",
		"openssl req -x509 -newkey rsa:2048 -keyout ca.key -out ca.crt -days 365 -nodes" +
			" -subj '/CN=fraiseql-test-ca'" +
			" -addext 'basicConstraints=critical,CA:TRUE' -addext 'keyUsage=critical,keyCertSign,cRLSign'",
		"openssl req -newkey rsa:2048 -keyout server.key -out server.csr -days 365 -nodes -subj '/CN=" + tlsBindHost + "'",
		"openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out server.crt -days 365" +
			" -extfile <(printf 'subjectAltName=DNS:" + tlsBindHost + ",DNS:localhost,IP:127.0.0.1\\nbasicConstraints=CA:FALSE')",
		"chmod 644 ca.crt server.crt server.key",
	}, "\n")

	return dag.Container().
		From(pgImage). // the postgres image ships openssl
		WithExec([]string{"bash", "-c", gen}).
		Directory("/out")
}

// tlsPgService is a postgres:16 that enables TLS using the pre-generated server cert.
// A small initdb script copies the cert/key into $PGDATA (as the postgres user, then
// chmod 600), turns on ssl, and seeds v_test_entity (the wire TLS tests query it and
// expect >= 10 rows).
func (m *FraiseqlCi) tlsPgService(certs *dagger.Directory) *dagger.Service {
	const initScript = `#!/bin/bash
set -e
cp /tls-certs/server.crt "$PGDATA/server.crt"
cp /tls-certs/server.key "$PGDATA/server.key"
chmod 600 "$PGDATA/server.key"
{ echo "ssl = on"; echo "ssl_cert_file = 'server.crt'"; echo "ssl_key_file = 'server.key'"; } >> "$PGDATA/postgresql.conf"
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<'EOSQL'
    CREATE TABLE IF NOT EXISTS test_entities (
        id   SERIAL PRIMARY KEY,
        name TEXT  NOT NULL,
        data JSONB NOT NULL DEFAULT '{}'
    );
    INSERT INTO test_entities (name, data)
    SELECT 'entity_' || i, jsonb_build_object('index', i, 'tag', md5(i::text))
    FROM generate_series(1, 20) AS i;
    CREATE OR REPLACE VIEW v_test_entity AS SELECT id, name, data FROM test_entities;
EOSQL
`
	initDir := dag.Directory().WithNewFile("00-tls.sh", initScript)

	return dag.Container().
		From(pgImage).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithDirectory("/tls-certs", certs).
		WithDirectory("/docker-entrypoint-initdb.d", initDir).
		WithExposedPort(5432).
		AsService()
}

// integrationHTTPE2e boots the actual
// fraiseql-server binary as a bound Dagger service (which itself binds an
// e2e-seeded Postgres), then drives it over HTTP from the test container. The e2e
// tests are skip-on-None (FRAISEQL_TEST_URL); legacy's --ignored ran 0, so they run
// without --ignored here.
func (m *FraiseqlCi) integrationHTTPE2e(ctx context.Context, source *dagger.Directory) (string, error) {
	server := m.serverE2eService(source)
	testURL := fmt.Sprintf("http://%s:8815", serverBindHost)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: http-e2e (fraiseql-server binary as a bound service)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-server --test http_server_e2e_test", // #880 canary
		"cargo test -p fraiseql-server --test http_server_e2e_test -- --test-threads=4",
		"cargo test -p fraiseql-server --test concurrent_load_test -- --test-threads=1",
		"echo 'test-integration OK: http-e2e suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(serverBindHost, server).
		WithEnvVariable("FRAISEQL_TEST_URL", testURL).
		WithEnvVariable("FRAISEQL_METRICS_TOKEN", e2eMetricsToken).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// serverE2eService builds the fraiseql-server binary, then runs it as a service
// bound to an e2e-seeded Postgres. It binds 0.0.0.0 (not 127.0.0.1) so the bound
// test container can reach it. Dagger starts the Postgres dependency (and waits for
// its port) before the server starts, and the caller waits for :8815 before testing.
func (m *FraiseqlCi) serverE2eService(source *dagger.Directory) *dagger.Service {
	// `integ4-` bump (2026-07-27): see the note on the sibling volume below.
	const targetVol = "fraiseql-rust-target-integ4-1-92"
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	// Build the binary and copy it out of the (cache-mounted) target dir to a plain
	// path so it can be extracted as a File into the runtime service container.
	built := m.rustBaseFor(rustMsrv).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume(targetVol)).
		WithExec([]string{
			"bash", "-c",
			"bash tools/ci-target-canary.sh -- build -p fraiseql-server && cp target/debug/fraiseql-server /usr/local/bin/fraiseql-server",
		})
	binary := built.File("/usr/local/bin/fraiseql-server")
	schema := source.File("docker/e2e/schema.compiled.json")

	// rustBase carries the runtime libs (openssl, etc.) the binary links against.
	return m.rustBase().
		WithFile("/usr/local/bin/fraiseql-server", binary).
		WithFile("/schema.compiled.json", schema).
		WithServiceBinding(pgBindHost, m.pgE2eService(source)).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("FRAISEQL_SCHEMA_PATH", "/schema.compiled.json").
		WithEnvVariable("FRAISEQL_BIND_ADDR", "0.0.0.0:8815").
		WithEnvVariable("FRAISEQL_METRICS_ENABLED", "true").
		WithEnvVariable("FRAISEQL_METRICS_TOKEN", e2eMetricsToken).
		WithEnvVariable("FRAISEQL_INTROSPECTION_ENABLED", "true").
		WithEnvVariable("FRAISEQL_INTROSPECTION_REQUIRE_AUTH", "false").
		WithEnvVariable("FRAISEQL_ENV", "development").
		WithEnvVariable("RUST_LOG", "info").
		WithExposedPort(8815).
		AsService(dagger.ContainerAsServiceOpts{Args: []string{"/usr/local/bin/fraiseql-server"}})
}

// pgE2eService is a postgres:16 seeded with the E2E fixture (docker/e2e/
// init-postgres.sql — tb_user + v_users), distinct from the main integration seed.
func (m *FraiseqlCi) pgE2eService(source *dagger.Directory) *dagger.Service {
	initDir := dag.Directory().
		WithFile("00-init.sql", source.File("docker/e2e/init-postgres.sql"))

	return dag.Container().
		From(pgImage).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithDirectory("/docker-entrypoint-initdb.d", initDir).
		WithExposedPort(5432).
		AsService()
}

// integrationObservers binds Postgres + Redis + NATS and runs the observer-runtime
// integration suites: PostgreSQL NOTIFY
// transport, storage/lease (Redis), the PG+NATS bridge, and fraiseql-server's
// observer runtime. All read their service URLs from env (DATABASE_URL / REDIS_URL /
// NATS_URL); the bridge's NatsConfig url is overridden from NATS_URL.
func (m *FraiseqlCi) integrationObservers(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	redisURL := fmt.Sprintf("redis://%s:6379", redisBindHost)
	natsURL := fmt.Sprintf("nats://%s:4222", natsBindHost)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: observers (Dagger-bound postgres+redis+nats)'",
		// postgres_notify lib tests are skip-on-None (not #[ignore]d); run name-filtered
		// (no --ignored) so the NOTIFY transport tests exercise the bound Postgres.
		"bash tools/ci-target-canary.sh -- test -p fraiseql-observers --features postgres --lib postgres_notify", // #880 canary
		"cargo test -p fraiseql-observers --features postgres --lib postgres_notify -- --test-threads=1",
		// Lease/storage: kept as the legacy `--lib --ignored` no-op. Those tests are
		// skip-on-None (not #[ignore]d) so this runs 0; running them unfiltered pulls in
		// the SSRF-guard unit tests, which assert the guard is ON and so fail under this
		// suite's FRAISEQL_OBSERVERS_ALLOW_INSECURE=true. Lease coverage gap == legacy.
		"cargo test -p fraiseql-observers --features 'postgres,caching,redis-lease' --lib -- --ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features 'postgres,nats' --test bridge_integration -- --ignored --test-threads=1",
		"cargo test -p fraiseql-server --features observers-nats --test observer_runtime_integration_test -- --ignored --test-threads=1",
		// #928: the observer E2E suite. It had never run in any leg — none of its
		// tests constructed a runtime, so all 8 waited for webhooks nothing could
		// send. Repaired in P17 (each test drives a real ObserverRuntime); gated
		// here so it can never silently die again.
		"cargo test -p fraiseql-server --features observers --test observer_e2e_test -- --ignored --test-threads=1",
		// #349 email happy-path: send through lettre to the bound MailHog sink and
		// assert the message arrived (real SMTP wire format, not a stub).
		"cargo test -p fraiseql-observers --test smtp_integration -- --ignored --test-threads=1",
		// #443 / #437 F6: change-log RLS isolation under a NOBYPASSRLS role (the
		// superuser DATABASE_URL would mask the policy). The test creates its own
		// tenant/consumer roles off the superuser connection — no extra env needed.
		"cargo test -p fraiseql-observers --features postgres --test rls_isolation -- --ignored --test-threads=1",
		// #390 (P29): the change-log contract suite — including the actor_type CHECK
		// constraint tests — plus the views + capture-trigger suites. All three binaries
		// had run in NO leg since they were written (the #[ignore]-with-no---ignored-leg
		// form of the skip-green pattern); capture_trigger is order-independent since it
		// drops the ambient table (an ambient NOT-NULL object_data red-flagged it).
		"cargo test -p fraiseql-observers --features postgres --test entity_change_log_contract -- --ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features postgres --test changelog_views -- --ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features postgres --test capture_trigger -- --ignored --test-threads=1",
		// #935: the two-session commit-order repro. Identity pks are allocated in
		// statement order but become visible in commit order, so the old
		// `pk > watermark` cursor permanently skipped late-committing rows and
		// their observers never fired. Needs a real Postgres — a mock cannot
		// produce the divergence — and its own binary, since it owns
		// core.tb_entity_change_log and the dispatch ledger.
		"cargo test -p fraiseql-observers --features postgres --test change_log_commit_order_pg -- --ignored --test-threads=1",
		// #573 source coordination primitives: the advisory-lease mutual-exclusion
		// + crash-safety boundary (Phase 00), and the source cursor store (CAS
		// advance, in-tx rollback, deny-by-default RLS) + single-firing runner
		// (Phase 01), all vs the bound Postgres. Self-skip on no DATABASE_URL (not
		// #[ignore]d) → run WITHOUT --ignored, as separate binaries so they never
		// pull in the SSRF-guard unit tests this suite disables.
		"cargo test -p fraiseql-observers --features postgres --test advisory_lease_pg -- --test-threads=1",
		"cargo test -p fraiseql-observers --features postgres --test source_cursor_pg -- --test-threads=1",
		// #382 outbound CDC: the drain-worker state machine vs the bound Postgres
		// (stub sink, no broker) + the NATS JetStream sink end-to-end vs the bound
		// JetStream (DATABASE_URL + NATS_URL + FRAISEQL_NATS_ALLOW_PLAINTEXT below).
		"cargo test -p fraiseql-cdc-sinks --test cdc_drain_pg -- --ignored --test-threads=1",
		"cargo test -p fraiseql-cdc-sinks --features cdc-nats-jetstream --test cdc_nats_e2e -- --ignored --test-threads=1",
		// #975: the same drain-through assertions against the bound Kafka broker
		// (DATABASE_URL + KAFKA_BOOTSTRAP + FRAISEQL_KAFKA_ALLOW_PLAINTEXT below).
		// Asserted THROUGH the drain, and the entity-keyed partition affinity is
		// only meaningful because the suite creates its topic with 6 partitions.
		"cargo test -p fraiseql-cdc-sinks --features cdc-kafka --test cdc_kafka_e2e -- --ignored --test-threads=1",
		// #1102: the SERVER's [subscription_kafka] mount against the same broker. The
		// whole issue was a transport no configuration could reach, so an adapter-level
		// test does not close it — this one writes the section, mounts it the way the
		// server does, and reads the message back with a real consumer. Both directions
		// in one binary: the second test asserts the same endpoint is refused without
		// the development opt-in, so a guard that has stopped refusing cannot hide
		// behind the passing happy path.
		//
		// --test-threads=1 because both tests scope FRAISEQL_ENV /
		// FRAISEQL_KAFKA_ALLOW_PLAINTEXT with temp_env, which is process-global.
		"cargo test -p fraiseql-server --features subscription-kafka --test subscription_kafka_mirror_e2e -- --test-threads=1",
		// #975: the same drain-through assertions against the bound LocalStack
		// Kinesis (DATABASE_URL + KINESIS_ENDPOINT + FRAISEQL_KINESIS_ALLOW_PLAINTEXT
		// below). The entity-keyed shard affinity is only meaningful because the
		// suite creates its stream with 4 shards — on a single-shard stream the
		// assertion passes with any partition key at all, including a per-message one.
		"cargo test -p fraiseql-cdc-sinks --features cdc-kinesis --test cdc_kinesis_e2e -- --ignored --test-threads=1",
		// #382: the SERVER's outbound-CDC mount — [cdc_outbound] built through
		// the real path drains the outbox to the bound JetStream, and an
		// unreachable broker refuses to boot. Before this the drain engine was
		// library-only: nothing in the shipped server constructed a DrainWorker.
		"cargo test -p fraiseql-server --features cdc-outbound --test cdc_outbound_mount_pg -- --ignored --test-threads=1",
		// #715/#717 arrow executed-SQL suite: the generated INSERT (every Arrow
		// type, specials, pre-epoch timestamps, NULLs) EXECUTED vs the bound
		// Postgres, and the batched-queries schema-mismatch guard. This is the
		// only leg that runs fraiseql-arrow with a live DATABASE_URL — the test
		// leg's arrow run has no database and its DB-backed tests self-skip.
		"cargo test -p fraiseql-arrow --all-features --test insert_sql_pg -- --ignored --test-threads=1",
		// #953 Flight Upload gate. Two halves, each against the bound Postgres:
		// the decision half over a real Flight socket (which tables are refused,
		// and that an adapter with no atomic-write seam refuses even an
		// allow-listed one), and the write half at the server's adapter (the rows
		// AND their change-log outbox rows, or neither). The hole these close was
		// reachable in the shipped binary, so both drive the mounted path.
		"echo '### #953: Flight Upload gate + Change Spine outbox (against the bound Postgres)'",
		"cargo test -p fraiseql-arrow --all-features --test flight_upload_gate_pg -- --ignored --test-threads=1",
		// `--features arrow` deliberately, NOT serverTestFeatures: that set includes
		// `wire-backend`, under which this suite is `cfg`'d out entirely (the wire
		// adapter has no atomic-upload seam) and would run zero tests while reading
		// green — the #940-class trap.
		"cargo test -p fraiseql-server --features arrow --test flight_upload_outbox_pg -- --ignored --test-threads=1",
		// #908/#1001: the three DB-backed Flight suites. They self-skipped on a
		// missing DATABASE_URL and the test leg has none, so all 31 tests read as
		// passing while running nothing — and two of the three asserted a
		// hardcoded registry constant even when they did run. Rewritten to drive
		// real do_get/do_exchange RPCs, marked #[ignore] so a no-database leg
		// cannot show them green, and named here so they either execute or the
		// leg fails. Each CREATE DATABASEs its own fixture: --test-threads=1.
		"echo '### #908/#1001: Flight DoGet e2e + error handling + adapter integration (real RPCs)'",
		"cargo test -p fraiseql-arrow --all-features --test flight_e2e_test -- --ignored --test-threads=1",
		"cargo test -p fraiseql-arrow --all-features --test flight_error_handling_test -- --ignored --test-threads=1",
		"cargo test -p fraiseql-arrow --all-features --test flight_integration -- --ignored --test-threads=1",
		"echo 'test-integration OK: observers suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithServiceBinding(redisBindHost, m.redisService()).
		WithServiceBinding(natsBindHost, m.natsService()).
		WithServiceBinding(kafkaBindHost, m.kafkaService()).
		WithServiceBinding(localstackBindHost, m.localstackService()).
		WithServiceBinding(mailhogBindHost, m.mailhogService()).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("TEST_DATABASE_URL", dbURL).
		WithEnvVariable("REDIS_URL", redisURL).
		WithEnvVariable("NATS_URL", natsURL).
		// Scheme-less on purpose: this is librdkafka's `bootstrap.servers` shape,
		// which the sink's own kafka:// / kafka+ssl:// scheme is prefixed onto.
		WithEnvVariable("KAFKA_BOOTSTRAP", kafkaBindHost+":9092").
		// Scheme-ful on purpose, and the opposite of KAFKA_BOOTSTRAP above: this
		// is the AWS SDK's endpoint-URL override, which the Kinesis sink screens
		// through `resolve_kinesis_endpoint_url`. It is http://, so the leg must
		// also declare the plaintext opt-in and a development environment below.
		WithEnvVariable("KINESIS_ENDPOINT", fmt.Sprintf("http://%s:4566", localstackBindHost)).
		WithEnvVariable("MAILHOG_SMTP_HOST", mailhogBindHost).
		WithEnvVariable("MAILHOG_SMTP_PORT", "1025").
		WithEnvVariable("MAILHOG_API", fmt.Sprintf("http://%s:8025", mailhogBindHost)).
		// Every insecure-mode escape hatch below is honoured only outside a
		// production environment, and an *unset* FRAISEQL_ENV now reads as
		// production (secure by default, #816). A leg that opts into a hatch has
		// to declare the environment too, or the hatch is silently refused and
		// the suite fails on a guard it thought it had opted out of.
		WithEnvVariable("FRAISEQL_ENV", "development").
		WithEnvVariable("FRAISEQL_ALLOW_PRIVATE_WEBHOOKS", "true").
		WithEnvVariable("FRAISEQL_OBSERVERS_ALLOW_INSECURE", "true").
		// The bound JetStream service speaks plaintext nats:// (bridge_integration);
		// opt into plaintext for the test broker (L-nats-plaintext).
		WithEnvVariable("FRAISEQL_NATS_ALLOW_PLAINTEXT", "true").
		// Likewise the bound Kafka broker: PLAINTEXT listener, so the sink's
		// transport guard needs the same explicit dev opt-in (#975).
		WithEnvVariable("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", "true").
		// And the bound LocalStack endpoint, which is http://. The Kinesis guard
		// permits an unencrypted override only under this opt-in, a development
		// environment, and a loopback-or-bound host (#975).
		WithEnvVariable("FRAISEQL_KINESIS_ALLOW_PLAINTEXT", "true").
		// Dummy static credentials: without them the AWS provider chain walks out
		// to IMDS and the suite waits on a network timeout instead of failing fast.
		WithEnvVariable("AWS_ACCESS_KEY_ID", "test").
		WithEnvVariable("AWS_SECRET_ACCESS_KEY", "test").
		WithEnvVariable("AWS_EC2_METADATA_DISABLED", "true").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationNats binds a NATS JetStream service and runs the observers NATS
// transport integration tests (#[ignore]d "requires NATS server"); they read
// NATS_URL (the tests override NatsConfig.url with it).
func (m *FraiseqlCi) integrationNats(ctx context.Context, source *dagger.Directory) (string, error) {
	natsURL := fmt.Sprintf("nats://%s:4222", natsBindHost)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: nats (Dagger-bound JetStream; tests read NATS_URL)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-observers --features nats --test nats_integration", // #880 canary
		"cargo test -p fraiseql-observers --features nats --test nats_integration -- --ignored --test-threads=1",
		"echo 'test-integration OK: nats suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(natsBindHost, m.natsService()).
		WithEnvVariable("NATS_URL", natsURL).
		// Every insecure-mode escape hatch below is honoured only outside a
		// production environment, and an *unset* FRAISEQL_ENV now reads as
		// production (secure by default, #816). A leg that opts into a hatch has
		// to declare the environment too, or the hatch is silently refused and
		// the suite fails on a guard it thought it had opted out of.
		WithEnvVariable("FRAISEQL_ENV", "development").
		WithEnvVariable("FRAISEQL_OBSERVERS_ALLOW_INSECURE", "true").
		// The bound JetStream service speaks plaintext nats://; the transport now
		// refuses plaintext by default (L-nats-plaintext), so opt in for the test
		// broker (honoured only outside production).
		WithEnvVariable("FRAISEQL_NATS_ALLOW_PLAINTEXT", "true").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// natsService returns a started nats:2.10-alpine service with JetStream + monitoring
// (`nats-server -js -m 8222`).
func (m *FraiseqlCi) natsService() *dagger.Service {
	return dag.Container().
		From(natsImage).
		WithExposedPort(4222).
		AsService(dagger.ContainerAsServiceOpts{UseEntrypoint: true, Args: []string{"-js", "-m", "8222"}})
}

// kafkaService returns a started single-node Kafka broker in KRaft mode (no
// ZooKeeper). `advertised.listeners` must name the Dagger bind host, not
// localhost: librdkafka connects to the bootstrap server, is handed the
// advertised address, and then connects to *that* — an advertised `localhost`
// would send every produce back into the test container.
func (m *FraiseqlCi) kafkaService() *dagger.Service {
	return dag.Container().
		From(kafkaImage).
		WithEnvVariable("KAFKA_NODE_ID", "1").
		WithEnvVariable("KAFKA_PROCESS_ROLES", "broker,controller").
		WithEnvVariable("KAFKA_LISTENERS", "PLAINTEXT://:9092,CONTROLLER://:9093").
		WithEnvVariable("KAFKA_ADVERTISED_LISTENERS", "PLAINTEXT://"+kafkaBindHost+":9092").
		WithEnvVariable("KAFKA_CONTROLLER_LISTENER_NAMES", "CONTROLLER").
		WithEnvVariable(
			"KAFKA_LISTENER_SECURITY_PROTOCOL_MAP",
			"CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT",
		).
		WithEnvVariable("KAFKA_CONTROLLER_QUORUM_VOTERS", "1@localhost:9093").
		WithEnvVariable("KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR", "1").
		WithEnvVariable("KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR", "1").
		WithEnvVariable("KAFKA_TRANSACTION_STATE_LOG_MIN_ISR", "1").
		WithEnvVariable("KAFKA_GROUP_INITIAL_REBALANCE_DELAY_MS", "0").
		WithExposedPort(9092).
		AsService(dagger.ContainerAsServiceOpts{UseEntrypoint: true})
}

// localstackService returns a started LocalStack with only the Kinesis service
// enabled, backing the #975 outbound CDC sink. `SERVICES=kinesis` keeps boot to
// the one API the suite uses; the image otherwise starts every emulator it ships.
func (m *FraiseqlCi) localstackService() *dagger.Service {
	return dag.Container().
		From(localstackImage).
		WithEnvVariable("SERVICES", "kinesis").
		// LocalStack signs nothing, but the AWS SDK still requires credentials to
		// be resolvable; these match the dummy pair the leg exports to the tests.
		WithEnvVariable("AWS_DEFAULT_REGION", "us-east-1").
		WithExposedPort(4566).
		AsService(dagger.ContainerAsServiceOpts{UseEntrypoint: true})
}

// mailhogService is the MailHog SMTP sink: SMTP on 1025 (plaintext) and an HTTP
// inspection API on 8025. Used by the #349 email happy-path integration test.
func (m *FraiseqlCi) mailhogService() *dagger.Service {
	return dag.Container().
		From(mailhogImage).
		WithExposedPort(1025).
		WithExposedPort(8025).
		AsService(dagger.ContainerAsServiceOpts{UseEntrypoint: true})
}

// redisService returns a started redis:7-alpine service (default redis-server CMD).
func (m *FraiseqlCi) redisService() *dagger.Service {
	return dag.Container().
		From(redisImage).
		WithExposedPort(6379).
		AsService()
}

// vaultService returns a started Vault dev-mode service. Dev mode disables mlock
// (no IPC_LOCK cap needed) and seeds the root token from VAULT_DEV_ROOT_TOKEN_ID.
func (m *FraiseqlCi) vaultService() *dagger.Service {
	return dag.Container().
		From(vaultImage).
		WithEnvVariable("VAULT_DEV_ROOT_TOKEN_ID", vaultToken).
		WithEnvVariable("VAULT_DEV_LISTEN_ADDRESS", "0.0.0.0:8200").
		WithEnvVariable("VAULT_LOG_LEVEL", "warn").
		WithExposedPort(8200).
		AsService(dagger.ContainerAsServiceOpts{UseEntrypoint: true, Args: []string{"server", "-dev"}})
}

// pgService returns a started postgres:16 service seeded with the repo's
// integration fixtures. The two SQL files are mounted into
// /docker-entrypoint-initdb.d under numeric names so the entrypoint runs them in
// load order (init before init-analytics) on first boot. Dagger waits for the
// exposed port before bound callers proceed, so Postgres is accepting connections
// by the time the tests run.
func (m *FraiseqlCi) pgService(source *dagger.Directory) *dagger.Service {
	initDir := dag.Directory().
		WithFile("00-init.sql", source.File("tests/sql/postgres/init.sql")).
		WithFile("01-init-analytics.sql", source.File("tests/sql/postgres/init-analytics.sql")).
		// #957: creates the replication role and opens `pg_hba.conf` to replication
		// connections, so pgStandbyService can base-backup and then stream. The SAME
		// file docker-compose.test.yml mounts — a replication setup that existed on
		// only one rig would make the lag suite green on one and red on the other.
		WithFile("02-replication.sh", source.File("docker/init/postgres-replication-init.sh"))

	return dag.Container().
		From(pgImage).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithDirectory("/docker-entrypoint-initdb.d", initDir).
		WithExposedPort(5432).
		AsService()
}

// pgStandbyService returns a REAL PostgreSQL streaming standby of `primary`,
// cloned with pg_basebackup through a replication slot (#957).
//
// `primary` must be the same *dagger.Service instance the test container binds,
// not a second pgService(source) call: two calls produce two independent
// databases, and a standby of the one nobody queries reports lag against writes
// nobody made.
// Each standby needs its own replication slot: two standbys sharing one slot
// fight over the same WAL retention point on the primary.
func (m *FraiseqlCi) pgStandbyService(
	source *dagger.Directory,
	primary *dagger.Service,
	slot string,
) *dagger.Service {
	return dag.Container().
		From(pgImage).
		WithServiceBinding(pgBindHost, primary).
		WithFile("/standby-entrypoint.sh", source.File("docker/init/postgres-standby-entrypoint.sh")).
		WithEnvVariable("PGPASSWORD", pgReplicationPassword).
		WithEnvVariable("PRIMARY_HOST", pgBindHost).
		WithEnvVariable("PRIMARY_USER", pgUser).
		WithEnvVariable("PRIMARY_PASSWORD", pgPassword).
		WithEnvVariable("PRIMARY_DB", pgDatabase).
		WithEnvVariable("REPLICATION_USER", pgReplicationUser).
		WithEnvVariable("REPLICATION_SLOT", slot).
		WithUser("postgres").
		WithExposedPort(5432).
		AsService(dagger.ContainerAsServiceOpts{
			Args: []string{"/bin/bash", "/standby-entrypoint.sh"},
		})
}

// integrationBase mounts the source on rustBaseFor(toolchain), ready to bind
// services into. It uses a dedicated integration target-cache volume (kept apart
// from the Phase-02 gate and Phase-03 unit-test caches, which hold different
// feature/artifact sets) and sets RUST_LOG=debug like the legacy integration jobs.
func (m *FraiseqlCi) integrationBase(source *dagger.Directory, rust string) *dagger.Container {
	toolchain := resolveToolchain(rust)
	// `integ3-` bump (2026-06-30): bust the stale integ2 target cache that reused
	// pre-#501 fraiseql-db artifacts, hiding `execute_function_call_dry_run` → false
	// E0599 in the integration postgres leg. Mirrors the `test2-` bump above.
	//
	// `integ4-` bump (2026-07-27): the same class recurred, and far more dangerously.
	// The #794/#795 analytics-injection fix was committed, the leg checked out the right
	// SHA, and cargo then reported EVERY crate fresh — zero `Compiling` lines in the whole
	// run — so the new test binary linked a pre-fix `fraiseql-core` and the leg reported
	// the injections still succeeding. Last time this surfaced as a compile error; this
	// time it silently validated stale code, which is the failure mode that matters:
	// a green integration leg does not prove the committed source was the source tested.
	// Reproduced across two dispatches of the same commit before the bump.
	//
	// A volume bump only clears the current drift. The durable fix (tracked separately)
	// is to stop trusting mtime-based freshness across a Dagger mount + persistent
	// target volume.
	targetVol := "fraiseql-rust-target-integ4-" + strings.ReplaceAll(toolchain, ".", "-")
	return m.rustBaseFor(toolchain).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume(targetVol)).
		WithEnvVariable("RUST_LOG", "debug")
}
