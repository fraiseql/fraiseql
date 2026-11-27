// Package main is the FraiseQL CI Dagger module.
//
// It hosts the self-hosted CI pipeline that replaces the GitHub-hosted workflows
// (Track 0). Phase 01 ports the smallest gate:
// the axum `:param` route-syntax check (issue #316).
package main

import (
	"context"
	"fmt"
	"strconv"
	"strings"
	"time"

	"dagger/fraiseql-ci/internal/dagger"
)

// FraiseqlCi is the FraiseQL CI module root.
type FraiseqlCi struct{}

const (
	// rustImage pins the toolchain to the workspace MSRV (rust-toolchain.toml channel = 1.100.0).
	// The PATCH is pinned deliberately: docker.io/library/rust:1.100 tracks the newest 1.100.x, so a
	// floating tag silently stops testing the rust-version we declare the day 1.100.1 ships.
	// The default (non-slim) variant is buildpack-deps-based, so gcc/perl/curl/git are present.
	// (Later: pin by digest — see docs/contributing/dagger-parity-notes.md Phase 02.)
	//
	// Pulled from ghcr.io/fraiseql/* (mirrored by .github/workflows/mirror-base-images.yml),
	// NOT Docker Hub: the self-hosted runner shares one Docker Hub account whose pull-rate
	// limit periodically 429s every leg. The ghcr mirrors are public, so the engine pulls
	// them anonymously. Every ghcr.io/fraiseql/* tag below MUST have a matching entry in that
	// workflow's IMAGES list. (mcr.microsoft.com/* and the Apollo router stay as-is — not
	// Docker Hub, not rate-limited.)
	rustImage = "ghcr.io/fraiseql/rust:1.100.0"
	// ubuntuImage backs shellBase (the toolchain-free shell-gate container).
	ubuntuImage = "ghcr.io/fraiseql/ubuntu:24.04"
	// smokePythonImage backs smokePythonBase: the in-repo Python SDK's floor is 3.15, which
	// neither rustImage (Debian 13, Python 3.13) nor ubuntuImage (3.12) ships. It is
	// python:3.15-slim-trixie, the same Debian 13 as rustImage, so a binary built there
	// runs here.
	smokePythonImage = "ghcr.io/fraiseql/python:3.15.2-slim-trixie"
	// sccacheVersion pins the prebuilt sccache binary fetched into rustBase.
	sccacheVersion = "v0.8.2"
	// iaiCallgrindVersion pins iai-callgrind-runner to the iai-callgrind library version
	// in crates/fraiseql-core/Cargo.toml; the runner refuses a library of another version.
	iaiCallgrindVersion = "0.16.1"
	// cargoLlvmCovVersion pins the cargo-llvm-cov CoveragePostgres installs.
	cargoLlvmCovVersion = "0.9.1"
	// rustMsrv mirrors Cargo.toml workspace rust-version and rust-toolchain.toml channel.
	rustMsrv = "1.100.0"
	// fmtNightly pins the nightly rustfmt this workspace is formatted with, because
	// rustfmt's output is not stable across nightlies: a bare `nightly` made the Fmt
	// gate enforce whatever published that morning, and on 2026-09-25 that reddened
	// dev — and every open PR — over a macro body no commit had touched. The value
	// is duplicated nowhere: tools/fmt-toolchain.txt is the source and
	// tools/check-fmt-toolchain.sh fails if this const drifts from it.
	// (Same lesson as FUZZ_NIGHTLY in .github/workflows/fuzz.yml.)
	fmtNightly = "nightly-2026-09-25"

	// SYNC:* feature sets — this file is the single authority since the legacy
	// ci.yml was retired (#951); the SYNC tags mark every use site in this file.
	coreTestFeatures = "audit-syslog,audit-webhook,federation,kafka,postgres,redis-apq,schema-lint,test-utils,wire-backend"
	dbTestFeatures   = "postgres,wire-backend"
	// redis-pkce + redis-rate-limiting are compiled in so the #770/#777 boot-guard
	// lib tests run here (they need no live Redis — closed ports and URL parsing);
	// the Redis-requiring tests stay #[ignore]d and run in the redis integration leg.
	serverTestFeatures = "arrow,auth,aws-s3,federation,grpc,mcp,metrics,observers,redis-apq,redis-pkce,redis-rate-limiting,rest,secrets,storage-transforms,testing,tracing-opentelemetry,webhooks"
	// serverIntegrationExtraFeatures: what the server integration leg's suites need on
	// top of serverTestFeatures, so the whole leg builds fraiseql-server once (#1885).
	// admin-sql: the SQL console suite. export-csv/export-xlsx: the REST export
	// suites. inbound-email (implies inbound and functions-runtime): local auth's
	// SMTP transport, the inbound webhook suites and the function-dispatch suites.
	// functions-runtime-deno: the tenant function-dispatch suite's TypeScript
	// function. It only registers the Deno runtime, which starts no V8 isolate until
	// a Deno function runs, so a suite that declares none behaves as without it.
	serverIntegrationExtraFeatures = "admin-sql,export-csv,export-xlsx,functions-runtime-deno,inbound-email"
	// capabilityFixtureExtraFeatures: what the capability fixtures need on top of
	// serverTestFeatures, every Stable capability. The observers leg builds the
	// `fraiseql-cli` binary their `run` entry point serves through with the same set.
	capabilityFixtureExtraFeatures = "export-csv,export-xlsx,sources,inbound,inbound-email,auth-saml,cdc-outbound,subscription-kafka,cli"
	// serverUnifiedFeatures: the one fraiseql-server feature set of the postgres and observers
	// integration legs (#1886, #1902). Every list a suite of those legs needs is a subset,
	// and each distinct list rebuilt fraiseql-server and, under it, fraiseql-core and
	// fraiseql-db. It is serverTestFeatures and capabilityFixtureExtraFeatures, plus
	// admin-sql, functions-runtime-deno and observers-nats; tools/check-leg-feature-sets.py
	// holds it to both lists. A line that needs a feature OFF says so with a
	// `// feature-off:` annotation that the same gate checks.
	serverUnifiedFeatures = "arrow,auth,aws-s3,federation,grpc,mcp,metrics,observers,redis-apq,redis-pkce,redis-rate-limiting,rest,secrets,storage-transforms,testing,tracing-opentelemetry,webhooks,export-csv,export-xlsx,sources,inbound,inbound-email,auth-saml,cdc-outbound,subscription-kafka,cli,admin-sql,functions-runtime-deno,observers-nats"
	// The two feature-OFF builds of serverUnifiedFeatures, for suites whose OFF arms need
	// the database: each drops a flag and every feature that turns it on.
	// serverWithoutArrowOrDenoFeatures: no `arrow`, no `functions-runtime-deno`
	// (`functions-runtime` stays on).
	serverWithoutArrowOrDenoFeatures = "auth,aws-s3,federation,grpc,mcp,metrics,observers,redis-apq,redis-pkce,redis-rate-limiting,rest,secrets,storage-transforms,testing,tracing-opentelemetry,webhooks,export-csv,export-xlsx,inbound,inbound-email,auth-saml,cdc-outbound,subscription-kafka,cli,admin-sql,observers-nats"
	// serverWithoutAuthFeatures: the integration leg's server set without `auth` and
	// every feature that turns it on; built with --no-default-features (`auth` is a
	// default).
	serverWithoutAuthFeatures = "arrow,aws-s3,cdc-outbound,cli,export-csv,export-xlsx,federation,functions-runtime-deno,grpc,inbound,inbound-email,metrics,observers,observers-nats,redis-apq,redis-rate-limiting,rest,secrets,sources,storage-transforms,subscription-kafka,testing,tracing-opentelemetry,webhooks"
	// serverWithoutFunctionsFeatures: no `functions-runtime`.
	serverWithoutFunctionsFeatures = "arrow,auth,aws-s3,federation,grpc,mcp,metrics,observers,redis-apq,redis-pkce,redis-rate-limiting,rest,secrets,storage-transforms,testing,tracing-opentelemetry,webhooks,export-csv,export-xlsx,auth-saml,cdc-outbound,subscription-kafka,cli,admin-sql,observers-nats"
	// cliPostgresFeatures: the one fraiseql-cli feature set of the postgres integration leg.
	cliPostgresFeatures = "test-postgres,federation,run-server,fraiseql-server/observers"
	// testWorkspaceSkip: the skip patterns the `Test` shard's workspace sweep
	// carries — the testcontainers lib tests (storage + functions); wire's
	// container tests live in tests/*, so only its lib unit tests run.
	// `uploads::tests` (#369) joins the list for the same reason: it needs a real
	// Postgres for the resumable-upload session table, and without DATABASE_URL
	// the harness spawns a testcontainer, which this engine has no Docker for.
	// It runs for real in the `storage` integration suite.
	//
	// A package const rather than a local in `Test`: `make test-leg` runs this
	// line through tools/leg-lines.py, which resolves consts but refuses an
	// expression it cannot resolve rather than run a different command.
	testWorkspaceSkip = "-- --skip metadata::tests --skip migrations::tests --skip routes::tests --skip uploads::tests"
	// serverInProcessTests: every fraiseql-server tests/*.rs binary that runs
	// in-process (no backing service) and is not already named by a dedicated
	// line. Enumerated because fraiseql-server is excluded from the workspace
	// run and DB-backed binaries must NOT run here (they belong to the
	// integration legs, loud-panic on a missing DATABASE_URL by contract).
	// The suite-coverage gate (tools/check-suite-coverage.py, preflight)
	// fails when a new binary lands in no leg, so this list cannot rot (#992).
	serverInProcessTests = " --test admin_api_security_test --test admin_authz_test --test admin_cache_vocabulary_e2e --test admission_control_test --test api_admin_tests --test api_design_audit_tests --test api_design_security_tests --test api_federation_tests --test api_infrastructure_tests --test api_query_tests --test api_schema_tests --test apq_mutation_e2e_test --test auth_me_integration_test --test auth_regression_test --test backpressure_overload_test --test backpressure_test --test cache_wiring_tests --test changelog_cascade_compose_boot --test config_struct_test --test constructor_drift_test --test endpoint_health_tests --test error_handling_validation_test --test example_validation_test --test functions_platform_pins_test --test graphql_http_layer_test --test graphql_request_validation_test --test grpc_transport_e2e_test --test introspection_gate_e2e_test --test introspection_mutation_authz_test --test introspection_security_test --test metrics_facade_scrape --test metrics_integration_test --test multitenancy_test --test observability_test --test platform_e2e_test --test production_safety_test --test profile_error_redaction_test --test profile_query_limits_test --test rate_limiting_integration_test --test rate_limit_sweep_is_scheduled_test --test rest_transport_e2e_test --test security --test property --test security_stack_integration_test --test service_account_conformance_test --test studio_admin_api_test --test studio_e2e_test --test studio_metrics_test --test studio_shell_test --test tracing_integration_test --test typename_e2e_test --test v230_integration_tests"
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
//
// ⚠⚠ THE CAPTURE MUST NOT LET `set -e` FIRE BEFORE THE OUTPUT IS PRINTED (#1276).
//
// Buffering into `$out` is what lets the SUITE-RAN line quote the suite's own
// `test result:` counts — but written as `out="$(…)"; rc=$?`, the assignment IS a
// simple command whose status is the substitution's, so under the script's `set -e`
// a FAILING cargo kills the shell right there. `printf '%s\n' "$out"` never runs and
// **the entire output of the failing suite is discarded**: no test names, no
// `test result: FAILED`, no panic message, no backtrace. Just the container's
// exit 101.
//
// That is not hypothetical, and it is not cheap. This prelude is injected into
// exactly ONE leg — `integrationServer` — and BOTH recorded "exit 101 with no failing
// test anywhere in the log" incidents happened in that leg: #1276's
// `graphql_sse_e2e_pg`, where 95,700 log lines held zero `test result: FAILED`, and
// the `rest_bulk_safety_e2e_pg` abort two runs later. #1276 concluded from the silence
// that the cause must be "an abort or hang, not a test failure". It could not have
// concluded otherwise, and a plain assertion failure produces exactly the same
// signature here. Measured on a real failing suite: 0 lines of output before, 36
// after, same exit 101.
//
// `|| rc=$?` is what protects the assignment; `rc=0` is its initialiser. `set -e`
// still aborts the script at the CALL SITE, so the leg stops at the same suite it
// always did — the only thing that changes is that the log says why.
//
// SUITE-START pairs with SUITE-RAN so the boundary never has to be inferred from an
// ABSENT line again. It is what a HANG leaves behind: a suite killed by the leg
// timeout produces no cargo exit at all, so `$out` is never printed and SUITE-RAN
// never fires — the last SUITE-START names it. Constructed at runtime for the same
// reason SUITE-RAN is.
const suiteCountPrelude = `cargo() {
  local out rc=0
  printf 'SUITE-START %s\n' "$*"
  out="$(command cargo "$@" 2>&1)" || rc=$?
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
		WithExec(aptInstall(

			"git", "gawk", "findutils", "grep", "ca-certificates",
		))
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
		{"sccache-bootstrap", m.SccacheBootstrap},
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

// SccacheBootstrap: tools/ci-stamp-sources.sh starts sccache's server before cargo does,
// compiles uncached and says so when none answers, and prints the server's error log when
// an exec fails (#1914). It runs in rustBase because that is where the legs' sccache is: the
// self-test fails, rather than skips, without it.
func (m *FraiseqlCi) SccacheBootstrap(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithEnvVariable("FRAISEQL_LEG_NONCE", strconv.FormatInt(time.Now().UnixNano(), 10)).
		WithExec([]string{"bash", "tools/tests/sccache_bootstrap_test.sh"}).
		Stdout(ctx)
}

// Fmt: `make fmt-check`, rustfmt in check mode at tools/fmt-toolchain.txt's nightly,
// which check-fmt-toolchain.sh holds equal to fmtNightly. rustfmt's
// advanced options need nightly (rust-toolchain.toml pins stable to the MSRV), so
// rustBase carries that pinned nightly with only the rustfmt component.
func (m *FraiseqlCi) Fmt(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "-c", "make -k --no-print-directory fmt-check"}).
		Stdout(ctx)
}

// Clippy: `make clippy`, warnings denied, every feature and target.
// --all-features is intentional (lints every feature path; the test-* gate features
// only need infra at runtime).
func (m *FraiseqlCi) Clippy(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustSrc(source).
		// The target also clippies the async-jobs example subgraph, its own crate
		// outside the workspace.
		WithExec([]string{"bash", "-c", "make -k --no-print-directory clippy"}).
		Stdout(ctx)
}

// CheckDefault: `make check-default`, `cargo check` with the DEFAULT feature set.
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
		WithExec([]string{"bash", "-c", "make -k --no-print-directory check-default"}).
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
		// The gate's self-test compiles fixture crates, so it runs here, with cargo.
		WithExec([]string{"bash", "-c", "make -k --no-print-directory check-fuzz test-fuzz-compiles-gate"}).
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
		// The gate's self-test compiles fixture crates, so it runs here, with cargo.
		WithExec([]string{"bash", "-c", "make -k --no-print-directory check-example-crates test-example-crates-gate"}).
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
		WithExec(aptInstall(
			"r-base-core",
		)).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		// The gate's self-test parses fixture R files, so it runs here, with Rscript.
		WithExec([]string{"bash", "-c", "make -k --no-print-directory lint-r-examples test-r-examples-gate"}).
		Stdout(ctx)
}

// Rustdoc: `make rustdoc`, every feature, warnings denied.
func (m *FraiseqlCi) Rustdoc(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustSrc(source).
		WithExec([]string{"bash", "-c", "make -k --no-print-directory rustdoc"}).
		Stdout(ctx)
}

// RustdocDefault: `make rustdoc-default`, the DEFAULT feature set, warnings denied.
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
//
// Then `tools/check-rustdoc-feature-sets.sh` documents fraiseql-server once per feature
// that gates one of its modules. Neither workspace pass sees a link from one optional
// subsystem to another: `--all-features` resolves it and the default build compiles
// neither end (#2089: `--features functions-runtime` alone failed on three links).
// It also documents fraiseql-functions once per runtime feature alone, with no default
// features: a Deno doc linking into the WASM runtime resolves only when both are built.
//
// The doctests per feature set are not here: they build every dependency graph again
// with codegen, which took 26 minutes cold, and run in `Doctests`, a leg of their own.
func (m *FraiseqlCi) RustdocDefault(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return m.rustSrc(source).
		WithExec([]string{"bash", "-c", "make -k --no-print-directory rustdoc-default"}).
		Stdout(ctx)
}

// Doctests runs `tools/check-doctest-feature-sets.sh`: each library's doctests on its
// own with its default features, and the umbrella's once per feature its docs are
// gated on. The Test leg's `cargo test --doc --all-features` compiles an example that
// needs a feature, and a `--workspace` doctest run unifies features across members,
// so neither sees an example that fails for a crate built with its defaults (#2015,
// #1898).
//
// A leg of its own, not a Preflight sibling. Each set compiles its crate's dependency
// graph with codegen under that crate's features, a build no other leg shares: inside
// Preflight it took 26 minutes on a cold runner and pushed the leg past its timeout.
// Its own target volume keeps those artifacts from evicting Preflight's check ones,
// and keeps the two legs from waiting on each other for one PRIVATE volume.
func (m *FraiseqlCi) Doctests(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	return rustSource(m.rustBase(), source, "fraiseql-rust-target-doctests").
		WithExec([]string{"bash", "-c", "bash tools/check-doctest-feature-sets.sh"}).
		Stdout(ctx)
}

// ShellGates runs every gate that needs no Rust toolchain, in one minimal container:
// the Makefile's `shell-gates` target, which `make preflight` also runs, so the list
// is written once. `-k` reports every failing gate in one run; make still exits
// non-zero if any failed. The source is mounted without `.git`; `git init` gives the
// gates that ask git for the toplevel an empty repository.
func (m *FraiseqlCi) ShellGates(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	script := "git init -q . >/dev/null && make -k --no-print-directory shell-gates"

	return m.shellBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// PerfInstructions is the performance gate (#1636). It first runs the dhat allocation tests
// (crates/fraiseql-core/benches/memory_profile.rs), then runs `Executor::execute` and a REST
// GET under callgrind (crates/fraiseql-core/benches/executor_instructions.rs,
// crates/fraiseql-server/benches/rest_instructions.rs) and compares each
// bench's instruction count with benchmarks/instructions-baseline.json, failing above 3%
// either way (tools/check-perf-instructions.py). Instruction counts are deterministic for
// a binary, so the gate needs no repetitions and no quiet machine, but they depend on the
// binary: the baseline is only valid as measured in this container, and
// `dagger call perf-instructions --source=. --print` is how it is refreshed.
//
// IAI_CALLGRIND_ALLOW_ASLR: the runner otherwise disables ASLR with setarch, which needs
// the personality syscall the engine's seccomp profile refuses.
func (m *FraiseqlCi) PerfInstructions(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// Print the measured counts as a baseline file instead of comparing.
	// +optional
	print bool,
) (string, error) {
	check := "python3 tools/check-perf-instructions.py"
	if print {
		check += " --print"
	}
	script := strings.Join([]string{
		"set -euo pipefail",
		"rm -rf /src/target/iai",
		// The allocation half: dhat assertions on what a warm read allocates (one
		// global allocator per binary, so they run single-threaded).
		"cargo test -p fraiseql-core --features dhat-heap --bench memory_profile -- --test-threads=1 >&2",
		"export IAI_CALLGRIND_ALLOW_ASLR=yes",
		// Nine runs, each in its own home: the comparator takes each bench's mean,
		// because std's per-process hash seed moves a single count by up to 2.6%.
		"for run in $(seq 1 9); do",
		"  export IAI_CALLGRIND_HOME=/src/target/iai/run$run",
		// Bench output goes to stderr, so that with --print stdout is the baseline file alone.
		"  cargo bench -p fraiseql-core --bench executor_instructions -- --save-summary=json >&2",
		"  cargo bench -p fraiseql-server --features rest --bench rest_instructions -- --save-summary=json >&2",
		"done",
		check,
	}, "\n")
	base := m.rustBase().
		WithExec(aptInstall("valgrind")).
		WithExec([]string{"bash", "-c", "cargo install iai-callgrind-runner --version " + iaiCallgrindVersion + " --locked"})
	return rustSource(base, source, "fraiseql-rust-target-perf").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// rustSrc mounts the source on rustBase with a persistent target cache volume. Used
// by the compiling gates (clippy, rustdoc); fmt skips the target cache (it never
// compiles).
func (m *FraiseqlCi) rustSrc(source *dagger.Directory) *dagger.Container {
	return rustSource(m.rustBase(), source, "fraiseql-rust-target")
}

// rustSource is the one way a Rust leg mounts the commit and a persistent target volume
// (#1421). Every leg that compiles goes through it, because a target volume outlives the
// run and cargo judges a workspace crate fresh by comparing mtimes, not content:
//
//   - Sequential staleness (#880): an artifact an earlier run built from different source
//     can be newer than this checkout's files, and cargo links it. So every file of the
//     source is stamped with one instant; no artifact written before it can be judged
//     fresh. The instant is exported as FRAISEQL_SOURCE_TOUCHED_AT, which is what lets
//     tools/ci-target-canary.sh prove it (any cached workspace artifact older than the
//     stamp fails the leg).
//   - Concurrent staleness: two runners on this box ran two branches' legs at once on
//     one volume, and one branch's tests linked the other's crates. So the volume is
//     mounted PRIVATE: Dagger hands a busy volume to one user at a time. Measured on this
//     engine (v0.21): concurrent PRIVATE users are serialized, never given a shared mount.
//   - The two together (#1596): the instant is taken, and the sources touched, by each of
//     the leg's execs as it starts, once it holds the volume: BASH_ENV sources
//     tools/ci-stamp-sources.sh. Taken when the pipeline was built, in an exec before the
//     mount, a leg that waited for the volume got artifacts another branch built after
//     its stamp, newer than its sources (fresh to cargo) and than the stamp (invisible to
//     the canary). So every exec on this container is a bash (tools/check-ci-source-stamp.sh
//     refuses a direct `cargo` one), and FRAISEQL_LEG_NONCE, new on every call, keeps
//     Dagger from replaying a cached exec.
//
// The cost is a rustc invocation per workspace unit on every leg; sccache (content-
// addressed, RUSTC_WRAPPER in rustBaseFor) answers those from its cache.
func rustSource(base *dagger.Container, source *dagger.Directory, targetVol string) *dagger.Container {
	return base.
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume(targetVol), dagger.ContainerWithMountedCacheOpts{
			Sharing: dagger.CacheSharingModePrivate,
		}).
		WithEnvVariable("FRAISEQL_LEG_NONCE", strconv.FormatInt(time.Now().UnixNano(), 10)).
		WithEnvVariable("FRAISEQL_STAMP_ROOT", "/src").
		WithEnvVariable("BASH_ENV", "/src/tools/ci-stamp-sources.sh")
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
		WithExec(aptInstall(

			"mold", "clang", "pkg-config", "libssl-dev", "cmake",
			"protobuf-compiler", "python3", "libsasl2-dev", "zlib1g-dev",
			// libxml2-dev + libxmlsec1-dev: samael's `xmlsec` backend (the #381
			// `auth-saml` feature). The shared clippy/rustdoc/test legs compile
			// `--all-features`, which turns auth-saml on, so the C stack must live in
			// the base. Mirrors the local requirement (`pacman -S xmlsec` on Arch).
			"libxml2-dev", "libxmlsec1-dev",
		)).
		// rustfmt + clippy on the pinned stable, plus rust-analyzer to satisfy
		// rust-toolchain.toml (avoids a mid-run auto-install); a minimal nightly
		// carrying only rustfmt, pinned by fmtNightly.
		WithExec([]string{"rustup", "component", "add", "clippy", "rustfmt", "rust-analyzer"}).
		WithExec([]string{"rustup", "toolchain", "install", fmtNightly, "--profile", "minimal", "--component", "rustfmt"}).
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
		// Where the sccache server writes its errors; tools/ci-stamp-sources.sh prints it when
		// an exec fails (#1914). /tmp is per container, so concurrent legs never share it.
		WithEnvVariable("SCCACHE_ERROR_LOG", "/tmp/sccache-error.log").
		WithMountedCache("/sccache", dag.CacheVolume("fraiseql-sccache")).
		WithMountedCache(cargoHome+"/registry", dag.CacheVolume("fraiseql-cargo-registry")).
		WithMountedCache(cargoHome+"/git", dag.CacheVolume("fraiseql-cargo-git"))
}

// shellBase is the minimal container for the non-Rust lint gates: bash + make + the
// grep/awk/find toolchain the `make lint-*` recipes and check-*.sh scripts use
// (gawk, not mawk, for the load-bearing multi-line route-syntax pass — see lintBase).
// aptInstall is ONE exec that refreshes the package index and installs pkgs.
//
// An `apt-get update` exec of its own is cached as a layer, and an install exec layered on
// it reads that index whenever the install misses the cache. Once Ubuntu's security pocket
// replaced libpng (1.6.43-5ubuntu0.6), the cached index still named the old .deb, and the R
// examples' install 404'd on every run of preflight until the cache was evicted
// (2026-10-10). In one exec the index is as fresh as the install. tools/check-dagger-apt.sh
// refuses any other form.
func aptInstall(pkgs ...string) []string {
	return []string{
		"sh", "-c",
		"apt-get update && apt-get install -y --no-install-recommends " + strings.Join(pkgs, " "),
	}
}

func (m *FraiseqlCi) shellBase() *dagger.Container {
	return dag.Container().
		From(ubuntuImage).
		WithExec(aptInstall(

			// python3: the suite-coverage gate (tools/check-suite-coverage.py).
			"make", "git", "gawk", "findutils", "grep", "ca-certificates", "python3",
		))
}

// ── Phase 03: Workspace Test Suite ────────────────────────────────────────────
//
// The workspace test leg: a full `cargo build --all-features`
// followed by the feature-scoped `cargo test -p …` invocations (the SYNC:* lists)
// and the doctest pass. Parameterized by toolchain (stable | MSRV 1.100.0).

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
	// Per-toolchain target cache: stable and 1.100.0 produce incompatible artifacts,
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
			// fraiseql-test-support is excluded so that `--all-features`, which applies to
			// every selected package, does not turn on its `local-testcontainers`: with it
			// on, a harness that finds no DATABASE_URL starts a container, and this leg has
			// no Docker, so the database suites panic here instead of skipping. They run in
			// the integration legs, which bind a database. Its own tests run below.
			" --exclude fraiseql-functions --exclude fraiseql-test-support" +
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
		"cargo test -p fraiseql-functions --features 'runtime-deno,runtime-wasm,host-live' -- --skip migrations::tests",
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
		"echo '### cargo test -p fraiseql-server --lib (SYNC:SERVER_FEATURES; storage_policy_admin_tests + runtime_config_drift → integration leg)'",
		"cargo test -p fraiseql-server --lib --features '" + serverTestFeatures + "' -- --skip server::routing::storage_policy_admin_tests --skip tenancy::tests::runtime_config_drift",
		// ...and on the DEFAULT feature set, for the same reason as the core line above:
		// the `not(federation)` health-status arms and
		// `from_file_names_the_build_feature_for_a_compiled_out_section` — which asserts
		// that a `[observers]` section in a build without the feature is REFUSED by name
		// rather than ignored — compile only where those features are off (#1179).
		"echo '### cargo test -p fraiseql-server --lib (default features: the feature-OFF refusal arms)'",
		"cargo test -p fraiseql-server --lib -- --skip server::routing::storage_policy_admin_tests --skip tenancy::tests::runtime_config_drift",
		// The build without `auth` (#2521, #2522): its refusals and its fixtures compile only
		// there, which neither line above builds. The database-backed tests it skips run in
		// integrationPostgres, in a build without `auth` too.
		"echo '### cargo test -p fraiseql-server --lib (no default features, rest: the auth-OFF build)'",
		"cargo test -p fraiseql-server --lib --no-default-features --features rest -- --skip server::routing::storage_policy_admin_tests --skip tenancy::tests::runtime_config_drift",
		// fraiseql-auth runs above only inside the `--all-features` workspace sweep, so its
		// `#[cfg(not(feature = "auth-saml"))]` arms compiled nowhere: the build without SAML,
		// the default one, was never tested.
		"echo '### cargo test -p fraiseql-auth --lib (default features: the not(auth-saml) arms)'",
		"cargo test -p fraiseql-auth --lib",
		// fraiseql-test-support's pin that postgres() fails with DATABASE_URL unset holds
		// only in the build without `local-testcontainers`, where an unset URL cannot
		// spawn a database. The workspace sweep above leaves the crate out, so these two
		// lines are where its tests run: default features, then every feature but the
		// local spawn, which this leg cannot run.
		"echo '### cargo test -p fraiseql-test-support --lib (default features: no local spawn)'",
		"cargo test -p fraiseql-test-support --lib",
		"echo '### cargo test -p fraiseql-test-support (every feature but local-testcontainers)'",
		"cargo test -p fraiseql-test-support --features boot-locks,held-replies,test-issuer",
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
		// `subscription_connection_caps_ws_test` pins the bounds on open
		// connections, and that a closed socket gives its slot back.
		"echo '### cargo test -p fraiseql-server subscription test binaries (SYNC:SERVER_FEATURES; not covered by --lib) — P18 verification gate'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --test subscription_ws_e2e_test --test subscription_protocol_test --test subscription_integration_test --test graphql_ws_row_visibility_pin_test --test subscription_lifecycle_ws_test --test subscription_connection_caps_ws_test",
		"echo '### cargo test -p fraiseql-server test-issuer auth binary (SYNC:SERVER_FEATURES; not covered by --lib)'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --test test_issuer_auth_test",
		// P19 (#747): the GraphQL Idempotency-Key receiver contract — a saga
		// peer's at-least-once retries deduplicate to one logical effect.
		// Docker-free (mock adapter), but a tests/* binary no other invocation
		// reaches (fraiseql-server is excluded from the workspace sweep).
		"echo '### cargo test -p fraiseql-server graphql idempotency binary (SYNC:SERVER_FEATURES; not covered by --lib) — P19 #747 receiver gate'",
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + "' --test graphql_idempotency_e2e_test",
		// #1159: over the SDK conformance fixture, compiled by the real compiler, every
		// sorting query's orderBy enum is exactly the keys both readers accept. No database.
		"cargo test -p fraiseql-server --test order_by_conformance_parity",
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
		// The function-runtime subsystem wiring: `functions = []` does not imply
		// `functions-runtime`, so the line above compiles none of it. deno implies the
		// base runtime, so one invocation covers both gates.
		// `routes::before_mutation::` (#1627): the plain `--lib` lines run its budget
		// tests, but the per-hook limits are functions-runtime-gated and ran nowhere.
		"cargo test -p fraiseql-server --features functions-runtime --lib -- routes::query_function:: routes::before_mutation:: server::provision_tests::each_configured_capability_plans_its_own_tables",
		"cargo test -p fraiseql-server --features functions-runtime-deno --lib subsystems::",
		// #1325: the authoring round trip — the real compiler writes a `functions`
		// section and the real loader reads it back, with the integrity hash
		// covering it. `functions-runtime` because #1326's `refuse_unservable_sections`
		// makes a lean build refuse the artifact, so the ON build is the only one
		// that can prove the section ARRIVES. Its own binary and file-level
		// `#![cfg(feature = "functions-runtime")]`, which cargo cannot see — naming
		// it on any line without the feature builds an empty binary that reports
		// "0 passed" and reads as covered (#1082).
		"cargo test -p fraiseql-server --features functions-runtime --test functions_authoring_round_trip_test --test function_backed_query_seam_test",
		"cargo test -p fraiseql-server --features cdc-outbound --lib -- cdc_outbound:: server::provision_tests::each_configured_capability_plans_its_own_tables",
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
		"echo '### cargo test -p fraiseql-server config coverage manifest + doc config examples + project security keys honoured (not covered by --lib)'",
		// Every `cfg`-gated `ServerConfig` field must be COMPILED IN here, or the
		// manifest gate's two directions disagree: a leaf that does not exist has
		// no consumer to name, and its manifest entry reads as stale. Grow this
		// list whenever a new feature adds a config section (#381 added `saml`).
		"cargo test -p fraiseql-server --features '" + serverTestFeatures + ",export-csv,export-xlsx,sources,inbound,inbound-email,auth-saml,cdc-outbound,subscription-kafka' --test config_coverage_manifest_test --test doc_config_examples_test --test project_security_keys_honoured_test",
		// fraiseql-observers --lib: the Docker-free unit tests (config, executor,
		// DLQ, email). DB/redis/nats tests are #[ignore]d (or skip-on-None)
		// and run in the integration legs. Previously observers was excluded from the workspace
		// run and only its #[ignore]d/name-filtered tests ran (in integration),
		// so these unit tests never executed in CI.
		// checkpoint/metrics are pure lib features whose unit modules were
		// compiled out of every leg before (#992).
		"echo '### cargo test -p fraiseql-observers --lib --features caching,checkpoint,metrics,nats (Docker-free unit tests; DB/redis/nats tests are #[ignore]d → integration legs)'",
		"cargo test -p fraiseql-observers --lib --features 'caching,checkpoint,metrics,nats'",
		// #992: observers in-process test binaries — the crate is excluded from
		// the workspace run, so these executed nowhere.
		"echo '### cargo test -p fraiseql-observers in-process test binaries (#992)'",
		"cargo test -p fraiseql-observers --features 'metrics,testing' --test stress_tests --test transport_pipeline_test",
		"echo '### cargo test --doc --all-features'",
		// Cap doctest concurrency: `cargo test --doc` spawns one process per doctest,
		// and the default thread count (= CPU count) OOMs the 31 GiB box on the heavy
		// --all-features doctest set (arrow doctests were OOM-killed). See the leg-level
		// CARGO_BUILD_JOBS note below.
		"cargo test --doc --all-features -- --test-threads=6",
		"echo \"test OK: workspace suite passed (toolchain " + toolchain + ", testcontainers tests skipped)\"",
	}, "\n")

	base := m.rustBaseFor(toolchain).
		// The test leg is the only one that runs a full `cargo build --all-features`
		// PLUS the workspace test + doctest suites in one container. Since the functions
		// runtime pulled V8 into --all-features (a very memory-heavy compilation unit),
		// 16 parallel rustc jobs peak over this box's 31 GiB RAM and the OOM killer
		// takes rustc/doctest processes (bare exit-101, no diagnostic). Cap this leg to
		// 8 jobs (the other legs stay at the base 16 — they don't OOM). See #615.
		WithEnvVariable("CARGO_BUILD_JOBS", "8")
	return rustSource(base, source, targetVol).
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
	// pgImage pins the integration Postgres at the supported floor, PostgreSQL 18
	// (#1452). pgvector/pgvector:pg18 = the official postgres:18 plus the pgvector
	// extension (#386's executed vector-similarity suites CREATE and query it);
	// mirrored by mirror-base-images.yml like every other base image.
	pgImage = "ghcr.io/fraiseql/pgvector:pg18"
	// pgBelowFloorImage is the newest major release BELOW the floor. Every other
	// service runs pgImage, where a connection that never asked the server its
	// version passes as well as one that did; the floor-refusal suites need a
	// server that is actually older.
	pgBelowFloorImage    = "ghcr.io/fraiseql/postgres:17"
	pgBelowFloorBindHost = "postgres-below-floor"
	// pgUser/pgPassword/pgDatabase are the test-only Postgres credentials.
	pgUser     = "fraiseql_test"
	pgPassword = "fraiseql_test_password"
	pgDatabase = "test_fraiseql"
	// pgBindHost is the service-binding alias; bound callers reach Postgres here on
	// its internal 5432 (not the legacy host-mapped 5433).
	pgBindHost = "postgres"
	// requirePostgresEnv is set on every leg that binds DATABASE_URL. There,
	// fraiseql-test-support's postgres() and try_database_url() fail instead of
	// returning None when they find no database, unset or unreachable: in a leg that
	// promised one, a test that skipped would read as a pass while asserting nothing.
	requirePostgresEnv = "FRAISEQL_TEST_REQUIRE_POSTGRES"
	// requireAllFeaturesEnv is set on the leg that compares the committed server
	// configuration schema, integrationServer, and on no other. There, fraiseql-server's drift test fails rather than being
	// ignored when the build leaves a feature off: the committed schema is the one every
	// feature generates, and this is the one run that checks it.
	requireAllFeaturesEnv = "FRAISEQL_TEST_REQUIRE_ALL_FEATURES"

	// pgStandbyBindHost — a REAL streaming standby of pgService, for the #957
	// bounded-staleness suite. The pre-existing read-replica tests stand a second
	// independent database in for a replica, which shows which server answered but
	// reports no replication lag at all (`pg_is_in_recovery()` is false there), so
	// `max_lag_ms` proven against it would be proven against a server that cannot
	// be stale. Credentials are the replication role `postgres-replication-init.sh`
	// creates on the primary.
	pgStandbyBindHost = "postgres-standby"

	// pgTviewsBindHost — pgImage + pg_tviews (docker/pg-tviews, pinned there), for the
	// suites that exercise pg_tviews only (#1391). Every other suite keeps pgService.
	pgTviewsBindHost = "postgres-tviews"
	// pgTviewsUnpreloadedBindHost — the same image, pg_tviews installed and then restarted
	// without it preloaded (docker/pg-tviews/unpreloaded-entrypoint.sh), for the /metrics
	// case that reads TVIEWS_UNPRELOADED_DATABASE_URL (#1551).
	pgTviewsUnpreloadedBindHost = "postgres-tviews-unpreloaded"
	// requireTviewsUnpreloadedEnv makes that case fail, not skip, where its URL is unset.
	requireTviewsUnpreloadedEnv = "FRAISEQL_TEST_REQUIRE_TVIEWS_UNPRELOADED"
	pgReplicationUser           = "fraiseql_repl"
	pgReplicationPassword       = "fraiseql_repl_password"

	// pgAuthUser/pgAuthPassword — first-party auth's own role (#1965), which
	// `postgres-auth-role-init.sh` creates on the primary: no superuser, no BYPASSRLS,
	// allowed to create in the test database so the auth DDL can create schema `auth`.
	pgAuthUser     = "fraiseql_auth"
	pgAuthPassword = "fraiseql_auth_test_password"

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
	// Pinned like every other mirrored image (tools/check-image-mirror.py).
	localstackImage    = "ghcr.io/fraiseql/localstack:3.8"
	localstackBindHost = "localstack"

	// mailhogImage / mailhogBindHost — the MailHog SMTP sink for the #349 email
	// happy-path test. Speaks real SMTP on 1025 (plaintext) and exposes an HTTP
	// API on 8025 to inspect captured messages; the test sends through lettre and
	// asserts the message arrived in the sink.
	mailhogImage    = "ghcr.io/fraiseql/mailhog:v1.0.1"
	mailhogBindHost = "mailhog"
	// greenmailImage / greenmailBindHost — the test IMAPS/SMTP server of the
	// inbound_email capability fixture (#1757), mirrored by digest. It serves a keystore
	// greenmailCerts generates per run, never the self-signed one it bundles.
	greenmailImage    = "ghcr.io/fraiseql/greenmail:2.1.0"
	greenmailBindHost = "greenmail"

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
	fakeGcsImage    = "ghcr.io/fraiseql/fake-gcs-server:1.56.1"
	fakeGcsBindHost = "fake-gcs"

	// minioImage / minioBindHost / minioUser / minioPass — the S3-compatible MinIO
	// service for fraiseql-server's storage_minio integration test. The test reads
	// MINIO_ENDPOINT (http://<alias>:9000) and authenticates with the constants below.
	// Chainguard's from-source build, mirrored by digest (#1453): MinIO no longer
	// publishes community images. Its entrypoint is `minio`, as upstream's was.
	minioImage    = "ghcr.io/fraiseql/minio:RELEASE.2026-09-22T19-25-18Z"
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
	// The http-e2e suite's server binary builds on a volume apart from integrationBase's
	// (`integ4-<toolchain>`, mounted by the same leg's test exec). Its name kept the
	// toolchain of the `integ4-` bump (2026-07-27) when the MSRV moved on; lane 0 keeps
	// that name, so the volume stays warm.
	serverE2eTargetVol = "fraiseql-rust-target-integ4-1-92"
)

// integrationTargetVol names integrationBase's target volume for a toolchain and a lane.
func integrationTargetVol(toolchain string, lane int) string {
	return laneVolume("fraiseql-rust-target-integ4-"+strings.ReplaceAll(toolchain, ".", "-"), lane)
}

// laneVolume names a lane's copy of an integration target volume. The integration
// volumes are mounted PRIVATE (rustSource), so two suites on one engine queue on one
// volume; a lane of its own gives each concurrent suite its own. Lane 0 is the base
// name unchanged, the volume every run used before lanes, so it stays warm; lane N
// takes the `-lane-N` suffix feature-combos.go gives its lanes.
func laneVolume(base string, lane int) string {
	if lane == 0 {
		return base
	}
	return fmt.Sprintf("%s-lane-%d", base, lane)
}

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
	// The runner's lane: each lane builds into target volumes of its own, so suites on
	// different lanes of one engine run side by side instead of queueing on one PRIVATE
	// volume. Lane 0 is the unsuffixed volume every run used before lanes existed.
	// +optional
	// +default=0
	lane int,
) (string, error) {
	if lane < 0 {
		return "", fmt.Errorf("lane must be 0 or more, got %d", lane)
	}
	switch suite {
	case "", "postgres":
		return m.integrationPostgres(ctx, source, lane)
	case "nats":
		return m.integrationNats(ctx, source, lane)
	case "observers":
		return m.integrationObservers(ctx, source, lane)
	case "http-e2e":
		return m.integrationHTTPE2e(ctx, source, lane)
	case "tls":
		return m.integrationTLS(ctx, source, lane)
	case "server":
		return m.integrationServer(ctx, source, lane)
	case "redis":
		return m.integrationRedis(ctx, source, lane)
	case "vault":
		return m.integrationVault(ctx, source, lane)
	case "wire":
		return m.integrationWire(ctx, source, lane)
	case "storage":
		return m.integrationStorage(ctx, source, lane)
	case "server-storage":
		return m.integrationServerStorage(ctx, source, lane)
	case "federation":
		return m.integrationFederation(ctx, source, lane)
	case "federation-compose":
		return m.integrationFederationCompose(ctx, source)
	case "saml":
		return m.integrationSaml(ctx, source, lane)
	case "quickstart":
		return m.integrationQuickstart(ctx, source, lane)
	case "examples":
		return m.integrationExamples(ctx, source, lane)
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
func (m *FraiseqlCi) integrationQuickstart(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	script := strings.Join([]string{
		"set -e",
		"echo \"### python: $(python3 --version)\"",
		"echo '### integration: quickstart (docs/guides/getting-started.md verbatim, #734)'",
		"bash tools/quickstart-smoke.sh",
		"echo 'test-integration OK: quickstart suite passed'",
	}, "\n")

	// The smoke drives the documented user tooling, not the test harness: psql applies
	// the doc's setup.sql, curl issues the doc's query, and the doc's
	// `pip install fraiseql` is substituted with the in-repo SDK, on the Python it
	// requires. smokeContainer builds the binaries and puts them on PATH.
	return m.smokeContainer(source, lane).
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
func (m *FraiseqlCi) integrationExamples(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	script := strings.Join([]string{
		"set -e",
		"echo \"### python: $(python3 --version)\"",
		"echo '### integration: examples (compile tier + smoke tier)'",
		"export FRAISEQL_BIN=/usr/local/bin/fraiseql-cli",
		"export SERVER_BIN=/usr/local/bin/fraiseql-server",
		"bash tools/check-examples-compile.sh",
		"bash tools/examples-smoke.sh",
		"echo 'test-integration OK: examples suite passed'",
	}, "\n")

	// The smoke drives the tools a reader drives: psql applies each example's
	// setup.sql, curl asks the booted server a question, and every example authors
	// through the in-repo SDK, on the Python it requires — five example schema.py files
	// still did `from fraiseql import key` when this landed, and nothing had run them
	// since v1.
	return m.smokeContainer(source, lane).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		// examples-smoke.sh CREATEs and DROPs a database per example, so it needs a
		// URL it can connect to while doing that — the maintenance database, not one
		// of its own.
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable(requirePostgresEnv, "1").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// smokePythonBase is the container the quickstart and examples legs run the in-repo
// Python SDK in: smokePythonImage, with psql and curl, which the smokes drive as a reader
// would, and the SDK's runtime dependencies, hash-pinned in
// tools/smoke-python-requirements.txt. Nothing of the source tree is mounted yet, and the
// requirements file is the only input taken from it, so both layers are cached across
// pushes until an engine GC.
func (m *FraiseqlCi) smokePythonBase(source *dagger.Directory) *dagger.Container {
	const requirements = "/opt/smoke-python-requirements.txt"
	return dag.Container().
		From(smokePythonImage).
		WithExec(aptInstall("postgresql-client", "curl")).
		WithFile(requirements, source.File("tools/smoke-python-requirements.txt")).
		WithExec([]string{
			"pip", "install", "--no-cache-dir", "--require-hashes", "--no-deps", "-r", requirements,
		})
}

// smokeBinaries builds fraiseql-cli and fraiseql-server in the leg's Rust container and
// returns them, for a smoke that runs them in smokePythonBase. They are copied out of the
// target volume, which is a cache mount and so not part of the container's files.
func (m *FraiseqlCi) smokeBinaries(source *dagger.Directory, lane int) *dagger.Directory {
	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"bash tools/ci-target-canary.sh -- build -p fraiseql-cli --bin fraiseql-cli", // #880 canary wraps the build
		"cargo build -p fraiseql-server --bin fraiseql-server",
		"mkdir -p /out",
		"cp target/debug/fraiseql-cli target/debug/fraiseql-server /out/",
	}, "\n")
	return m.integrationBase(source, rustMsrv, lane).
		WithExec([]string{"bash", "-c", script}).
		Directory("/out")
}

// smokeContainer is smokePythonBase with the two binaries on PATH and the source at /src.
// FRAISEQL_LEG_NONCE, as rustSource sets it, keeps a re-run of the same commit from
// being answered from the cache.
func (m *FraiseqlCi) smokeContainer(source *dagger.Directory, lane int) *dagger.Container {
	bins := m.smokeBinaries(source, lane)
	return m.smokePythonBase(source).
		WithFile("/usr/local/bin/fraiseql-cli", bins.File("fraiseql-cli")).
		WithFile("/usr/local/bin/fraiseql-server", bins.File("fraiseql-server")).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithEnvVariable("FRAISEQL_LEG_NONCE", strconv.FormatInt(time.Now().UnixNano(), 10))
}

// integrationPostgres binds a seeded PostgreSQL 18 service — plus a real streaming
// standby of it (#957) — and runs the PostgreSQL integration tests that already
// route through the harness. The harness reads DATABASE_URL (injected below) and
// connects to the bound service; the read-replica lag suite additionally reads
// STANDBY_DATABASE_URL.
func (m *FraiseqlCi) integrationPostgres(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: postgres (Dagger-bound service; tests read DATABASE_URL via harness)'",
		// Broad core/db `--test '*'` sweep (matches the legacy integration-postgres job).
		// The redis/federation-gated tests skip cleanly (only pg is bound).
		"bash tools/ci-target-canary.sh -- test -p fraiseql-core --features '" + coreTestFeatures + ",test-postgres' --test '*'", // #880 canary
		"cargo test -p fraiseql-core --features '" + coreTestFeatures + ",test-postgres' --test '*' -- --test-threads=1",
		"cargo test -p fraiseql-db --features '" + dbTestFeatures + ",test-postgres' --test '*' -- --test-threads=1",
		// The shared instant type as a timestamptz, through sqlx and tokio-postgres, beside
		// chrono over the same values.
		"cargo test -p fraiseql-time --all-features --test timestamptz_pg -- --ignored --test-threads=1",
		// fraiseql-db keeps every database-backed test in tests/ (#1370): the line above
		// runs them, and `--lib` can never reach one, whatever features it enables.
		// The boot-lock harness drops a test's database even when the test does not.
		"cargo test -p fraiseql-test-support --features boot-locks --lib boot_locks:: -- --test-threads=1",
		// Tier-C migrated: fraiseql-functions cron-state migration (lib tests; harness postgres()).
		"cargo test -p fraiseql-functions --features 'runtime-deno,runtime-wasm,host-live' --lib migrations::tests -- --test-threads=1",
		// The Model B cursor ops through the Deno runtime need this leg's database: in the
		// `Test` leg's deno run they self-skip, having none. V8 runs here as it does there.
		"cargo test -p fraiseql-functions --features 'runtime-deno,runtime-wasm,host-live' --lib runtime::deno::cursor_tests:: -- --test-threads=1",
		// #411 durable identity store (PostgresAccountStore: auth.tb_user / tb_auth_identity + RLS).
		"cargo test -p fraiseql-auth --test postgres_account_store -- --test-threads=1",
		// #1450: a minted session carries the account's tenant under the tenant claim.
		"cargo test -p fraiseql-auth --test postgres_session_tenant -- --test-threads=1",
		// A session for a user enrolled in MFA is minted only through the second factor:
		// the gate sits where every sign-in path mints, so a path cannot skip it.
		"cargo test -p fraiseql-auth --test postgres_session_mfa -- --test-threads=1",
		// The first-party issuer's ES256 key ring: one active key, retired keys published
		// for their window, private halves sealed to their own kid.
		"cargo test -p fraiseql-auth --test postgres_signing_key -- --test-threads=1",
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
		// #1857 OTP guesses are the address's, not the code's: a resend issues a new code
		// but no new guesses, and the budget returns only with the send window.
		"cargo test -p fraiseql-auth --test postgres_otp_guess_budget -- --test-threads=1",
		// #1029 `integration` is a `mod` aggregator: the suite-coverage gate counted
		// only its entry file, scored it zero tests and dropped it from every check,
		// so its 36 tests — several of which self-skip without DATABASE_URL — ran
		// nowhere while the gate printed OK over them.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test integration -- --test-threads=1",
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
		// #1831: in parallel, not --test-threads=1. A test that owns whole tables does so on
		// a schema of its own (testing::pg_schema); a serial leg could not see the shared-table
		// races that made these red under any parallel run. #1787 will rework how legs select
		// tests: keep this line parallel when it does.
		// `auth` and `server::parity_tests::`: the parity suite's inbound arms, and the one
		// that needs `auth` with `inbound-email` (a local-auth OTP naming an unknown
		// mailbox), compiled in no leg until the suite-coverage gate followed `mod`
		// declarations.
		// `server::provision_boot_locks_tests::`: a second boot of each server store over a
		// current schema waits on no lock a live write holds; each test owns a database.
		// `server::routing::admin_gate_pin_tests::`: the admin token gates through the server's
		// mounts; its SQL-console cases need `admin-sql`, which the Test leg's lib run leaves out.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --lib -- inbound:: server::provision_tests:: server::provision_boot_locks_tests:: server::parity_tests:: server::routing::mount_gate_tests:: server::routing::admin_gate_pin_tests:: server_config::tests::refused_in_every_deployment::",
		// #974: the storage bucket-policy admin API (PUT/GET/DELETE) against a
		// real policy store — the split-token separation, the request-time
		// refusal that leaves the running policy in place, and the wholesale
		// store-over-config precedence. The DB-less test leg skips it by name.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --lib server::routing::storage_policy_admin_tests -- --test-threads=1",
		// #1333: the tenant-executor RuntimeConfig pins. These build a real tenant pool,
		// so in the DB-less `test` leg they self-skip and report `ok` while asserting
		// nothing — a skip reads as passing. They are the drift comparison against a
		// factory-built executor, the Authorizer actually binding on one, and the proof
		// the config is read per registration rather than captured. Serial: each
		// registration provisions its own pool.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --lib tenancy::tests::runtime_config_drift -- --test-threads=1",
		// #2522: the server lib's database-backed tests in the build without `auth`, which
		// the DB-less Test leg skips or sees self-skip. A fixture naming a section only an
		// `auth` build has fails here, against a real database.
		// feature-off: auth — server::provision_tests
		"cargo test -p fraiseql-server --no-default-features --features '" + serverWithoutAuthFeatures + "' --lib -- server::provision_tests:: server::routing::storage_policy_admin_tests:: tenancy::tests::runtime_config_drift:: --test-threads=1",
		// #775: per-mailbox spine scoping + content-digest dedup key. Drives
		// EmailIngestSink against real PG (no IMAP, no real mailbox): the same
		// Message-ID to two mailboxes lands twice; a pre-claimed Message-ID cannot
		// suppress a genuine, differently-bodied message.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test inbound_email_dedup_scope_pg -- --test-threads=1",
		// #1782: an inbound webhook delivery is claimed once, by the spine insert. Through
		// the mounted route with an after:ingest plan: a redelivery is a duplicate, a
		// failure after the claim rolls it back, a concurrent pair emits once, and the
		// spine DDL converges on a table created before `event_type`.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test webhook_single_claim_e2e_pg -- --test-threads=1",
		// #573 source scheduler (Model B): the SourceQueryExecutor identity/tenant
		// seam, the source poller build_host composition (cursor round-trip vs PG +
		// executor reachable via host.query), and the scheduler's schedulable/config
		// resolution. `sources` pulls the Deno path (compiled only). The
		// fires_a_model_b_connector_end_to_end name-skip is lifted (#971):
		// the "V8 SIGSEGVs in the exec sandbox" diagnosis was #969's
		// second-isolate-per-process crash, fixed there — this run is the
		// sandbox re-test the issue asked for. stranded_tables:: is gated on
		// any(observers, sources, inbound-email).
		// `capability::schema_tests::` holds a sources-gated arm that ran in no leg.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --lib -- sources:: stranded_tables:: server_config::egress:: server::provision_tests::each_configured_capability_plans_its_own_tables capability::schema_tests:: --test-threads=1",
		// P16 functions-runtime gate (#796/#803/#840/#841/#842): the cron
		// fire-window + cross-restart guard vs real PG (`cron::tests` reads
		// back `_fraiseql_cron_state`), and the dispatched host's identity /
		// send_email / env-allowlist wiring (`routes::after_mutation::tests`).
		// These are feature-gated on functions-runtime, which serverTestFeatures
		// deliberately omits — so before this line NO leg executed them.
		// (The #804 watchdog test is runtime-deno and stays local-only:
		// embedded V8 SIGSEGVs in the exec sandbox, see docs/contributing/dagger-parity-notes.md.)
		// `schema::tests::` (#1326): the loader's feature-ON arm — a `functions`
		// section LOADS in a build that can serve one, where a lean build refuses it.
		// Both arms have to run somewhere, and a filtered `--lib` line runs only what its
		// filter names, so without the filter entry the ON arm runs nowhere and reads
		// as passing. check-suite-coverage.py caught it.
		// #992: widened beyond the original P16 cron/after_mutation filters —
		// query_bridge, subsystems::loader, function_metrics and the
		// pg_function_dlq observers module are functions-runtime-gated too and
		// executed in no leg.
		// `auth` + `identity::` + `observers::changelog_handlers::` joined in
		// #1297. Both modules reach Postgres through a skip-clean getter
		// (`fraiseql_test_support::postgres()`, `try_database_url()`) and the only
		// leg compiling them was the service-less workspace run, where every
		// DB-backed case returned early and read exactly like a pass.
		// `function_dispatch::` and `server::provision_tests::` joined in #1628:
		// the dispatch ledger's caller replay and its boot provisioning are
		// functions-runtime-gated, and no other `--lib` line compiled them.
		// `server::entry_point_parity_tests::`, `server::health_capabilities_tests::` and
		// `subsystems::loader::` run here with every feature ON, and below with the
		// features their OFF arms need turned off.
		// Three more ran in no leg until the suite-coverage gate found lib test modules
		// by declaration rather than by a `tests.rs` file name: the functions-runtime
		// arms of `server::health_capabilities_tests::` and of
		// `server::initialization::tests::static_enriched_fields::functions::`, and
		// `server::declared_observer_env_tests::`, whose refused-boot case needs this
		// leg's database and self-skipped in the DB-less `Test` leg.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --lib -- cron:: routes::after_mutation:: server::after_mutation_wiring_tests:: server::functions_setup:: query_bridge:: subsystems::loader:: schema::tests:: function_metrics:: observers::pg_function_dlq:: identity:: observers::changelog_handlers:: function_dispatch:: server::provision_tests:: server::entry_point_parity_tests:: server::health_capabilities_tests:: server::initialization::tests::static_enriched_fields::functions:: server::declared_observer_env_tests:: --test-threads=1",
		// The entry-point parity and `/health` capability suites boot a server with the
		// observer runtime, through a skip-clean `postgres()` getter. The `Test` leg
		// compiles them with no database, where they returned early and read as a pass.
		// Their feature-OFF arms need a build without the feature, against the database.
		// Without `arrow`: the parity fixture names an Arrow-only config field that only
		// such a build refuses. The same build keeps `functions-runtime` without Deno,
		// the arm in which the function loader refuses a Deno function loud.
		// feature-off: arrow — server::entry_point_parity_tests
		// feature-off: functions-runtime-deno — subsystems::loader::tests
		"cargo test -p fraiseql-server --features '" + serverWithoutArrowOrDenoFeatures + "' --lib -- server::entry_point_parity_tests:: subsystems::loader:: --test-threads=1",
		// Without `functions-runtime`, as the default server build ships: the parity
		// fixture's functions capability is absent, and `/health` reports it so.
		// feature-off: functions-runtime — server::entry_point_parity_tests
		"cargo test -p fraiseql-server --features '" + serverWithoutFunctionsFeatures + "' --lib -- server::entry_point_parity_tests:: server::health_capabilities_tests:: --test-threads=1",
		// `fraiseql run` boots through the binary's assembly; the observer runtime is
		// what proves it holds the pool, so the server's `observers` is switched on.
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --lib commands::run_serve_tests:: -- --test-threads=1",
		// `fraiseql run --admin-database-url` creates and grants the engine's and auth's roles in
		// development. The suite runs the binary, so it needs `run-server`; the features match
		// the line above, which has built the server already.
		"echo '### cargo test -p fraiseql-cli --test run_admin_provisioning_pg (development role provisioning)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test run_admin_provisioning_pg -- --test-threads=1",
		// `fraiseql auth rotate-signing-key` retires the first-party issuer's active key in a
		// database of its own per case; same features, so the binary is already built.
		"echo '### cargo test -p fraiseql-cli --test auth_rotate_signing_key_pg (signing-key rotation)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test auth_rotate_signing_key_pg -- --test-threads=1",
		// #896: the functions subsystem is configured from the schema the server was
		// built with, on BOTH serving entry points. Its own binary, and
		// functions-runtime-gated, so it belongs on this line rather than in
		// serverInProcessTests — which omits the feature and would run zero tests.
		// `mcp` adds the third entry point, serve_mcp_stdio (#1332).
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test functions_schema_seam_test",
		// #1082: same argument as the line above, and the case that proved the gate
		// blind. This binary is `#![cfg(feature = "functions-runtime")]` at file level,
		// which cargo cannot see, so naming it in serverInProcessTests built an EMPTY
		// binary that reported "test result: ok. 0 passed" and read as covered.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test functions_query_bridge_pin_test",
		// (#869: the rich-filter surface — including the #721 sql_templates_execute_pg
		// suite — was removed in P25: the compiler advertised WHERE operators the
		// runtime could never parse. The compiler↔runtime contract test now lives in
		// fraiseql-cli's lib tests: compiled_schema_advertises_no_unservable_where_operators.)
		// #823/#822/#569 — the first-run e2e: `fraiseql init` scaffolds, every
		// printed next step is executed, the scaffold's DDL is applied, and a
		// query + a mutation run through the real executor against PostgreSQL.
		"echo '### cargo test -p fraiseql-cli --test init_first_run_pg (#823/#822 first-run e2e)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test init_first_run_pg -- --test-threads=1",
		// #821 — `generate-views --validate` executes the DDL against PostgreSQL
		// in a rolled-back transaction; this suite proves it can PASS and FAIL.
		"echo '### cargo test -p fraiseql-cli --test generate_views_validate_pg (#821 validate can fail)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test generate_views_validate_pg -- --test-threads=1",
		// #1626: validate facts reads an authoring schema.json, so a declared fact table the
		// database lacks fails the command instead of "no fact tables declared".
		"echo '### cargo test -p fraiseql-cli --test validate_facts_pg (#1626 validate facts can fail)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test validate_facts_pg -- --test-threads=1",
		// #384 — `compile --database` is a drift linter that can FAIL: error-severity
		// schema↔database drift exits non-zero and writes no artifact; doctor
		// reports the same drift as structured JSON. Proves both directions.
		"echo '### cargo test -p fraiseql-cli --test compile_drift_fail_pg (#384 drift linter can fail)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test compile_drift_fail_pg -- --test-threads=1",
		"echo '### cargo test -p fraiseql-cli --test native_column_nullability_pg (#1533 NOT NULL recorded where proven)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test native_column_nullability_pg -- --test-threads=1",
		// What an author declares in schema.json is what the runtime serves, compiled with the
		// real CLI and read through the real executor (#1586).
		"echo '### cargo test -p fraiseql-cli --test authored_contract_pg (declaration = served contract)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test authored_contract_pg -- --test-threads=1",
		// #1607: `compile --database` refuses a schema declaring [accent_insensitive] against a
		// database without its wrapper, or with one an index cannot use, and compiles one whose
		// wrapper is ready.
		"echo '### cargo test -p fraiseql-cli --test accent_insensitive_against_db_pg (#1607 wrapper drift)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test accent_insensitive_against_db_pg -- --test-threads=1",
		// #384 verification suites that had NEVER run with a database in any leg:
		// each self-skips without DATABASE_URL, and the workspace test leg (which
		// compiles them under --all-features) binds no Postgres — so all three read
		// green while running zero tests. The remaining self-skipping -cli suite
		// (cascade_rls_against_db) is tracked in its own issue.
		"echo '### cargo test -p fraiseql-cli --test mutation_contract_against_db --test doctor_against_db --test source_probe_against_db (#384 against-db gates)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test mutation_contract_against_db -- --test-threads=1",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test doctor_against_db -- --test-threads=1",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test source_probe_against_db -- --test-threads=1",
		// #1690 — a function root's function is checked against pg_proc: it binds the
		// query's parameters, returns a set, and is not VOLATILE.
		"echo '### cargo test -p fraiseql-cli --test function_root_against_db (#1690 function-root contract)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test function_root_against_db -- --test-threads=1",
		// #960 — the cascade RLS conformance proof (2-tenant isolation incl. the
		// default-privilege-view LEAK proof) had never executed its assertions in
		// CI: it self-skips without DATABASE_URL and only the DB-less test leg
		// compiled it. One consolidated test — expect `1 passed, 0 skipped` here.
		"echo '### cargo test -p fraiseql-cli --test cascade_rls_against_db (#960 cascade RLS boundary proof)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test cascade_rls_against_db -- --test-threads=1",
		// #992: the remaining self-skipping fraiseql-cli against-db suites, same
		// shape as #960/#384 — green-while-running-zero-tests until named here.
		"echo '### cargo test -p fraiseql-cli remaining against-db suites (#992)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test setup_against_db -- --test-threads=1",
		// #1427: `federation sdl` and `_service { sdl }` agree on one artifact.
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test federation_sdl_against_db -- --test-threads=1",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test sources_against_db -- --test-threads=1",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test validate_sql_sources_gate -- --test-threads=1",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test runtime_smoke -- --test-threads=1",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test perf_against_db -- --test-threads=1",
		// #1452: the CLI's own connections refuse PostgreSQL 17 (BELOW_FLOOR_DATABASE_URL).
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test server_version_floor_pg -- --test-threads=1",
		// `fraiseql migrate` is a wrapper around confiture, and for two releases every verb it
		// shelled (`confiture status --source DIR`) was a command and an option no confiture
		// release has had. Its only tests self-skipped whenever confiture was present, so the
		// wrapper was tested exactly where the wrapped tool was absent. This leg installs
		// confiture from an exact pin set (transitive versions included; the file carries the
		// regeneration command) and runs the wrapper against it. The suite fails, rather
		// than skips, when DATABASE_URL is bound and confiture is not on PATH.
		"apt-get install -y --no-install-recommends python3-venv >/dev/null",
		// confiture needs Python 3.14 (1.30.0+); this image has 3.11. uv, pinned, fetches it.
		"python3 -m venv /tmp/uv",
		"/tmp/uv/bin/pip install --quiet uv==0.9.18",
		"/tmp/uv/bin/uv venv --quiet --python 3.14.2 /tmp/confiture",
		"/tmp/uv/bin/uv pip install --quiet --python /tmp/confiture/bin/python -r /src/tools/confiture-requirements.txt",
		"export PATH=/tmp/confiture/bin:$PATH",
		"echo \"### confiture: $(confiture --version)\"",
		"echo '### cargo test -p fraiseql-cli --test migrate_against_confiture (fraiseql migrate → confiture call shape)'",
		"cargo test -p fraiseql-cli --features '" + cliPostgresFeatures + "' --test migrate_against_confiture -- --test-threads=1",
		// #936: the seed-fixture integrity gate runs LAST — any clobber a suite
		// above introduced fails here, naming the fixture.
		"echo '### cargo test -p fraiseql-db --test seed_fixture_integrity (#936 gate, runs last)'",
		"cargo test -p fraiseql-db --features postgres,wire-backend,test-postgres --test seed_fixture_integrity -- --test-threads=1",
		"echo 'test-integration OK: postgres suite passed'",
	}, "\n")

	return m.withPostgresServices(m.integrationBase(source, rustMsrv, lane), source).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// withPostgresServices binds what integrationPostgres runs against: a seeded primary,
// two real streaming standbys of it, and a server below the version floor, with the URLs
// its suites read. CoveragePostgres measures over the same bindings, so the two cannot
// come to differ in what a database-backed test finds.
func (m *FraiseqlCi) withPostgresServices(c *dagger.Container, source *dagger.Directory) *dagger.Container {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	standbyURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgStandbyBindHost, pgDatabase)
	failoverURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgFailoverBindHost, pgDatabase)
	belowFloorURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBelowFloorBindHost, pgDatabase)

	// One pgService instance, bound to both the test container and the standby:
	// the suite writes on the primary and reads through the standby, so a second
	// pgService(source) call would leave the standby streaming from a database
	// nobody writes to and its measured lag permanently zero.
	primary := m.pgService(source)

	return c.
		WithServiceBinding(pgBindHost, primary).
		WithServiceBinding(pgStandbyBindHost, m.pgStandbyService(source, primary, "fraiseql_standby")).
		WithServiceBinding(pgFailoverBindHost, m.pgStandbyService(source, primary, pgFailoverSlot)).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable(requirePostgresEnv, "1").
		WithEnvVariable("STANDBY_DATABASE_URL", standbyURL).
		WithEnvVariable("FAILOVER_STANDBY_DATABASE_URL", failoverURL).
		WithServiceBinding(pgBelowFloorBindHost, m.pgBelowFloorService()).
		WithEnvVariable("BELOW_FLOOR_DATABASE_URL", belowFloorURL)
}

// CoveragePostgres measures line coverage of fraiseql-core and fraiseql-db over the
// tests CI runs for them, the database-backed ones included, and returns the report
// (lcov.info, coverage.json). The hosted coverage workflow cannot bind what those tests
// need: streaming standbys and a server below the version floor.
//
// It runs no line of its own. `tools/leg-lines.py --crates` prints the two crates' cargo
// test lines from `Test` and `integrationPostgres` as those shards run them, and each is
// executed unchanged under `cargo llvm-cov show-env`, which instruments a plain `cargo
// test`. A reader that prints no line for either crate fails the function, and so does a
// failing test: a number measured over fewer tests than CI runs would read as coverage.
// The thresholds are the ones the coverage workflow held these crates to.
//
//	dagger call coverage-postgres --source=. export --path=coverage
func (m *FraiseqlCi) CoveragePostgres(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// The runner's lane, as for TestIntegration: the instrumented build has a target
	// volume of its own per lane.
	// +optional
	// +default=0
	lane int,
) (*dagger.Directory, error) {
	if lane < 0 {
		return nil, fmt.Errorf("lane must be 0 or more, got %d", lane)
	}
	script := strings.Join([]string{
		"set -euo pipefail",
		"echo \"### toolchain: $(rustc --version); $(cargo llvm-cov --version)\"",
		"eval \"$(cargo llvm-cov show-env --sh)\"",
		"cargo llvm-cov clean --workspace",
		// The assignment fails the script when the reader does: no line for a crate.
		"lines=\"$(python3 tools/leg-lines.py --crates fraiseql-core,fraiseql-db Test integrationPostgres)\"",
		// In this shell, not a child bash: BASH_ENV would stamp the sources again before
		// every line and rebuild the workspace each time.
		"while IFS= read -r line; do echo \"### $line\"; eval \"$line\"; done <<<\"$lines\"",
		"mkdir -p coverage",
		"cargo llvm-cov report --lcov --output-path coverage/lcov.info",
		"cargo llvm-cov report --json --output-path coverage/coverage.json",
		"cargo llvm-cov report -p fraiseql-core --fail-under-lines 70",
		"cargo llvm-cov report -p fraiseql-db --fail-under-lines 65",
	}, "\n")

	toolchain := resolveToolchain(rustMsrv)
	base := m.rustBaseFor(toolchain).
		WithExec([]string{"bash", "-c", "rustup component add llvm-tools-preview"}).
		WithExec([]string{"bash", "-c", "cargo install cargo-llvm-cov --version " + cargoLlvmCovVersion + " --locked"})
	volume := laneVolume("fraiseql-rust-target-cov-"+strings.ReplaceAll(toolchain, ".", "-"), lane)
	built := m.withPostgresServices(rustSource(base, source, volume).WithEnvVariable("RUST_LOG", "debug"), source).
		WithExec([]string{"bash", "-c", script})
	if _, err := built.Sync(ctx); err != nil {
		return nil, err
	}
	return built.Directory("/src/coverage"), nil
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
func (m *FraiseqlCi) integrationSaml(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	authURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgAuthUser, pgAuthPassword, pgBindHost, pgDatabase)

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
		// First-party auth's tables live in schema `auth`: every auth script, applied twice
		// to an empty database, creates exactly the expected tables there and nothing
		// outside it. Here and not in the `postgres` leg: it applies the SAML scripts too,
		// which need `auth-saml`, and the `postgres` leg builds fraiseql-auth without it.
		"cargo test -p fraiseql-auth --features auth-saml --test auth_schema_inventory_pg -- --test-threads=1",
		// A second boot of every auth store waits on no lock a live write holds, and a boot
		// over a schema that lost a guarded object builds it again. Here for the same reason:
		// the suite applies the SAML scripts, which need `auth-saml`.
		"cargo test -p fraiseql-auth --features auth-saml --test boot_locks_pg -- --test-threads=1",
		// #381 P26: the SERVER mount — [saml] config → boot provisioning →
		// /auth/saml/login redirect with SAMLRequest; configured-but-broken
		// shapes (no pool, dud metadata) refuse to boot.
		// #947 adds the operator's path on top: manage IdPs over /api/saml/idps on a
		// running server and watch /auth/saml/login follow, scoped by tenant. The `--lib`
		// line runs the management router's own DB-free tests (router construction) — this
		// leg is the only one that enables `auth-saml` on fraiseql-server, so without it that
		// module is compiled out everywhere and reads as passing.
		"echo '### cargo test -p fraiseql-server --test saml_mount_e2e_pg (#381 server mount)'",
		"cargo test -p fraiseql-server --features auth-saml,auth --lib api::saml_idp_management -- --test-threads=1",
		"cargo test -p fraiseql-server --features auth-saml,auth --test saml_mount_e2e_pg -- --test-threads=1",
		// #1913: a sign-in through the ACS, signed as a production IdP signs (SHA-256), and
		// the session it mints: lifetime, issuer, audience, tenant, and the refusals.
		"echo '### cargo test -p fraiseql-server --test saml_acs_e2e_pg (#1913 ACS sign-in)'",
		"cargo test -p fraiseql-server --features auth-saml,auth --test saml_acs_e2e_pg -- --test-threads=1",
		// #946 SCIM provisioning. The store suite carries the property the feature exists
		// for — `active = false` blocks a local-password login, a credential SCIM never
		// touches — and the e2e drives the mounted surface over HTTP.
		"cargo test -p fraiseql-auth --test postgres_scim_provisioning -- --test-threads=1",
		"cargo test -p fraiseql-server --features auth,observers --test scim_provisioning_e2e_pg -- --test-threads=1",
		// #1089 tenant administrators: every tenant-aware admin router (SAML IdPs, SCIM
		// tokens, RBAC) driven as the platform and as two tenants on one booted server. It
		// needs `auth-saml` and `observers` together, which only this leg enables.
		"cargo test -p fraiseql-server --features auth-saml,auth,observers --test tenant_admin_e2e_pg -- --test-threads=1",
		// #946's verification gate: a THIRD-PARTY SCIM client, because a suite we wrote
		// ourselves passes on the shapes we thought of. Okta's validator and the Entra
		// agent are hosted services needing a public URL and a vendor tenant, so neither
		// can run here; scim2-tester can, and it found real defects the hand-written tests
		// missed. Vendor validation stays a manual pre-release step.
		// Installed from an exact pin set, transitive versions included: the verdict comes
		// from third-party code, and an unpinned install lets an upstream release redden an
		// untouched tree. It did, twice in nine hours on 2026-09-20 — the file carries both
		// post-mortems and the regeneration command.
		"apt-get install -y --no-install-recommends python3-venv >/dev/null",
		"python3 -m venv /tmp/scim-tester",
		"/tmp/scim-tester/bin/pip install --quiet -r /src/tools/scim-conformance-requirements.txt",
		"export FRAISEQL_SCIM_TESTER_PYTHON=/tmp/scim-tester/bin/python",
		"cargo test -p fraiseql-server --features auth,observers --test scim_conformance_e2e_pg -- --test-threads=1 --nocapture",
		"echo 'test-integration OK: saml suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv, lane).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable(requirePostgresEnv, "1").
		// Auth's own role, which the ACS suite's two-role sign-in boots auth on.
		WithEnvVariable("AUTH_DATABASE_URL", authURL).
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
		WithExec(aptInstall("nodejs", "npm")).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationServer binds a seeded Postgres and runs fraiseql-server's
// database-query integration tests. They use try_database_url() + skip-on-None
// (no #[ignore]), so they run plainly and execute once DATABASE_URL is injected.
func (m *FraiseqlCi) integrationServer(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	authURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgAuthUser, pgAuthPassword, pgBindHost, pgDatabase)
	tviewsURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgTviewsBindHost, pgDatabase)
	tviewsUnpreloadedURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgTviewsUnpreloadedBindHost, pgDatabase)
	standbyURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgStandbyBindHost, pgDatabase)

	// Every fraiseql-server suite in this leg runs under ONE feature set (#1885).
	// Each distinct `--features` list rebuilds fraiseql-server and whatever of its
	// dependency tree the list changes, and those builds run one after another: at
	// 16 sets they took ~16 min of a warm 25-min run; the tests took seconds. A
	// suite that needs a feature OFF gets its own line, with a comment saying which
	// feature and why: default_build_pools_e2e_pg (#2098) needs `observers` off.
	serverLegFeatures := "--features '" + serverTestFeatures + "," + serverIntegrationExtraFeatures + "'"

	script := strings.Join([]string{
		"set -e",
		suiteCountPrelude,
		"echo \"### toolchain: $(rustc --version)\"",
		// The committed server configuration schema is the one a build with every feature
		// generates, so only this build compares it; narrower builds list the test as
		// ignored. `requireAllFeaturesEnv`, set on this leg, fails the run if it ever is.
		"cargo test -p fraiseql-server --all-features --lib server_config::json_schema::",
		"echo '### integration: server database (Dagger-bound postgres)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-server " + serverLegFeatures + " --test database_query_test", // #880 canary
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test database_query_test -- --test-threads=1",
		// Tier-C migrated (each helper creates + TRUNCATE/DROP its tables for shared-DB isolation).
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test usage_postgres_backend_test -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test observer_repository_test -- --test-threads=1",
		// #1625: every observer admin route acts only on the caller's organisation, driven
		// through the real routers against the stats view and tables the API reads.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test observer_admin_scope_pg -- --test-threads=1",
		// #1670: a webhook body_template is checked against the observed entity's fields at
		// create and update; an update reads the stored observer it leaves.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test observer_admin_templates_pg -- --test-threads=1",
		// #2061: a condition_expression is parsed at create and update, as the runtime parses
		// it; one that does not parse is refused and nothing is written.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test observer_admin_conditions_pg -- --test-threads=1",
		// #751: the inbound webhook replay defence is only provable against a real
		// idempotency claim — the defect it guards lived precisely where no
		// real-system test reached.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test webhook_replay_header_dedup_pg -- --test-threads=1",
		// #1046: both dedup layers are scoped per route, not per provider. Two
		// routes on one provider, each sender's own event 1001, driven through the
		// real route + real claim + real spine. Only a live database can show it:
		// the ledger and the spine fail in the same direction, and fixing just the
		// ledger still answers 200 "processed" while dropping the message.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test webhook_route_dedup_scope_pg -- --test-threads=1",
		// #781/#787: genuine provider-shaped deliveries (Slack timestamp threading,
		// Twilio public_url, LemonSqueezy hex) through the real route + real
		// idempotency claim, plus the boot-time route validation.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test webhook_provider_matrix_pg -- --test-threads=1",
		// #1322: what a token-authenticated delivery WRITES. The scheme's own suite
		// pins what verification reports; this pins that the route's spine row and
		// ledger row come from the signed claims and not from the outer envelope,
		// which the fixture makes contradict them on every field. A response of
		// `{"status":"processed"}` is identical either way, so only the rows can
		// tell — and only a live database has rows. It is also the one place a
		// `jwt-jwks` route meets a real `JwksSource` over HTTP rather than a
		// substituted key source.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test webhook_jwt_jwks_dispatch_pg -- --test-threads=1",
		// #794/#795 (CRITICAL): the analytics injection guards. Both holes were reachable
		// by any client that can POST a GraphQL query, and both were invisible to unit
		// tests because the allowlist that "covered" them was only ever consulted by a
		// planner the shipped binary never calls. This drives the real handler against a
		// real database and asserts both that the request is refused and that no catalog
		// data reaches the response.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test analytics_injection_e2e_pg -- --test-threads=1",
		// #386 — pgvector similarity search, executed: `nearest` row ORDER per
		// metric against real pgvector (the service image ships the extension),
		// threshold WHERE filters returning rows, dimension-mismatch and
		// binary-metric refusals. The first vector suite that runs SQL at all —
		// every prior "vector test" was sql.contains("<=>").
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test graphql_vector_e2e_pg -- --test-threads=1",
		// #387 — GraphQL-over-SSE + root-field @stream, through the real
		// `Server::serve_on_listener` mount: negotiation is opt-in, batches
		// re-enter the full pipeline, auth is enforced before the stream and
		// re-checked per batch, the delivery survives request_timeout_secs, and
		// the compression predicate exempts text/event-stream.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test graphql_sse_e2e_pg -- --test-threads=1",
		// #938 — the `<name>Count` sibling, through the real mount. A count is a
		// second door onto the rows the list query guards, so most of what this
		// pins is refusals: the total ignores limit/offset but not `where`, and
		// it inherits inject-param tenant scoping, `requires_role` and the
		// anonymous refusal. A count that dropped the tenant filter returns no
		// row and leaks another tenant's row total.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test graphql_count_e2e_pg -- --test-threads=1",
		// A `Float` round-trips exactly through the real mount: a request variable and an
		// inline literal each select their own row among doubles one step apart, and the
		// largest double is served exactly. Each was one step off, or unreadable, under the
		// default JSON number reader.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test graphql_float_exact_e2e_pg -- --test-threads=1",
		// #962 — the operator SQL console (G4: mount the full arbitrary-SQL
		// endpoint, gated). The most powerful endpoint the server has, so what
		// this pins is the containment, and every part of it is a *database*
		// behaviour no unit test can see: the preview really rolls back, the
		// commit opt-in really persists, the read-only token's write is refused
		// by the transaction's mode, the row cap and statement timeout fire, one
		// statement is all the protocol accepts, and impersonation sets the
		// session variables the executor would set.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test admin_sql_console_e2e_pg -- --test-threads=1",
		// #966 — `requires_actor` enforced at execution on EVERY transport. The
		// claim is not "the predicate works" (a unit test covers that) but "no
		// door around it exists", so this drives GraphQL query, GraphQL mutation,
		// REST and MCP at the same restricted operation and requires the same
		// refusal from each. Its REST case found a real hole: the direct-read
		// path does not go through the GraphQL entry gates, so a predicate placed
		// only there served every restricted row over REST (#808's shape).
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test actor_predicate_e2e_pg -- --test-threads=1",
		// #812/#739/#810: the REST read surface carried no authentication, discarded the
		// resolved tenant filter, and honoured `require_auth` on one route out of six.
		// None of it was visible to the existing REST suite, which builds its router with
		// `rest_router(...)` directly and so never exercises the server's actual mount.
		// This boots the real mount over a real socket against a real database and
		// asserts two tenants' rows stay apart — authenticated and anonymous.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_tenant_isolation_e2e_pg -- --test-threads=1",
		// #1799: a fact table's aggregate and window roots carried no tenant predicate in a
		// schema that scopes its reads by inject_params, so one tenant's aggregate counted
		// every tenant's rows. Boots the real mount with tokens for two tenants and
		// requires both roots to refuse rather than read across them, anonymous included,
		// and a `row_scope = "relation"` fact table to see its own tenant's rows only.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test fact_table_tenant_isolation_e2e_pg -- --test-threads=1",
		// #1336 — `[identity.enrichment]`'s "every authenticated request resolves and
		// fail-closes" was true of /graphql only. Drives one deployment through two
		// transports and requires the same verdict from each: the GraphQL cases are the
		// control (they pass before the fix), so a red REST case is a transport gap
		// rather than a broken fixture. Needs a real resolver against a real actor
		// table — the defect is invisible without one, because an unwired resolver and
		// an unreached one look identical from the response.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test enrichment_transport_parity_e2e_pg -- --test-threads=1",
		// #1858: roles from the actor row drive requires_role / requires_scope; names the
		// schema never reads grant nothing. Owns its tf_p1858_* fixtures → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test enrichment_roles_e2e_pg -- --test-threads=1",
		// #1512 — the request locale: `[locale]` compiled through the real compiler, loaded
		// through `from_json`, resolved per request and read back as `fraiseql.locale`
		// through a view, on every transport the suite drives; never on a write.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test locale_context_e2e_pg -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test locale_grpc_e2e_pg -- --test-threads=1",
		// #2178/#2213: a gRPC read left out `LocalDateTime` fields and served a temporal
		// value outside its scalar's format. Each is now served in its format, or refused in
		// completion's words. #2192: an `ID` is read as text, as GraphQL serves it.
		// Needs `grpc` (in serverLegFeatures, required-features).
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test grpc_row_view_e2e_pg -- --test-threads=1",
		// #1520 — a `source = "header"` session variable is the request's header, never a
		// claim of the same name, on every transport the suite drives; an anonymous request
		// gets its header and literal variables.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test session_variable_header_source_e2e_pg -- --test-threads=1",
		// #1528 — a mutation value of the wrong type (top-level, nested in a list of inputs,
		// an undeclared input key) is refused before the function runs, on GraphQL, REST and
		// MCP; the function's own call log proves it never ran.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test mutation_value_types_e2e_pg -- --test-threads=1",
		// #1915 — an omitted input field reaches the function as its default (variable, literal,
		// nested, jsonb input, REST); a sent null is kept, or refused on a non-null field.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test input_field_defaults_e2e_pg -- --test-threads=1",
		// #1522 — a stored value missing from a non-null field, on `_entities`, a mutation
		// payload, REST (refused) and MCP (an error result).
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test non_null_completion_transports_e2e_pg -- --test-threads=1",
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
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_write_mount_e2e_pg -- --test-threads=1",
		// #391: durable submit/status/cancel through the production mount, with each of
		// P19's six recovery failure modes pinned (terminal-never-reclaimed, staleness-
		// gated claiming, claim-guarded completion, idempotent submission, truthful
		// cancellation, status-reads-the-row). Owns _system.async_operations + a
		// changelog table → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test async_operations_e2e_pg -- --test-threads=1",
		// #376: an MCP-originated write is tagged transport=mcp in the change-log and
		// Bearer tokens authenticate MCP calls (auth parity with /graphql).
		// Recreates core.tb_entity_change_log → serial, changelog-owning binary.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test mcp_transport_stamp_e2e_pg -- --test-threads=1",
		// #1546: a tool whose operation returns a union, an interface or an auto_error_union
		// result is called through the MCP service against PostgreSQL.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test mcp_abstract_returns_e2e_pg -- --test-threads=1",
		// #390: every authenticated write path (graphql + REST) records a derived,
		// unforgeable actor in the change-log; unauthenticated writes are refused, not
		// recorded unattributed. Recreates core.tb_entity_change_log → serial, and must
		// not share a process with another changelog-owning binary.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test actor_attribution_e2e_pg -- --test-threads=1",
		// #862/#913/#914/#916: bulk update/delete reported rows it never touched, and the
		// "at least one filter" guard was satisfied by parameters contributing no WHERE.
		// Asserts the *table*, because `affected_rows` is exactly the number #913 fabricates.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_bulk_safety_e2e_pg -- --test-threads=1",
		// #863/#864: a client embedding filter could overwrite the parent join key and
		// return another parent's children, and nested embeddings were depth-validated
		// then silently dropped at execution.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_embedding_safety_e2e_pg -- --test-threads=1",
		// #1266: the same surface, but served from a schema `fraiseql compile` produced
		// rather than a hand-built CompiledSchema. Until relationships had an authoring
		// producer, every embed request against a compiled schema was a 400 reading
		// "Available: none" — so the four fixes above were made in code no user reached.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_embedding_compiled_schema_e2e_pg -- --test-threads=1",
		// #1351/#379: `?select=` embedding is one composed statement, so the request's
		// cost and bytes ceilings bound it whole: the cost is scored over the nested
		// projection before the statement is sent, and must equal what the old
		// one-sub-read-per-parent-row fan-out charged. Needs a database because the
		// equality is read off a served request, bracketed from both sides, and because
		// the embedded levels' own gates (target RLS, target field RBAC) are only
		// visible in the rows the statement returns. Drops and recreates its own schema
		// per rig.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_embedding_read_budget_e2e_pg -- --test-threads=1",
		// The composed `?select=` statement gates each embedded level as a read of its own
		// target: the target's RLS predicate in that level's LATERAL, its field-level RBAC
		// (Mask as NULL, Reject refused), its counts scoped alike. Needs a database because
		// what the unit tests cannot show is that PostgreSQL binds each level's unqualified
		// predicate to that level's own view — which only the rows it returns reveal.
		// Creates and drops its own schema.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_embedding_composed_gates_e2e_pg -- --test-threads=1",
		// A GraphQL selection into a nested type — `users { orders { ... } }`, served from
		// the parent view's `data` — against the nested type's own field RBAC and RLS
		// policy. The reproductions are `#[ignore]`d until the fix lands; what runs here
		// are their controls (the same reads at the root, which are gated) and the one
		// case the view composition already covers. Creates and drops its own schema.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test graphql_nested_type_gates_e2e_pg -- --test-threads=1",
		// #1579: a type scoped by a native `inject_params` column it publishes is embedded
		// (a read, a write's result, a relation filter) and filtered on its document key;
		// one that does not publish it is still refused. Creates and drops its own schema.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test nested_inject_document_key_e2e_pg -- --test-threads=1",
		// #1271: a declared field name and the stored JSONB key it reads are two
		// different strings. The REST runner reads the whole `data` document and
		// projects in Rust, and that projector took the declared name verbatim
		// while the SQL projection generator and the `where` parser snake_case it
		// — so every multi-word camelCase field was absent from a REST 200 while
		// GraphQL served it correctly. Asserts REST against GraphQL on one seeded
		// row, which is the only way to see a key that is simply not there.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_declared_name_stored_key_e2e_pg -- --test-threads=1",
		// #1268: the three export representations accepted a `?select=` naming an embed
		// or a count, validated it, and emitted rows without it — NDJSON dropped the key,
		// CSV and XLSX carried a column named after the relationship that was empty on
		// every row. Needs `export-csv` and `export-xlsx` (in serverLegFeatures): the
		// binary refuses to compile without them rather than dropping those cases.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_export_embedding_e2e_pg -- --test-threads=1",
		// #811/#917: exports paginated through `variables`, which the executor ignores —
		// so every export either truncated to one page or looped forever emitting
		// duplicates. Needs the export features compiled in: the CSV and XLSX cases are
		// `#[cfg]`-gated, and without `export-csv`/`export-xlsx` they would silently not run.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_export_integrity_e2e_pg -- --test-threads=1",
		// #2160: a streamed read served its rows as the database produced them, never
		// completed: a missing non-null value, an enum value outside its members, a value of
		// another kind than its published type. Each row is now completed as the buffered
		// read completes it, and a refused row ends the stream with that read's message.
		// Needs `export-csv` and `export-xlsx` (in serverLegFeatures, required-features).
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_stream_completion_e2e_pg -- --test-threads=1",
		// #1284: `?search=` with no `?sort=` answered 400 on every representation —
		// the implicit relevance ordering emitted a shape no consumer parses, and no
		// `ts_rank` existed to signal. The handler-level half runs on the required
		// test leg; whether the SQL it builds actually ORDERS rows is a question only
		// a database answers, and it is the assertion that separates ranking from
		// merely no longer refusing.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_search_relevance_e2e_pg -- --test-threads=1",
		// #1303: an offset page is a slice of a sequence, and an unordered read is
		// not a sequence — two pages of the same relation overlap and skip rows
		// under a 200. Which clause reaches the adapter is pinned on the required
		// test leg; three things need a database: that `ORDER BY pk_p18doc ASC` (the
		// native-column branch) executes at all, that a three-page walk returns every
		// row exactly once, and that a view's own ORDER BY survives the declared
		// `pagination_order = "none"` opt-out — the one case this change would
		// otherwise make worse.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_pagination_stability_e2e_pg -- --test-threads=1",
		// #1306: `[validation] max_offset` refuses a deep offset before any statement, on
		// GraphQL, REST and an embedded level. Owns its p1306_offset schema → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test offset_ceiling_e2e_pg -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test engine_refusal_rendering_e2e_pg -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test response_bytes_override_e2e_pg -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rich_scalar_serving_e2e_pg -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test constraint_violation_identifier_e2e_pg -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test mutation_success_fields_e2e_pg -- --test-threads=1",
		// #1550: a tenant's REST request and function write run on its own executor
		// (schema mode, public decoys).
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test tenant_rest_dispatch_e2e_pg -- --test-threads=1",
		// #1700: a function's run_as.user_id reaches a mutation's UUID `jwt:sub` inject
		// through the fraiseql_query bridge. Owns its p1700 schema → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test function_run_as_user_id_e2e_pg -- --test-threads=1",
		// #1989: a function's and a source's run_as.static_enriched answer session variables
		// mapped from the enriched identity, through the bridge. Owns its p1989 schema → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test run_as_static_enriched_e2e_pg -- --test-threads=1",
		// #2347: the storage API dispatches to the caller's tenant on every route, so a
		// suspended or unregistered tenant is refused there too. Owns tenant_p2347* → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test storage_tenant_dispatch_e2e_pg -- --test-threads=1",
		// #1844: every transport refuses the same five requests the same way, and every
		// transport the server mounts has a row. Owns tenant_p1844* and p1844rls → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test transport_refusal_matrix_e2e_pg -- --test-threads=1",
		// #633: a schema-mode tenant over its storage quota is refused writes until it is back under.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test tenant_storage_quota_e2e_pg -- --test-threads=1",
		// #1391: a cascade derived from pg_tviews' affected set, against a real pg_tviews
		// (pgTviewsService); its runtime-absent case reads DATABASE_URL, which has none.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test tviews_cascade_e2e_pg -- --test-threads=1",
		// #1575: a write evicts the cached reads of views embedding what it wrote (the graph
		// from pg_tviews' registry and pg_depend), and every type a derived cascade reports.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test cache_embedding_eviction_e2e_pg -- --test-threads=1",
		// #1576: a cached read of one TVIEW row by id survives writes to other rows and is
		// evicted by every write that changes its row, as pg_tviews reports it.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test cache_by_id_eviction_e2e_pg -- --test-threads=1",
		// #1589: pg_tviews' change feed evicts what a write this server did not run made
		// stale, and composed lists by collection. Fails, rather than skips, on a pg_tviews
		// without tviews.registry.typename.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test cache_tviews_feed_e2e_pg -- --test-threads=1",
		// #1587: compile --database proves a relationship's native join from pg_tviews'
		// registry, and the REST composed read correlates on it.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rest_embedding_native_join_e2e_pg -- --test-threads=1",
		// #1887: a GraphQL nested level joined through its relationship correlates on the
		// native join, read back from pg_stat_statements (preloaded by this image) and
		// EXPLAINed. Owns its p1887 schema and resets pg_stat_statements → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test graphql_nested_native_join_e2e_pg -- --test-threads=1",
		// #1614: a null item at a `[T!]` filter operand is refused over REST (it read as
		// `NOT IN (NULL, …)`, no rows, under a 200). Owns its p1614_items schema → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test null_list_items_rest_e2e_pg -- --test-threads=1",
		// #1577: with `cache_cross_instance = "notify"`, a write through one server evicts on
		// another over the same database, and a server that cannot hear the others (feed
		// unreachable, severed, or receiving nothing) caches nothing. DATABASE_URL only.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test cache_cross_instance_e2e_pg -- --test-threads=1",
		// #1391/#1392, the CLI side: compile --database with and without pg_tviews, and
		// doctor's pg_tviews health (both databases are bound in this leg).
		"cargo test -p fraiseql-cli --features test-postgres --test cascade_source_compile_test -- --test-threads=1",
		"cargo test -p fraiseql-cli --features test-postgres --test doctor_pg_tviews_pg -- --test-threads=1",
		// #1571: compile --database advises the pg_tviews `indexes` option for the document
		// keys a TVIEW query filters on, and FraiseQL's reads are served by the index it declares.
		"cargo test -p fraiseql-cli --features test-postgres --test tview_index_advice_pg -- --test-threads=1",
		// #1815/#1818: compile --database advises a document date on every relation by what it
		// is, recognising a TVIEW from pg_tviews' catalog. Its cases are #[ignore]d so the DB-less
		// test leg cannot pass them; here both databases are bound and a missing one fails.
		// --include-ignored, not --ignored: its document guard is not ignored (#1945).
		"cargo test -p fraiseql-cli --features test-postgres --test temporal_key_advice_pg -- --include-ignored --test-threads=1",
		// #1894: a webhook handler that writes a table a TVIEW reads commits at every isolation
		// level the pipeline offers (pg_tviews refuses such a write above READ COMMITTED). Needs
		// pgTviewsService, so it runs here rather than beside inbound_pipeline_pg.
		"cargo test -p fraiseql-webhooks --test webhook_tviews_isolation_pg -- --test-threads=1",
		// #809: schema-per-tenant isolation was a single session `SET search_path` on
		// one pooled connection. Every other connection resolved against `public`, so
		// the leak is only visible under concurrency — a single-connection test passes
		// against the broken code, which is why it shipped.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test tenant_schema_isolation_e2e_pg -- --test-threads=1",
		// #859: DELETE /admin/tenants answered "removed" while every row survived, and
		// `destroy_tenant_schema` had no callers. Needs real DDL against a real database.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test tenant_lifecycle_e2e_pg -- --test-threads=1",
		// #628: the shipped multi-tenant examples now carry real RLS + session-variable
		// wiring. This applies their SQL, compiles them through the real compile path,
		// and asserts two tenants never cross — connecting as the examples' own
		// unprivileged role, because the harness role bypasses RLS entirely.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test example_multitenant_rls_e2e_pg -- --test-threads=1",
		// #1512/#1513: the localized-catalog example, Python-authored, compiled and served:
		// labels per request locale and per tenant's stored locale, a merged localized
		// write under the example's write-session guard, and its projection catalog check.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test example_localized_catalog_e2e_pg -- --test-threads=1",
		// #748/#769/#768: the RBAC management API had never executed one statement
		// against PostgreSQL — its schema DDL did not parse, so setting `admin_token`
		// made the shipped -full binary refuse to boot. Its four test files were ~90
		// empty-bodied `#[test]` functions, which is why `cargo test` was green
		// throughout. Needs a real database (the subject is SQL PostgreSQL either
		// accepts or rejects) and a real boot (the DDL runs in the serve prologue).
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test rbac_admin_e2e_pg -- --test-threads=1",
		// #749: five Studio admin write endpoints answered `{"success": true}` having
		// performed no side effect, and six reads answered a hard-coded empty
		// collection. The corpus derives its route list from the router source at
		// compile time, so the next handler that drifts from the 501 convention fails
		// here rather than shipping.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test studio_admin_no_fabricated_success_e2e -- --test-threads=1",
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
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test api_key_postgres_e2e_pg -- --test-threads=1",
		// #934 service accounts behind the auth layer: the bearer-less x-api-key request
		// must reach the handler's ADR-0018 seam, while a credential-less request, an
		// invalid bearer and an unmatched secret all still 401 at the layer.
		"echo '### cargo test -p fraiseql-server --test service_account_auth_layers_e2e_pg (#934, #1650)'",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test service_account_auth_layers_e2e_pg -- --test-threads=1",
		// #1112 token revocation on every bearer the server authenticates: an auth layer
		// that never consulted the store left a configured [security.token_revocation]
		// inert, and Studio's "revoke all sessions" reported success over tokens that kept
		// working. Both revocation shapes (single jti, revoke-all epoch) plus the accepting
		// half and require_jti, through the real mount.
		"echo '### cargo test -p fraiseql-server --test bearer_revocation_e2e_pg (#1112)'",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test bearer_revocation_e2e_pg -- --test-threads=1",
		// #368 P26 — social login through the shipped mount, against a stub IdP:
		// Google OIDC full loop, the GitHub /user/emails second hop (email-keyed
		// linking), the auth_start path bucket on /auth/v1/authorize (#788), and
		// state/provider refusals.
		"echo '### cargo test -p fraiseql-server --test social_oauth_e2e_pg (#368 social login mount)'",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test social_oauth_e2e_pg -- --test-threads=1",
		// #367 P26 — the [auth.local] reachability tier through the shipped mount:
		// password signup/login/reset, OTP whose identity resolves through the
		// account store, MFA whose Postgres enrollment survives a server restart,
		// anonymous signup, and the disabled-method-is-absent invariant.
		"echo '### cargo test -p fraiseql-server --test local_auth_e2e_pg (#367 local auth mount)'",
		// `inbound-email` is in serverLegFeatures for this suite: the password/OTP tests
		// need the SMTP transport, and without it `[auth.local]` refuses to boot (by
		// design), so they are feature-gated and would silently vanish otherwise.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test local_auth_e2e_pg -- --test-threads=1",
		// #1853 — [auth.session]: access-token lifetime on every sign-in path, refresh
		// rotation with its 10-second reuse window, the absolute limit, logout, the
		// session routes' rate limits and the sweep. One test sleeps ~2 minutes past a
		// real expiry to pin that the first-party issuer's leeway is fixed.
		"echo '### cargo test -p fraiseql-server --test session_refresh_e2e_pg (#1853 session lifetime)'",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test session_refresh_e2e_pg -- --test-threads=1",
		// #1965 — [auth.database]: auth on a role of its own (AUTH_DATABASE_URL), the engine
		// on a role that cannot read schema `auth`, the boot refusals, the boot logs, and a
		// schema `auth` FraiseQL did not create.
		"echo '### cargo test -p fraiseql-server --test auth_database_e2e_pg (#1965 auth role and pool)'",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test auth_database_e2e_pg -- --test-threads=1",
		// #2098 — the application pool on a DEFAULT-features build, the shape the default
		// image ships: SCIM, the RBAC API and tenant admin tokens mounted through the
		// binary's own boot path. Its own line, with no `--features`, on purpose:
		// serverLegFeatures includes `observers`, under which the binary always had a
		// pool, so every suite above passed while the default build served none of these.
		"echo '### cargo test -p fraiseql-server --test default_build_pools_e2e_pg (#2098 default build, application pool)'",
		"cargo test -p fraiseql-server --test default_build_pools_e2e_pg -- --test-threads=1",
		// The same suite's one `observers` test: the binary opens two distinct pools.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test default_build_pools_e2e_pg -- --test-threads=1 two_distinct_pools",
		// #1964 — [auth_issuer]: the server signs its sessions with ES256 keys of its own,
		// publishes them, rotates them, and admits an external provider's tokens on
		// /graphql but not on the routes acting on the caller's own account.
		"echo '### cargo test -p fraiseql-server --test first_party_issuer_e2e_pg (#1964 first-party issuer)'",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test first_party_issuer_e2e_pg -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test mcp_tenant_dispatch_e2e_pg -- --test-threads=1",
		// A function a tenant's write triggers runs within that tenant. File-level
		// `#![cfg(feature = "functions-runtime-deno")]`: without the feature the binary is
		// empty and reports "0 passed".
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test function_tenant_dispatch_e2e_pg -- --test-threads=1",
		// #1628: an after:mutation / after:capture dispatch the server was retrying when it
		// was killed runs again after a restart, with its token, exactly once more. Each test
		// re-executes this binary as its server process (one V8 isolate each) → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test function_dispatch_restart_e2e_pg -- --test-threads=1",
		// #1628: a role that may not create the dispatch ledger stops the boot, and the error
		// names the table and the CREATE privilege. Owns a scratch database and role → serial.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test function_dispatch_boot_pg -- --test-threads=1",
		// pipeline_e2e is env-gated (FRAISEQL_PIPELINE_E2E); it compiles a schema and drives a server.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test pipeline_e2e_test -- --test-threads=1",
		// P18 verification gate (PG half): the FULL subscription delivery path —
		// tb_entity_change_log row → observer runtime loop → EventBridge → manager
		// → real WebSocket client. Proves a CUSTOM/Debezium-'r' snapshot row is
		// never delivered as a phantom create (#773) and a burst beyond the bridge
		// capacity is delivered completely (#772). --test-threads=1: the tests
		// share the observer schema DDL.
		"echo '### P18: subscription pipeline e2e (change log → runtime → bridge → /ws)'",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test subscription_pipeline_e2e_pg -- --include-ignored --test-threads=1",
		// #992: DB-needing binaries that ran in NO leg — each self-skips without
		// DATABASE_URL, and only the DB-less test leg compiled them.
		"echo '### cargo test -p fraiseql-server DB-backed strays (#992)'",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test revocation_pg_test -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test sql_source_boot_check -- --test-threads=1",
		// #1558: FRAISEQL_POOL_MAX_SIZE sizes the pool the server opens.
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test pool_size_pg -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test storage_wiring_test -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test tenant_provisioning_test -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test test_helpers -- --test-threads=1",
		"cargo test -p fraiseql-server " + serverLegFeatures + " --test observer_test_helpers -- --test-threads=1",
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

	return m.integrationBase(source, rustMsrv, lane).
		WithServiceBinding(pgBindHost, primary).
		WithServiceBinding(pgStandbyBindHost, m.pgStandbyService(source, primary, "fraiseql_standby")).
		WithServiceBinding(pgTviewsBindHost, m.pgTviewsService(source)).
		WithServiceBinding(pgTviewsUnpreloadedBindHost, m.pgTviewsUnpreloadedService(source)).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable(requirePostgresEnv, "1").
		WithEnvVariable(requireAllFeaturesEnv, "1").
		// Auth's own role, which the auth_database suite boots auth on.
		WithEnvVariable("AUTH_DATABASE_URL", authURL).
		WithEnvVariable("STANDBY_DATABASE_URL", standbyURL).
		WithEnvVariable("TVIEWS_DATABASE_URL", tviewsURL).
		WithEnvVariable("TVIEWS_UNPRELOADED_DATABASE_URL", tviewsUnpreloadedURL).
		WithEnvVariable(requireTviewsUnpreloadedEnv, "1").
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
func (m *FraiseqlCi) integrationWire(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
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
	// #1115: the server's wire backend (`FraiseWireAdapter` in fraiseql-db) against the same
	// SCRAM database: reads, the session-variable refusal, and its streaming read.
	lines = append(lines, "cargo test -p fraiseql-db --features wire-backend,postgres --test wire_adapter_pg -- --test-threads=1")
	lines = append(lines, "echo 'test-integration OK: wire suite passed'")
	script := strings.Join(lines, "\n")

	return m.integrationBase(source, rustMsrv, lane).
		WithServiceBinding(wireBindHost, m.wirePgService()).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable(requirePostgresEnv, "1").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// wirePgService is a pgImage Postgres with SCRAM-SHA-256 forced on (matching the auth
// config the old wire testcontainer used). It is otherwise blank: the wire test
// helper creates the `test` schema and seeds it on first connect (idempotent +
// seed-if-empty), so no initdb fixtures are mounted.
// pgBelowFloorService is a bare PostgreSQL 17, which exists to be refused: the
// #1452 floor suites connect to it and assert the version refusal.
func (m *FraiseqlCi) pgBelowFloorService() *dagger.Service {
	return dag.Container().
		From(pgBelowFloorImage).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithExposedPort(5432).
		AsService()
}

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
func (m *FraiseqlCi) integrationStorage(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
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

	// nodejs/npm for the tus-js-client interop suite, fonts-dejavu-core for #973's text
	// watermark (whose typeface is the operator's and is therefore never vendored into
	// the crate). Installed for this leg rather than in rustBase: only this leg needs
	// them, and rustBase is shared by every heavy leg.
	return m.integrationBaseWith(source, rustMsrv, lane, "nodejs", "npm", "fonts-dejavu-core").
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithServiceBinding(azuriteBindHost, m.azuriteService()).
		WithServiceBinding(fakeGcsBindHost, m.fakeGcsService()).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable(requirePostgresEnv, "1").
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
func (m *FraiseqlCi) integrationServerStorage(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
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

	return m.integrationBase(source, rustMsrv, lane).
		WithServiceBinding(minioBindHost, m.minioService()).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithEnvVariable("MINIO_ENDPOINT", minioEndpoint).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable(requirePostgresEnv, "1").
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
func (m *FraiseqlCi) integrationFederation(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	usersURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, fedUsersBindHost, pgDatabase)
	routerURL := fmt.Sprintf("http://%s:4000", apolloRouterBindHost)
	subgraphAURL := fmt.Sprintf("http://%s:8815", fedSubgraphABindHost)
	subgraphBURL := fmt.Sprintf("http://%s:8816", fedSubgraphBBindHost)

	binary := m.fedServerBinary(source, lane)
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

	return m.fedBase(source, lane).
		WithServiceBinding(fedUsersBindHost, pgUsers).
		WithServiceBinding(fedSubgraphABindHost, subgraphA).
		WithServiceBinding(fedSubgraphBBindHost, subgraphB).
		WithServiceBinding(apolloRouterBindHost, router).
		WithEnvVariable("DATABASE_URL", usersURL).
		WithEnvVariable(requirePostgresEnv, "1").
		WithEnvVariable("FEDERATION_TESTS", "1").
		WithEnvVariable("ROUTER_URL", routerURL).
		WithEnvVariable("SUBGRAPH_A_URL", subgraphAURL).
		WithEnvVariable("SUBGRAPH_B_URL", subgraphBURL).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// fedBase mounts the source on a dedicated federation-feature target cache volume.
func (m *FraiseqlCi) fedBase(source *dagger.Directory, lane int) *dagger.Container {
	return rustSource(m.rustBaseFor(rustMsrv), source, laneVolume(fedTargetVol, lane)).
		WithEnvVariable("RUST_LOG", "debug")
}

// fedServerBinary builds the fraiseql-server binary with the federation feature and
// returns it as a File (extracted from the cache-mounted target dir to a plain path).
//
// It builds through the suite's own test invocation, which links the package's bins
// too, so the binary and the test share one dependency graph. A `cargo build` of the
// bin leaves the dev-dependencies' features out and resolves 377 of the test's 698
// units differently; on a cold volume the leg compiled both graphs (#1890).
func (m *FraiseqlCi) fedServerBinary(source *dagger.Directory, lane int) *dagger.File {
	built := rustSource(m.rustBaseFor(rustMsrv), source, laneVolume(fedTargetVol, lane)).
		WithExec([]string{
			"bash", "-c",
			"bash tools/ci-target-canary.sh -- test -p fraiseql-server --features federation --test federation_integration_test && cp target/debug/fraiseql-server /usr/local/bin/fraiseql-server",
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

// fedPgService returns a pgImage Postgres seeded with a federation fixture
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
// suites: fraiseql-core APQ storage and fraiseql-observers cache/lease. Those lib
// tests are #[ignore]d ("requires Redis running") and read REDIS_URL / DATABASE_URL.
func (m *FraiseqlCi) integrationRedis(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	redisURL := fmt.Sprintf("redis://%s:6379", redisBindHost)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: redis (core APQ + observers lease + #428 cache-invalidation) — Dagger-bound redis+postgres'",
		// The filter names the module rather than the substring `redis` (#1297):
		// `apq::tests::redis_storage_tests` is the target the coverage gate scores,
		// and a filter longer than a module path is deliberately not credited to
		// it. Staying specific also keeps `cache::tests`' manual scaling benchmark
		// — the crate's other #[ignore]d lib test — out of this leg.
		"bash tools/ci-target-canary.sh -- test -p fraiseql-core --features redis-apq --lib apq::tests::redis_storage_tests", // #880 canary
		"cargo test -p fraiseql-core --features redis-apq --lib apq::tests::redis_storage_tests -- --ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --lib -- --ignored --test-threads=1",
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
		// The Redis URL of PKCE, the rate limiter and token revocation: one holding a
		// password reaches the bound Redis, as written and through redis_url_env, and the
		// startup log names its host only.
		"cargo test -p fraiseql-server --features 'redis-pkce,redis-rate-limiting' --lib server::initialization::tests -- --ignored --test-threads=1",
		// #992: Redis-needing binaries that ran in NO leg (each self-skips
		// without REDIS_URL, and only service-less legs compiled them).
		"echo '### redis-backed strays (#992)'",
		// --include-ignored: the redis-touching halves are #[ignore]d and this is
		// the leg that binds the Redis they need.
		"cargo test -p fraiseql-server --features redis-apq --test redis_apq_integration_test -- --include-ignored --test-threads=1",
		// redis-rate-limiting: the whole binary is #![cfg]-gated on it and runs
		// ZERO tests without (verified).
		"cargo test -p fraiseql-auth --features redis-rate-limiting --test redis_failover_test -- --include-ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features 'caching,redis-lease,testing' --test integration_test -- --test-threads=1",
		// Unfiltered since #1297. Under `--features redis-pkce` the only #[ignore]d
		// lib tests in this crate are the four PKCE ones — the three state-store
		// cases below are gated on `redis-rate-limiting`, which this build does not
		// enable — so the set run is unchanged, and dropping the filter is what
		// lets the coverage gate see that `tests[redis-pkce]` is covered at all.
		"cargo test -p fraiseql-auth --features redis-pkce --lib -- --ignored --test-threads=1",
		// #1295: the RedisStateStore single-use/replay/multi-state cases. They
		// hardcoded `redis://localhost:6379` and treated a failed connection as a
		// skip, so they had asserted nothing in ANY leg — this one binds Redis
		// under a service alias, never on localhost — while a developer box that
		// did run one could abort the whole `set -e` leg on a slow response. They
		// now read REDIS_URL and are #[ignore]d, so this is the only line that
		// runs them.
		//
		// Unfiltered since #1297, for the same reason as the PKCE line: under
		// `--features redis-rate-limiting` these three are the crate's only
		// #[ignore]d lib tests, so `-- --ignored` runs exactly them, and a filter
		// longer than the `tests` module path could never be credited as covering
		// it. It also retires the load-bearing filter this comment used to warn
		// about — a rename would have made the line print `ok. 0 passed`.
		"cargo test -p fraiseql-auth --features redis-rate-limiting --lib -- --ignored --test-threads=1",
		"echo 'test-integration OK: redis suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv, lane).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithServiceBinding(redisBindHost, m.redisService()).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable(requirePostgresEnv, "1").
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
func (m *FraiseqlCi) integrationVault(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	vaultAddr := fmt.Sprintf("http://%s:8200", vaultBindHost)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: vault secrets manager (Dagger-bound vault dev)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-server --features secrets --test secrets_manager_integration_test", // #880 canary
		"cargo test -p fraiseql-server --features secrets --test secrets_manager_integration_test -- --ignored --test-threads=1",
		"echo 'test-integration OK: vault suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv, lane).
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
func (m *FraiseqlCi) integrationTLS(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
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
		// #1429: the CLI's connections negotiate TLS through the same connector, honour the
		// URL's sslmode and, where a command loaded fraiseql.toml, its [database] ssl_mode.
		"echo '### integration: tls (fraiseql-cli database connections)'",
		"cargo test -p fraiseql-cli --features test-postgres --test database_tls_pg -- --test-threads=1",
		"echo 'test-integration OK: tls suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv, lane).
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

// greenmailCerts generates the test mail server's PKI per run: a CA, and a server
// certificate it signs for greenmailBindHost, as a PKCS12 keystore GreenMail loads. The
// fixture trusts the CA through `[mailbox.<name>.imap] ca_file`; nothing is committed.
// The keystore is world-readable: GreenMail does not run as root, and a keystore it
// cannot read leaves IMAPS unstarted and every handshake reset.
func (m *FraiseqlCi) greenmailCerts() *dagger.Directory {
	gen := strings.Join([]string{
		"set -e",
		"mkdir -p /out && cd /out",
		"openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -keyout ca.key -out ca.crt -days 2 -nodes" +
			" -subj '/CN=fraiseql-greenmail-test-ca'" +
			" -addext 'basicConstraints=critical,CA:TRUE' -addext 'keyUsage=critical,keyCertSign'",
		"openssl req -newkey ec -pkeyopt ec_paramgen_curve:P-256 -keyout server.key -out server.csr -nodes -subj '/CN=" + greenmailBindHost + "'",
		"openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out server.crt -days 2" +
			" -extfile <(printf 'subjectAltName=DNS:" + greenmailBindHost + ",DNS:localhost,IP:127.0.0.1\\nbasicConstraints=critical,CA:FALSE\\nextendedKeyUsage=serverAuth')",
		"openssl pkcs12 -export -in server.crt -inkey server.key -out greenmail.p12 -passout pass:greenmail -name greenmail",
		"chmod 644 ca.crt greenmail.p12",
	}, "\n")

	return dag.Container().
		From(pgImage). // the postgres image ships openssl
		WithExec([]string{"bash", "-c", gen}).
		Directory("/out")
}

// greenmailService is GreenMail over the keystore greenmailCerts made: IMAPS on 3993,
// SMTP on 3025, and the one user the inbound_email fixture logs in as.
func (m *FraiseqlCi) greenmailService(certs *dagger.Directory) *dagger.Service {
	return dag.Container().
		From(greenmailImage).
		WithFile("/tls/greenmail.p12", certs.File("greenmail.p12")).
		WithEnvVariable("GREENMAIL_OPTS", strings.Join([]string{
			"-Dgreenmail.setup.test.all",
			"-Dgreenmail.hostname=0.0.0.0",
			"-Dgreenmail.users=fixture:fixture@example.test",
			"-Dgreenmail.tls.keystore.file=/tls/greenmail.p12",
			"-Dgreenmail.tls.keystore.password=greenmail",
		}, " ")).
		WithExposedPort(3993).
		WithExposedPort(3025).
		AsService()
}

// tlsPgService is a pgImage Postgres that enables TLS using the pre-generated server cert.
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
func (m *FraiseqlCi) integrationHTTPE2e(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	server := m.serverE2eService(source, lane)
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

	return m.integrationBase(source, rustMsrv, lane).
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
func (m *FraiseqlCi) serverE2eService(source *dagger.Directory, lane int) *dagger.Service {
	targetVol := laneVolume(serverE2eTargetVol, lane)
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	// Build the binary and copy it out of the (cache-mounted) target dir to a plain
	// path so it can be extracted as a File into the runtime service container.
	built := rustSource(m.rustBaseFor(rustMsrv), source, targetVol).
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

// pgE2eService is a pgImage Postgres seeded with the E2E fixture (docker/e2e/
// init-postgres.sql — tb_e2e_user + v_users), distinct from the main integration seed.
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
func (m *FraiseqlCi) integrationObservers(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	redisURL := fmt.Sprintf("redis://%s:6379", redisBindHost)
	natsURL := fmt.Sprintf("nats://%s:4222", natsBindHost)

	// The leg's server features, on fraiseql-server for the `fraiseql-cli` binary the
	// capability fixtures' `run` entry point serves through.
	capabilityRunServerFeatures := "run-server"
	for _, f := range strings.Split(serverUnifiedFeatures, ",") {
		capabilityRunServerFeatures += ",fraiseql-server/" + f
	}

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: observers (Dagger-bound postgres+redis+nats)'",
		// postgres_notify lib tests are skip-on-None (not #[ignore]d); run name-filtered
		// (no --ignored) so the NOTIFY transport tests exercise the bound Postgres.
		"bash tools/ci-target-canary.sh -- test -p fraiseql-observers --features 'caching,redis-lease' --lib postgres_notify", // #880 canary
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --lib postgres_notify -- --test-threads=1",
		// The crate's `#[ignore]`d lib tests, 14 against the bound Postgres:
		// `listener::replay`'s and the `database` action's. Not a plain `--lib` run:
		// that pulls in the SSRF-guard unit tests, which assert the guard is ON and so
		// fail under this suite's FRAISEQL_OBSERVERS_ALLOW_INSECURE=true.
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --lib -- --ignored --test-threads=1",
		// #1297: the four advisory-lease tests are skip-on-None, not `#[ignore]`d, so
		// the `--ignored` line above runs none of them; a
		// NAME-FILTERED plain run reaches them without pulling in the SSRF-guard
		// unit tests that fail under this suite's FRAISEQL_OBSERVERS_ALLOW_INSECURE.
		// This leg binds both Postgres and Redis, so both halves execute here.
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --lib listener::tests -- --test-threads=1",
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test observer_runtime_integration_test -- --ignored --test-threads=1",
		// #928: the observer E2E suite. It had never run in any leg — none of its
		// tests constructed a runtime, so all 8 waited for webhooks nothing could
		// send. Repaired in P17 (each test drives a real ObserverRuntime); gated
		// here so it can never silently die again.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test observer_e2e_test -- --ignored --test-threads=1",
		// #2040: a webhook header value read from the environment at delivery, end to end
		// through the compiler, the admin API and a real runtime. It needs this leg's
		// private-webhook allowance (FRAISEQL_ALLOW_PRIVATE_WEBHOOKS) for the loopback
		// receiver; `auth` for the admin principal.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test observer_headers_env_e2e_pg -- --test-threads=1",
		// #1309: the REST /{resource}/stream fan-out, and the one assertion the issue
		// made mandatory — open a stream AND assert the observer still fires for the
		// same event. It lives in THIS leg rather than `integration (server)` because
		// it needs both halves: PostgreSQL for the change log, and the outbound bypass
		// (FRAISEQL_ALLOW_PRIVATE_WEBHOOKS / FRAISEQL_OBSERVERS_ALLOW_INSECURE, set
		// above) for the observer's webhook to reach the wiremock server. The server
		// leg has the database and not the bypass, so the observer half would have had
		// to assert on a log row written by a REFUSED dispatch — an assertion about the
		// SSRF guard rather than about the fan-out.
		"echo '### #1309: REST /{resource}/stream fan-out (stream delivers AND observers still fire)'",
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test rest_stream_fanout_e2e_pg -- --include-ignored --test-threads=1",
		// #349 email happy-path: send through lettre to the bound MailHog sink and
		// assert the message arrived (real SMTP wire format, not a stub).
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test smtp_integration -- --ignored --test-threads=1",
		// #443 / #437 F6: change-log RLS isolation under a NOBYPASSRLS role (the
		// superuser DATABASE_URL would mask the policy). The test creates its own
		// tenant/consumer roles off the superuser connection — no extra env needed.
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test rls_isolation -- --ignored --test-threads=1",
		// #390 (P29): the change-log contract suite — including the actor_type CHECK
		// constraint tests — plus the views + capture-trigger suites. All three binaries
		// had run in NO leg since they were written (the #[ignore]-with-no---ignored-leg
		// form of the skip-green pattern); capture_trigger is order-independent since it
		// drops the ambient table (an ambient NOT-NULL object_data red-flagged it).
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test entity_change_log_contract -- --ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test changelog_views -- --ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test capture_trigger -- --ignored --test-threads=1",
		// #935: the two-session commit-order repro. Identity pks are allocated in
		// statement order but become visible in commit order, so the old
		// `pk > watermark` cursor permanently skipped late-committing rows and
		// their observers never fired. Needs a real Postgres — a mock cannot
		// produce the divergence — and its own binary, since it owns
		// core.tb_entity_change_log and the dispatch ledger.
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test change_log_commit_order_pg -- --ignored --test-threads=1",
		// A change-log row a reader cannot read is reported and passed over, and the
		// rows after it are delivered: by the poller (whose loop gave up after ten
		// failed polls), the per-process tail and a resume. Owns the change log too.
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test change_log_unreadable_row_pg -- --ignored --test-threads=1",
		// A connection handed to the change-log tail inside a transaction is refused and
		// reported by backend, with no other session's statement text. Owns the change log.
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test change_log_tail_dirty_connection_pg -- --ignored --test-threads=1",
		// A transaction start cancelled while its BEGIN's reply is held leaves no pooled
		// connection inside a transaction. Owns the change log.
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test cancelled_transaction_start_pg -- --ignored --test-threads=1",
		// #573 source coordination primitives: the advisory-lease mutual-exclusion
		// + crash-safety boundary (Phase 00), and the source cursor store (CAS
		// advance, in-tx rollback, deny-by-default RLS) + single-firing runner
		// (Phase 01), all vs the bound Postgres. Self-skip on no DATABASE_URL (not
		// #[ignore]d) → run WITHOUT --ignored, as separate binaries so they never
		// pull in the SSRF-guard unit tests this suite disables.
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test advisory_lease_pg -- --test-threads=1",
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test source_cursor_pg -- --test-threads=1",
		// A second boot of each observer script over a current schema waits on no lock a live
		// write holds, and a boot puts back what each guard skips on. Each test owns a database.
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test boot_locks_pg",
		// The DDL the change-log listener runs on its first poll, raced by eight listeners.
		"cargo test -p fraiseql-observers --features 'caching,redis-lease' --test ddl_lock_pg -- --test-threads=1",
		// #382 outbound CDC: the drain-worker state machine vs the bound Postgres
		// (stub sink, no broker) + the NATS JetStream sink end-to-end vs the bound
		// JetStream (DATABASE_URL + NATS_URL + FRAISEQL_NATS_ALLOW_PLAINTEXT below).
		"cargo test -p fraiseql-cdc-sinks --features 'cdc-nats-jetstream,cdc-kafka,cdc-kinesis' --test cdc_drain_pg -- --ignored --test-threads=1",
		"cargo test -p fraiseql-cdc-sinks --features 'cdc-nats-jetstream,cdc-kafka,cdc-kinesis' --test cdc_nats_e2e -- --ignored --test-threads=1",
		// #975: the same drain-through assertions against the bound Kafka broker
		// (DATABASE_URL + KAFKA_BOOTSTRAP + FRAISEQL_KAFKA_ALLOW_PLAINTEXT below).
		// Asserted THROUGH the drain, and the entity-keyed partition affinity is
		// only meaningful because the suite creates its topic with 6 partitions.
		"cargo test -p fraiseql-cdc-sinks --features 'cdc-nats-jetstream,cdc-kafka,cdc-kinesis' --test cdc_kafka_e2e -- --ignored --test-threads=1",
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
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test subscription_kafka_mirror_e2e -- --test-threads=1",
		// #975: the same drain-through assertions against the bound LocalStack
		// Kinesis (DATABASE_URL + KINESIS_ENDPOINT + FRAISEQL_KINESIS_ALLOW_PLAINTEXT
		// below). The entity-keyed shard affinity is only meaningful because the
		// suite creates its stream with 4 shards — on a single-shard stream the
		// assertion passes with any partition key at all, including a per-message one.
		"cargo test -p fraiseql-cdc-sinks --features 'cdc-nats-jetstream,cdc-kafka,cdc-kinesis' --test cdc_kinesis_e2e -- --ignored --test-threads=1",
		// #382: the SERVER's outbound-CDC mount — [cdc_outbound] built through
		// the real path drains the outbox to the bound JetStream, and an
		// unreachable broker refuses to boot. Before this the drain engine was
		// library-only: nothing in the shipped server constructed a DrainWorker.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test cdc_outbound_mount_pg -- --ignored --test-threads=1",
		// 04c: every capability fixture boots the `fraiseql-server` binary (`cli`) on a database
		// of its own and must reach its effect. It runs here because this leg binds every
		// service a fixture names (NATS for CDC). serverUnifiedFeatures holds every Stable capability.
		//
		// The `run` entry point is `fraiseql run` from the `fraiseql-cli` binary, built here
		// with `run-server` and the same server features, into this leg's target directory.
		// The suite cannot build it: `run-server` depends on fraiseql-server with its default
		// features, and as its dev-dependency that turned them on in every test build of the
		// crate (#2096). Unset, every fixture driven through `run` fails, naming the variable.
		"cargo build -p fraiseql-cli --features '" + capabilityRunServerFeatures + "'",
		"export FRAISEQL_CAPABILITY_RUN_BIN=\"${CARGO_TARGET_DIR:-$PWD/target}/debug/fraiseql-cli\"",
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test capability_fixtures_e2e_pg -- --test-threads=1",
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
		// `arrow` (in serverUnifiedFeatures) on the line: the suite is `#![cfg(feature = "arrow")]`,
		// so a leg built without it would run zero tests while reading green — the
		// #940-class trap.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test flight_upload_outbox_pg -- --ignored --test-threads=1",
		// #1355: the SECOND upload verb. #953 reached DoExchange only, so every DoPut
		// upload committed unrecorded and the Change Spine saw the same rows in the same
		// table appear or not appear depending on which Flight verb wrote them. Asserts on
		// the OUTBOX, never on the target table — a row-count assertion passed throughout
		// the defect. Carries a DoExchange twin driving the same bytes through the verb
		// #953 fixed: it is what makes a DoPut failure a statement about DoPut rather than
		// about the fixture, and if both ever go red it is the harness that broke.
		// `--features arrow` for the same #940-class reason as the line above.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test flight_do_put_outbox_pg -- --ignored --test-threads=1",
		// Flight `OptimizedView` serves exactly the operator's `flight_views` (the registry the
		// library pre-fills is emptied by the server). `--features arrow` named on the line
		// for the same #940-class reason as the two lines above.
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test flight_views_pg -- --ignored --test-threads=1",
		// Flight serves a `numeric` with its digits on every ticket that reads rows (#1766).
		"cargo test -p fraiseql-server --features '" + serverUnifiedFeatures + "' --test flight_decimal_pg -- --ignored --test-threads=1",
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

	// One PKI for the service and the client that trusts it.
	greenmailCerts := m.greenmailCerts()

	return m.integrationBase(source, rustMsrv, lane).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithServiceBinding(redisBindHost, m.redisService()).
		WithServiceBinding(natsBindHost, m.natsService()).
		WithServiceBinding(kafkaBindHost, m.kafkaService()).
		WithServiceBinding(localstackBindHost, m.localstackService()).
		WithServiceBinding(mailhogBindHost, m.mailhogService()).
		WithServiceBinding(greenmailBindHost, m.greenmailService(greenmailCerts)).
		WithFile("/greenmail-ca.crt", greenmailCerts.File("ca.crt")).
		WithEnvVariable("GREENMAIL_HOST", greenmailBindHost).
		WithEnvVariable("GREENMAIL_IMAPS_PORT", "3993").
		WithEnvVariable("GREENMAIL_SMTP_PORT", "3025").
		WithEnvVariable("GREENMAIL_CA", "/greenmail-ca.crt").
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable(requirePostgresEnv, "1").
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
		// The bound JetStream service speaks plaintext nats://;
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
func (m *FraiseqlCi) integrationNats(ctx context.Context, source *dagger.Directory, lane int) (string, error) {
	natsURL := fmt.Sprintf("nats://%s:4222", natsBindHost)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: nats (Dagger-bound JetStream; tests read NATS_URL)'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-observers --features nats --test nats_integration", // #880 canary
		"cargo test -p fraiseql-observers --features nats --test nats_integration -- --ignored --test-threads=1",
		"echo 'test-integration OK: nats suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv, lane).
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
		AsService()
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
		AsService()
}

// mailhogService is the MailHog SMTP sink: SMTP on 1025 (plaintext) and an HTTP
// inspection API on 8025. Used by the #349 email happy-path integration test.
func (m *FraiseqlCi) mailhogService() *dagger.Service {
	return dag.Container().
		From(mailhogImage).
		WithExposedPort(1025).
		WithExposedPort(8025).
		AsService()
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

// pgService returns a started pgImage service seeded with the repo's
// integration fixtures. The two SQL files are mounted into
// /docker-entrypoint-initdb.d under numeric names so the entrypoint runs them in
// load order (init before init-analytics) on first boot. Dagger waits for the
// exposed port before bound callers proceed, so Postgres is accepting connections
// by the time the tests run.
func (m *FraiseqlCi) pgService(source *dagger.Directory) *dagger.Service {
	return m.pgContainer(source).AsService()
}

// pgContainer is pgService's container before it is started, for a caller that needs a
// service of its own: two bindings of one definition are one server.
func (m *FraiseqlCi) pgContainer(source *dagger.Directory) *dagger.Container {
	initDir := dag.Directory().
		WithFile("00-init.sql", source.File("tests/sql/postgres/init.sql")).
		WithFile("01-init-analytics.sql", source.File("tests/sql/postgres/init-analytics.sql")).
		// #957: creates the replication role and opens `pg_hba.conf` to replication
		// connections, so pgStandbyService can base-backup and then stream. The SAME
		// file docker-compose.test.yml mounts — a replication setup that existed on
		// only one rig would make the lag suite green on one and red on the other.
		WithFile("02-replication.sh", source.File("docker/init/postgres-replication-init.sh")).
		// #1965: first-party auth's own role. The same file docker-compose.test.yml mounts.
		WithFile("03-auth-role.sh", source.File("docker/init/postgres-auth-role-init.sh"))

	return dag.Container().
		From(pgImage).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithDirectory("/docker-entrypoint-initdb.d", initDir).
		WithExposedPort(5432)
}

// pgTviewsService returns pgImage with pg_tviews built in from source and preloaded
// (docker/pg-tviews/Dockerfile, which holds the pin), for the pg_tviews suites (#1391).
// pg_tviews ships no binaries; the build is a pgrx compile, cached on the build context
// (that one directory) and the base image.
func (m *FraiseqlCi) pgTviewsService(source *dagger.Directory) *dagger.Service {
	return source.Directory("docker/pg-tviews").
		DockerBuild(dagger.DirectoryDockerBuildOpts{
			BuildArgs: []dagger.BuildArg{{Name: "PG_IMAGE", Value: pgImage}},
		}).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithExposedPort(5432).
		// The Dockerfile's CMD (`postgres -c shared_preload_libraries=…`) becomes the default
		// args, and Dagger runs default args without the base image's entrypoint unless told
		// to: `postgres` then starts as root and refuses ("root" execution … is not
		// permitted). `docker run` always prepends the entrypoint, which is why compose and
		// a local `docker run` never showed it. pgService sets no CMD of its own.
		AsService(dagger.ContainerAsServiceOpts{UseEntrypoint: true})
}

// pgTviewsUnpreloadedService returns pgTviewsService's image with pg_tviews installed and a
// TVIEW created while preloaded, then restarted without it (#1551): pg_tviews refuses to
// install unpreloaded, so this is how an operator reaches the state.
// docker/pg-tviews/unpreloaded-entrypoint.sh does it in one container, no volume, and
// opens the port only once the unpreloaded server is up; tools/pg-tviews-unpreloaded.sh
// runs the same script locally.
func (m *FraiseqlCi) pgTviewsUnpreloadedService(source *dagger.Directory) *dagger.Service {
	return source.Directory("docker/pg-tviews").
		DockerBuild(dagger.DirectoryDockerBuildOpts{
			BuildArgs: []dagger.BuildArg{{Name: "PG_IMAGE", Value: pgImage}},
		}).
		WithFile("/unpreloaded-entrypoint.sh", source.File("docker/pg-tviews/unpreloaded-entrypoint.sh")).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithUser("postgres").
		WithExposedPort(5432).
		AsService(dagger.ContainerAsServiceOpts{
			Args: []string{"/bin/bash", "/unpreloaded-entrypoint.sh"},
		})
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
func (m *FraiseqlCi) integrationBase(source *dagger.Directory, rust string, lane int) *dagger.Container {
	return m.integrationBaseWith(source, rust, lane)
}

// integrationBaseWith is integrationBase with `pkgs` apt-installed on rustBaseFor, BELOW
// the source mount. A layer above the mount is keyed on the source, so an install there
// misses the cache on every push and reaches the package archive on every run. Below
// it, it misses only after an engine GC.
func (m *FraiseqlCi) integrationBaseWith(
	source *dagger.Directory,
	rust string,
	lane int,
	pkgs ...string,
) *dagger.Container {
	toolchain := resolveToolchain(rust)
	base := m.rustBaseFor(toolchain)
	if len(pkgs) > 0 {
		base = base.WithExec(aptInstall(pkgs...))
	}
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
	// A volume bump only clears the current drift. The durable fix is rustSource (#1421):
	// sources stamped at leg start so no earlier run's artifact is judged fresh, and the
	// volume mounted PRIVATE so no concurrent run shares it.
	return rustSource(base, source, integrationTargetVol(toolchain, lane)).
		WithEnvVariable("RUST_LOG", "debug")
}
