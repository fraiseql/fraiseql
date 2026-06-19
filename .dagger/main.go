// Package main is the FraiseQL CI Dagger module.
//
// It hosts the self-hosted CI pipeline that replaces the GitHub-hosted workflows
// (Track 0, see .phases/dagger-adoption/). Phase 01 ports the smallest gate:
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
	// rustImage pins the toolchain to the workspace MSRV (rust-toolchain.toml channel = 1.92).
	// The default (non-slim) variant is buildpack-deps-based, so gcc/perl/curl/git are present.
	// (Later: pin by digest — see parity-notes.md Phase 02.)
	//
	// Pulled from ghcr.io/fraiseql/* (mirrored by .github/workflows/mirror-base-images.yml),
	// NOT Docker Hub: the self-hosted runner shares one Docker Hub account whose pull-rate
	// limit periodically 429s every leg. The ghcr mirrors are public, so the engine pulls
	// them anonymously. Every ghcr.io/fraiseql/* tag below MUST have a matching entry in that
	// workflow's IMAGES list. (mcr.microsoft.com/* and the Apollo router stay as-is — not
	// Docker Hub, not rate-limited.)
	rustImage = "ghcr.io/fraiseql/rust:1.92"
	// ubuntuImage backs shellBase (the toolchain-free shell-gate container).
	ubuntuImage = "ghcr.io/fraiseql/ubuntu:24.04"
	// unwrapAllowLimit mirrors the ci.yml clippy job's `make lint-unwrap UNWRAP_ALLOW_LIMIT=3`.
	unwrapAllowLimit = "3"
	// sccacheVersion pins the prebuilt sccache binary fetched into rustBase.
	sccacheVersion = "v0.8.2"
	// rustMsrv mirrors Cargo.toml workspace rust-version and rust-toolchain.toml channel.
	rustMsrv = "1.92"

	// SYNC:* feature sets lifted verbatim from ci.yml's Test Suite job — keep in
	// lockstep with the YAML (the ci.yml steps carry the same SYNC: tags).
	coreTestFeatures   = "arrow,federation,kafka,mysql,postgres,redis-apq,schema-lint,sqlite,sqlserver,test-utils,wire-backend"
	dbTestFeatures     = "mysql,postgres,sqlite,sqlserver,wire-backend"
	serverTestFeatures = "arrow,auth,aws-s3,federation,grpc,mcp,metrics,observers,redis-apq,rest,secrets,testing,tracing-opentelemetry,webhooks,wire-backend"
)

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
		From(ubuntuImage).
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{
			"apt-get", "install", "-y", "--no-install-recommends",
			"git", "gawk", "findutils", "grep", "ca-certificates",
		})
}

// ── Phase 02: Fast Gates ──────────────────────────────────────────────────────
//
// Ports the cheap-but-frequent lint/format/doc gates from ci.yml so every change
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
		{"clippy", m.Clippy},
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

// Fmt mirrors ci.yml's Format Check: `cargo +nightly fmt --all -- --check`. rustfmt's
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

// Clippy mirrors ci.yml's Clippy Lints:
// `cargo clippy --workspace --all-features --all-targets -- -D warnings`.
// --all-features is intentional (lints every feature path; the test-* gate features
// only need infra at runtime) — see the ci.yml comment.
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
		Stdout(ctx)
}

// Rustdoc mirrors ci.yml's Documentation gate:
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

// ShellGates runs every non-Rust lint gate from the ci.yml clippy job verbatim, in
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
		"bash tools/check-test-imports.sh",
		"bash tools/check-route-syntax.sh",
		"bash tools/check-deploy-security.sh",
		"bash tools/check-audit-lockstep.sh",
		"bash tools/check-deadlines.sh",
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
			"make", "git", "gawk", "findutils", "grep", "ca-certificates",
		})
}

// ── Phase 03: Workspace Test Suite ────────────────────────────────────────────
//
// Ports ci.yml's `test` job (Linux path): a full `cargo build --all-features`
// followed by the feature-scoped `cargo test -p …` invocations (the SYNC:* lists)
// and the doctest pass. Parameterized by toolchain (stable | MSRV 1.92).

// Test runs the workspace test suite for the given toolchain. `rust` is "msrv"
// (default — the pinned floor, == rust-toolchain.toml) or "stable" (latest stable).
//
// Testcontainers-backed tests are SKIPPED here: the Dagger engine has no Docker
// socket, so tests that boot their own Postgres container (storage metadata/
// migrations/routes, functions migrations, all of fraiseql-wire's tests/* binaries)
// cannot run. They fail cleanly (no container leak), and are restored in Phase 04
// via Dagger-native service bindings. The skip is logged explicitly. See
// parity-notes.md Phase 03/04.
func (m *FraiseqlCi) Test(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// +optional
	// +default="msrv"
	rust string,
) (string, error) {
	toolchain := resolveToolchain(rust)
	// Per-toolchain target cache: stable and 1.92 produce incompatible artifacts,
	// so they must not share a target dir (kept separate from the Phase-02 gates'
	// `fraiseql-rust-target`, which holds clippy/rustdoc check artifacts).
	targetVol := "fraiseql-rust-target-test-" + strings.ReplaceAll(toolchain, ".", "-")

	// Skip patterns for the testcontainers lib tests (storage + functions); wire's
	// container tests live in tests/*, so we run only its lib unit tests.
	skip := "-- --skip metadata::tests --skip migrations::tests --skip routes::tests"

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### cargo build --all-features'",
		// No: on a cold run the verbose stream + telemetry can back up the
		// dagger client session and time out the return value ("client session
		// attachables: context deadline exceeded"). Failures still surface (cargo
		// prints them regardless), and warm runs are short.
		"cargo build --all-features",
		"echo '### skipped in-engine (env-incompatible; restored in a later phase):'",
		"echo '###   testcontainers (need Docker): storage metadata/migrations/routes::tests, functions migrations::tests, fraiseql-wire tests/*'",
		"echo '###   runtime-deno (v8 SIGSEGVs in exec sandbox): functions deno tests, excluded by feature'",
		"echo '### cargo test --workspace (non-DB crates; wire+functions run separately below)'",
		"cargo test --workspace" +
			" --exclude fraiseql-core --exclude fraiseql-db --exclude fraiseql-arrow" +
			" --exclude fraiseql-observers --exclude fraiseql-server --exclude fraiseql-wire" +
			" --exclude fraiseql-functions" +
			" --all-features " + skip,
		"echo '### cargo test -p fraiseql-wire --lib (tests/* skipped: testcontainers)'",
		"cargo test -p fraiseql-wire --lib --all-features",
		// fraiseql-functions runs with all features EXCEPT runtime-deno. Every v8
		// path (the 23 runtime::deno tests + observer::tests::*dispatches_ts_to_deno)
		// is #[cfg(feature = "runtime-deno")], and embedded V8 SIGSEGVs inside the
		// engine's exec sandbox even single-threaded (it works on the bare-metal
		// runner). Dropping the feature cfgs those tests out cleanly; the build
		// --all-features step above still compiles the deno path. migrations::tests
		// skipped (testcontainers). v8-in-sandbox is a follow-up (host-run or a
		// relaxed exec sandbox) — see parity-notes.md.
		"echo '### cargo test -p fraiseql-functions (all features except runtime-deno: v8 SIGSEGVs in-engine; migrations::tests skipped: testcontainers)'",
		"cargo test -p fraiseql-functions --features 'runtime-wasm,host-live,host-storage' -- --skip migrations::tests",
		// core/db: --lib only. Their src/ unit tests are Docker-free, but their
		// tests/* integration binaries boot Postgres via tests/common/testcontainer.rs
		// (and the federation/* docker tests) — those belong to Phase 04's integration
		// matrix (Dagger services), not the unit-test phase. server (step below) is
		// already --lib for the same reason.
		"echo '### cargo test -p fraiseql-core --lib (SYNC:CORE_FEATURES; tests/* = testcontainer integration → Phase 04)'",
		"cargo test -p fraiseql-core --lib --features '" + coreTestFeatures + "'",
		"echo '### cargo test -p fraiseql-db --lib (SYNC:DB_FEATURES; tests/* = testcontainer integration → Phase 04)'",
		"cargo test -p fraiseql-db --lib --features '" + dbTestFeatures + "'",
		"echo '### cargo test -p fraiseql-server --lib (SYNC:SERVER_FEATURES)'",
		"cargo test -p fraiseql-server --lib --features '" + serverTestFeatures + "'",
		// fraiseql-observers --lib: the Docker-free unit tests (config, executor,
		// DLQ, email, CLI). DB/redis/nats tests are #[ignore]d (or skip-on-None)
		// and run in the integration legs; `--features cli` pulls in the CLI
		// subcommand tests. Previously observers was excluded from the workspace
		// run and only its #[ignore]d/name-filtered tests ran (in integration),
		// so these unit tests never executed in CI.
		"echo '### cargo test -p fraiseql-observers --lib --features cli (Docker-free unit tests; DB/redis/nats tests are #[ignore]d → integration legs)'",
		"cargo test -p fraiseql-observers --lib --features cli",
		"echo '### cargo test --doc --all-features'",
		"cargo test --doc --all-features",
		"echo \"test OK: workspace suite passed (toolchain " + toolchain + ", testcontainers tests skipped)\"",
	}, "\n")

	return m.rustBaseFor(toolchain).
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
// Ports ci.yml's integration-* jobs onto Dagger-native service bindings — NO
// testcontainers, NO DinD. Each backing service is a dag.Container().AsService()
// bound into the test container; the tests read the injected env URL through the
// fraiseql-test-support harness. This makes local == CI: `dagger call
// test-integration` here provisions the same pinned, bound services as the
// self-hosted workflow does. See parity-notes.md Phase 04.

const (
	// pgImage pins the integration Postgres (matches ci.yml's integration jobs).
	pgImage = "ghcr.io/fraiseql/postgres:16"
	// pgUser/pgPassword/pgDatabase are the test-only Postgres credentials from ci.yml.
	pgUser     = "fraiseql_test"
	pgPassword = "fraiseql_test_password"
	pgDatabase = "test_fraiseql"
	// pgBindHost is the service-binding alias; bound callers reach Postgres here on
	// its internal 5432 (not the legacy host-mapped 5433).
	pgBindHost = "postgres"

	// redisImage / redisBindHost — the Redis service (ci.yml integration-redis).
	redisImage    = "ghcr.io/fraiseql/redis:7-alpine"
	redisBindHost = "redis"

	// vaultImage / vaultBindHost / vaultToken — the Vault dev-mode service
	// (ci.yml integration-vault). Dev-mode root token; test-only.
	vaultImage    = "ghcr.io/fraiseql/vault:1.17"
	vaultBindHost = "vault"
	vaultToken    = "fraiseql-test-token"

	// mysqlImage / mysqlBindHost / mysqlRootPassword — the MySQL service (ci.yml
	// integration-mysql). User/password/database match the Postgres ones
	// (fraiseql_test / fraiseql_test_password / test_fraiseql), so the pg* consts
	// are reused for the URL.
	mysqlImage        = "ghcr.io/fraiseql/mysql:8.3"
	mysqlBindHost     = "mysql"
	mysqlRootPassword = "fraiseql_test_root"

	// natsImage / natsBindHost — the NATS JetStream service (ci.yml integration-nats),
	// started with `-js -m 8222`.
	natsImage    = "ghcr.io/fraiseql/nats:2.10-alpine"
	natsBindHost = "nats"

	// mailhogImage / mailhogBindHost — the MailHog SMTP sink for the #349 email
	// happy-path test. Speaks real SMTP on 1025 (plaintext) and exposes an HTTP
	// API on 8025 to inspect captured messages; the test sends through lettre and
	// asserts the message arrived in the sink.
	mailhogImage    = "ghcr.io/fraiseql/mailhog:v1.0.1"
	mailhogBindHost = "mailhog"

	// serverBindHost / e2eMetricsToken — the HTTP E2E server service (ci.yml
	// integration-http-e2e): the fraiseql-server binary run as a bound service the
	// test container drives over HTTP.
	serverBindHost  = "fraiseql-server"
	e2eMetricsToken = "e2e-test-metrics-token-32chars!"

	// tlsBindHost — the TLS Postgres service (ci.yml integration-tls). The cert's
	// SAN includes this alias (CERT_HOSTNAME) so rustls servername verification
	// passes when the wire client connects to it.
	tlsBindHost = "postgres-tls"

	// wireBindHost — the Postgres service for the fraiseql-wire integration tests.
	// It enables SCRAM-SHA-256 explicitly so the wire client's auth path (and the
	// auth/scram rejection tests) are exercised exactly as under the old testcontainer.
	wireBindHost = "postgres-wire"

	// sqlserverImage / sqlserverBindHost / sqlserverSaPassword — the SQL Server
	// service (ci.yml integration-sqlserver). mssql has no initdb mechanism, so
	// init.sql is applied via sqlcmd behind a readiness loop before the tests run.
	sqlserverImage      = "mcr.microsoft.com/mssql/server:2022-CU16-ubuntu-22.04"
	sqlserverBindHost   = "sqlserver"
	sqlserverSaPassword = "FraiseQL_Test1234"

	// azuriteImage / azuriteBindHost — the Azure Blob emulator for the fraiseql-storage
	// azure_emulator test (ci.yml integration-storage). The backend reaches it at
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
// tiers converge onto the harness (see .phases/dagger-adoption/phase-04-…).
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
	case "sqlite":
		return m.integrationSqlite(ctx, source)
	case "mysql":
		return m.integrationMysql(ctx, source)
	case "nats":
		return m.integrationNats(ctx, source)
	case "observers":
		return m.integrationObservers(ctx, source)
	case "http-e2e":
		return m.integrationHTTPE2e(ctx, source)
	case "tls":
		return m.integrationTLS(ctx, source)
	case "sqlserver":
		return m.integrationSQLServer(ctx, source)
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
	case "cross-db":
		return m.integrationCrossDb(ctx, source)
	default:
		return "", fmt.Errorf("unknown integration suite %q (known: postgres, sqlite, mysql, nats, observers, http-e2e, tls, sqlserver, server, redis, vault, wire, storage, server-storage, federation, cross-db)", suite)
	}
}

// integrationPostgres binds a seeded postgres:16 service and runs the PostgreSQL
// integration tests that already route through the harness. The harness reads
// DATABASE_URL (injected below) and connects to the bound service.
func (m *FraiseqlCi) integrationPostgres(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: postgres (Dagger-bound service; tests read DATABASE_URL via harness)'",
		// Broad core/db `--test '*'` sweep (matches the legacy integration-postgres job).
		// The mysql/sqlserver/redis/federation-gated tests skip cleanly (only pg is bound).
		"cargo test -p fraiseql-core --features '" + coreTestFeatures + ",test-postgres' --test '*' -- --test-threads=1",
		"cargo test -p fraiseql-db --features '" + dbTestFeatures + ",test-postgres' --test '*' -- --test-threads=1",
		// Tier-C migrated: fraiseql-functions cron-state migration (lib tests; harness postgres()).
		"cargo test -p fraiseql-functions --lib migrations::tests -- --test-threads=1",
		"echo 'test-integration OK: postgres suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationSqlite runs the SQLite integration tests. SQLite is in-process
// (`SqliteAdapter::in_memory`) — no service binding, no env URL. The `sqlite`
// feature compiles a different code path than `test-postgres`; both sets of
// artifacts coexist in the shared integration target cache (cargo keys fingerprints
// per feature-set; sccache backs the cross-feature object reuse).
func (m *FraiseqlCi) integrationSqlite(ctx context.Context, source *dagger.Directory) (string, error) {
	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: sqlite (in-process; no service)'",
		"cargo test -p fraiseql-core --features sqlite --test integration -- multi_database_integration::sqlite --test-threads=1",
		"echo 'test-integration OK: sqlite suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationMysql binds a seeded MySQL service and runs the MySQL multi-database
// integration tests. They are #[cfg(feature = "test-mysql")] and read MYSQL_URL,
// querying the v_user / v_post views (init.sql) and the fn_create_tag stored
// procedure (procedures.sql).
func (m *FraiseqlCi) integrationMysql(ctx context.Context, source *dagger.Directory) (string, error) {
	mysqlURL := fmt.Sprintf("mysql://%s:%s@%s:3306/%s", pgUser, pgPassword, mysqlBindHost, pgDatabase)

	svc, err := m.mysqlService(ctx, source)
	if err != nil {
		return "", err
	}

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: mysql (Dagger-bound service; tests read MYSQL_URL)'",
		"cargo test -p fraiseql-core --features test-mysql --test integration -- multi_database_integration --test-threads=1",
		"echo 'test-integration OK: mysql suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(mysqlBindHost, svc).
		WithEnvVariable("MYSQL_URL", mysqlURL).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// mysqlService returns a started mysql:8.3 service seeded with init.sql (the views
// the tests query) and procedures.sql (the fn_create_tag stored procedure). MySQL's
// entrypoint creates the user/db from the env vars and runs
// /docker-entrypoint-initdb.d on first boot.
//
// procedures.sql uses `//` as its statement terminator with no DELIMITER statement
// (legacy loaded it via `mysql --delimiter="//"`). The entrypoint runs initdb files
// through the mysql client with the default `;` delimiter, so we wrap the file body
// in `DELIMITER //` … `DELIMITER ;` (a client directive the mysql CLI honours) and
// seed the wrapped copy.
func (m *FraiseqlCi) mysqlService(ctx context.Context, source *dagger.Directory) (*dagger.Service, error) {
	procs, err := source.File("tests/sql/mysql/procedures.sql").Contents(ctx)
	if err != nil {
		return nil, fmt.Errorf("read mysql procedures.sql: %w", err)
	}
	wrappedProcs := "DELIMITER //\n" + procs + "\nDELIMITER ;\n"

	initDir := dag.Directory().
		WithFile("00-init.sql", source.File("tests/sql/mysql/init.sql")).
		WithNewFile("01-procedures.sql", wrappedProcs)

	return dag.Container().
		From(mysqlImage).
		WithEnvVariable("MYSQL_ROOT_PASSWORD", mysqlRootPassword).
		WithEnvVariable("MYSQL_DATABASE", pgDatabase).
		WithEnvVariable("MYSQL_USER", pgUser).
		WithEnvVariable("MYSQL_PASSWORD", pgPassword).
		WithDirectory("/docker-entrypoint-initdb.d", initDir).
		WithExposedPort(3306).
		AsService(), nil
}

// integrationServer binds a seeded Postgres and runs fraiseql-server's
// database-query integration tests. They use try_database_url() + skip-on-None
// (no #[ignore]), so they run plainly and execute once DATABASE_URL is injected.
func (m *FraiseqlCi) integrationServer(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: server database (Dagger-bound postgres)'",
		"cargo test -p fraiseql-server --test database_query_test -- --test-threads=1",
		// Tier-C migrated (each helper creates + TRUNCATE/DROP its tables for shared-DB isolation).
		"cargo test -p fraiseql-server --test usage_postgres_backend_test -- --test-threads=1",
		"cargo test -p fraiseql-server --features observers --test observer_repository_test -- --test-threads=1",
		// pipeline_e2e is env-gated (FRAISEQL_PIPELINE_E2E); it compiles a schema and drives a server.
		"cargo test -p fraiseql-server --test pipeline_e2e_test -- --test-threads=1",
		"echo 'test-integration OK: server suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithEnvVariable("DATABASE_URL", dbURL).
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

	// Every tests/*.rs binary except tls_integration (own suite) and the common/ helper.
	wireBins := []string{
		"client_integration", "config_integration", "integration", "integration_full",
		"integration_operators", "integration_pause_resume", "load_tests", "metrics_integration",
		"property_protocol", "property_protocol_extended", "protocol_robustness_test",
		"rust_predicate_integration", "scram_integration", "sdk_sql_compliance_test",
		"streaming_integration", "stress_tests", "testcontainer_auth", "typed_streaming",
	}

	lines := []string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: wire (Dagger-bound SCRAM postgres; tests read DATABASE_URL via harness)'",
	}
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
		"cargo test -p fraiseql-storage --lib -- metadata::tests migrations::tests routes::tests --test-threads=1",
		"cargo test -p fraiseql-storage --features azure-blob --test azure_emulator -- --test-threads=1",
		"cargo test -p fraiseql-storage --features gcs --test gcs_emulator -- --test-threads=1",
		"echo 'test-integration OK: storage suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithServiceBinding(azuriteBindHost, m.azuriteService()).
		WithServiceBinding(fakeGcsBindHost, m.fakeGcsService()).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("AZURE_BLOB_ENDPOINT", azureEndpoint).
		WithEnvVariable("GCS_ENDPOINT", gcsEndpoint).
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

// integrationCrossDb binds Postgres + MySQL and runs fraiseql-core's cross-database
// parity tests (cross_database_test in the `integration` binary). They are gated by
// FEDERATION_TESTS (the legacy job left it unset, so they were a no-op) and apply their
// own schema/seed to each backend via the harness postgres()/mysql() services.
func (m *FraiseqlCi) integrationCrossDb(ctx context.Context, source *dagger.Directory) (string, error) {
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	mysqlURL := fmt.Sprintf("mysql://%s:%s@%s:3306/%s", pgUser, pgPassword, mysqlBindHost, pgDatabase)

	svc, err := m.mysqlService(ctx, source)
	if err != nil {
		return "", err
	}

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: cross-db (Dagger-bound postgres + mysql; FEDERATION_TESTS=1)'",
		"cargo test -p fraiseql-core --features 'test-postgres,test-mysql' --test integration -- cross_database_test --test-threads=1",
		"echo 'test-integration OK: cross-db suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithServiceBinding(mysqlBindHost, svc).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("MYSQL_URL", mysqlURL).
		WithEnvVariable("FEDERATION_TESTS", "1").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationServerStorage binds a MinIO (S3-compatible) service and runs the S3 backend
// against it from two angles:
//   - fraiseql-server's storage_minio integration test (Tier-C migrated off the MinIO
//     testcontainer), which reads MINIO_ENDPOINT via the harness; and
//   - fraiseql-storage's own backend::s3 unit tests (audit #440), previously triple-gated
//     (aws-s3 not in any CI feature set, #[ignore], skip-if-no-S3_ENDPOINT) and therefore
//     never executed in CI — this is what let H40 (S3 NotFound detection) survive. Those
//     tests read S3_ENDPOINT and create their own per-test bucket.
//
// Both authenticate with the minioadmin/minioadmin dev credentials. The storage run is
// filtered to backend::s3 so an --include-ignored sweep does not also pull in the crate's
// DB-backed metadata/migrations/routes tests (no Postgres is bound in this leg).
func (m *FraiseqlCi) integrationServerStorage(ctx context.Context, source *dagger.Directory) (string, error) {
	minioEndpoint := fmt.Sprintf("http://%s:9000", minioBindHost)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: server-storage (Dagger-bound MinIO; tests read MINIO_ENDPOINT / S3_ENDPOINT)'",
		"cargo test -p fraiseql-server --features aws-s3 --test storage_minio_integration_test -- --test-threads=1",
		"echo '### integration: storage backend::s3 unit tests (audit #440; read S3_ENDPOINT)'",
		"cargo test -p fraiseql-storage --features aws-s3 backend::s3 -- --include-ignored --test-threads=1",
		"echo 'test-integration OK: server-storage suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(minioBindHost, m.minioService()).
		WithEnvVariable("MINIO_ENDPOINT", minioEndpoint).
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
			"cargo build -p fraiseql-server --features federation && cp target/debug/fraiseql-server /usr/local/bin/fraiseql-server",
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
		"echo '### integration: redis (core APQ + observers queue/lease) — Dagger-bound redis+postgres'",
		"cargo test -p fraiseql-core --features redis-apq --lib redis -- --ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features 'caching,queue,redis-lease' --lib -- --ignored --test-threads=1",
		"echo 'test-integration OK: redis suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithServiceBinding(redisBindHost, m.redisService()).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("TEST_DATABASE_URL", dbURL).
		WithEnvVariable("REDIS_URL", redisURL).
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
		"cargo test -p fraiseql-server --features secrets --test secrets_manager_integration_test -- --ignored --test-threads=1",
		"echo 'test-integration OK: vault suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(vaultBindHost, m.vaultService()).
		WithEnvVariable("VAULT_ADDR", vaultAddr).
		WithEnvVariable("VAULT_TOKEN", vaultToken).
		WithEnvVariable("FRAISEQL_VAULT_ALLOW_INSECURE", "true").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// integrationSQLServer runs ci.yml's integration-sqlserver job as a real enforcing
// gate (no continue-on-error). mssql:2022 has no initdb mechanism, so tests/sql/
// sqlserver/init.sql is applied via sqlcmd (from the mssql image, which ships it)
// behind a readiness loop — that loop structurally removes the startup-race flake the
// legacy job hit. The init runs in its own container bound to the same service; the
// test container takes a data dependency on the init's marker file so the schema is
// in place before the tests run. The 4 sqlserver test modules read SQLSERVER_URL via
// the harness (test_support::sqlserver) and append their database.
func (m *FraiseqlCi) integrationSQLServer(ctx context.Context, source *dagger.Directory) (string, error) {
	// The mssql service self-applies init.sql on boot (see sqlserverService), so EVERY
	// instance is initialized — this is what makes the suite robust. The earlier
	// design applied init.sql from a SEPARATE init container bound to the same service
	// object; Dagger does not guarantee that a second container binding the same
	// service reuses the first's running instance, so the test container could connect
	// to an UNINITIALIZED mssql (no databases) → bb8 retried each connect to the full
	// 30s timeout → a deterministic 21×30s ≈ 630s wall of `bb8: Timed out` panics.
	// (Surfaced reproducibly once the 2026-06-02 disk migration left the engine cold;
	// init.sql + mssql are both fine standalone. See parity-notes.md.) Start() holds
	// one instance up across the readiness gate and the test container.
	svc, err := m.sqlserverService(source).Start(ctx)
	if err != nil {
		return "", fmt.Errorf("starting sqlserver service: %w", err)
	}
	const sqlcmd = "/opt/mssql-tools18/bin/sqlcmd"
	// -b makes sqlcmd exit non-zero on any SQL error, so the poll only breaks once the
	// fraiseql_test DB AND the dbo.init_done sentinel (written last by init.sql) exist.
	probe := fmt.Sprintf("%s -b -S %s,1433 -U sa -P '%s' -C -d fraiseql_test -Q 'SET NOCOUNT ON; SELECT TOP 1 ok FROM dbo.init_done'",
		sqlcmd, sqlserverBindHost, sqlserverSaPassword)

	// Readiness gate: block until the service has FULLY applied init.sql, then emit a
	// marker the test container depends on so cargo test never races the DB warmup
	// (Dagger only waits for port 1433 to listen, which happens before init completes).
	readyScript := strings.Join([]string{
		"set -e",
		"for i in $(seq 1 90); do",
		"  " + probe + " >/dev/null 2>&1 && break",
		"  echo \"waiting for sqlserver init ($i/90)...\"; sleep 2",
		"  if [ \"$i\" -eq 90 ]; then echo 'sqlserver init never completed'; exit 1; fi",
		"done",
		"echo ok > /tmp/ready",
	}, "\n")

	// Readiness probe runs in the mssql image (it has sqlcmd); no init.sql needed here.
	readyMarker := dag.Container().
		From(sqlserverImage).
		WithServiceBinding(sqlserverBindHost, svc).
		WithExec([]string{"bash", "-c", readyScript}).
		File("/tmp/ready")

	sqlserverURL := fmt.Sprintf("server=%s,1433;user=sa;password=%s;TrustServerCertificate=true", sqlserverBindHost, sqlserverSaPassword)

	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"echo '### integration: sqlserver (Dagger-bound self-initializing mssql:2022)'",
		"cargo test -p fraiseql-core --features test-sqlserver --test integration -- multi_database_integration --test-threads=1",
		"echo 'test-integration OK: sqlserver suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(sqlserverBindHost, svc).
		// Data dependency on the readiness marker forces init.sql to be fully applied
		// before cargo test runs.
		WithFile("/tmp/ready", readyMarker).
		WithEnvVariable("SQLSERVER_URL", sqlserverURL).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// sqlserverService returns a self-initializing SQL Server 2022 (Developer edition)
// service. Its startup command launches sqlservr, waits for it to accept connections,
// applies init.sql, then waits on sqlservr to hold the service in the foreground.
// Baking init.sql into the service (rather than applying it from a separate
// container) means every instance is initialized — robust against Dagger's service
// instance lifecycle, which otherwise left the test container talking to an
// uninitialized mssql. init.sql is idempotent, so a re-applied instance is harmless.
func (m *FraiseqlCi) sqlserverService(source *dagger.Directory) *dagger.Service {
	const tools = "/opt/mssql-tools18/bin/sqlcmd"
	entry := strings.Join([]string{
		"set -e",
		"/opt/mssql/bin/sqlservr & SQLSERVR_PID=$!",
		"for i in $(seq 1 90); do",
		"  " + tools + " -b -S localhost,1433 -U sa -P '" + sqlserverSaPassword + "' -C -Q 'SELECT 1' >/dev/null 2>&1 && break",
		"  sleep 2",
		"done",
		tools + " -b -S localhost,1433 -U sa -P '" + sqlserverSaPassword + "' -C -i /init.sql",
		"wait $SQLSERVR_PID",
	}, "\n")

	return dag.Container().
		From(sqlserverImage).
		WithEnvVariable("ACCEPT_EULA", "Y").
		WithEnvVariable("MSSQL_SA_PASSWORD", sqlserverSaPassword).
		WithEnvVariable("MSSQL_PID", "Developer").
		WithFile("/init.sql", source.File("tests/sql/sqlserver/init.sql")).
		WithExposedPort(1433).
		AsService(dagger.ContainerAsServiceOpts{Args: []string{"bash", "-c", entry}})
}

// integrationTLS runs ci.yml's integration-tls job: a TLS-enabled Postgres and the
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
		"cargo test -p fraiseql-wire --test tls_integration -- --test-threads=1",
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

// integrationHTTPE2e runs ci.yml's integration-http-e2e job: it boots the actual
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
	const targetVol = "fraiseql-rust-target-integ2-1-92"
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	// Build the binary and copy it out of the (cache-mounted) target dir to a plain
	// path so it can be extracted as a File into the runtime service container.
	built := m.rustBaseFor(rustMsrv).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume(targetVol)).
		WithExec([]string{
			"bash", "-c",
			"cargo build -p fraiseql-server && cp target/debug/fraiseql-server /usr/local/bin/fraiseql-server",
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
// integration suites from ci.yml's integration-observers job: PostgreSQL NOTIFY
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
		"cargo test -p fraiseql-observers --features postgres --lib postgres_notify -- --test-threads=1",
		// Lease/storage: kept as the legacy `--lib --ignored` no-op. Those tests are
		// skip-on-None (not #[ignore]d) so this runs 0; running them unfiltered pulls in
		// the SSRF-guard unit tests, which assert the guard is ON and so fail under this
		// suite's FRAISEQL_OBSERVERS_ALLOW_INSECURE=true. Lease coverage gap == legacy.
		"cargo test -p fraiseql-observers --features 'postgres,caching,redis-lease' --lib -- --ignored --test-threads=1",
		"cargo test -p fraiseql-observers --features 'postgres,nats' --test bridge_integration -- --ignored --test-threads=1",
		"cargo test -p fraiseql-server --features observers-nats --test observer_runtime_integration_test -- --ignored --test-threads=1",
		// #349 email happy-path: send through lettre to the bound MailHog sink and
		// assert the message arrived (real SMTP wire format, not a stub).
		"cargo test -p fraiseql-observers --test smtp_integration -- --ignored --test-threads=1",
		// #443 / #437 F6: change-log RLS isolation under a NOBYPASSRLS role (the
		// superuser DATABASE_URL would mask the policy). The test creates its own
		// tenant/consumer roles off the superuser connection — no extra env needed.
		"cargo test -p fraiseql-observers --features postgres --test rls_isolation -- --ignored --test-threads=1",
		"echo 'test-integration OK: observers suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithServiceBinding(redisBindHost, m.redisService()).
		WithServiceBinding(natsBindHost, m.natsService()).
		WithServiceBinding(mailhogBindHost, m.mailhogService()).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("TEST_DATABASE_URL", dbURL).
		WithEnvVariable("REDIS_URL", redisURL).
		WithEnvVariable("NATS_URL", natsURL).
		WithEnvVariable("MAILHOG_SMTP_HOST", mailhogBindHost).
		WithEnvVariable("MAILHOG_SMTP_PORT", "1025").
		WithEnvVariable("MAILHOG_API", fmt.Sprintf("http://%s:8025", mailhogBindHost)).
		WithEnvVariable("FRAISEQL_ALLOW_PRIVATE_WEBHOOKS", "true").
		WithEnvVariable("FRAISEQL_OBSERVERS_ALLOW_INSECURE", "true").
		// The bound JetStream service speaks plaintext nats:// (bridge_integration);
		// opt into plaintext for the test broker (L-nats-plaintext).
		WithEnvVariable("FRAISEQL_NATS_ALLOW_PLAINTEXT", "true").
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
		"cargo test -p fraiseql-observers --features nats --test nats_integration -- --ignored --test-threads=1",
		"echo 'test-integration OK: nats suite passed'",
	}, "\n")

	return m.integrationBase(source, rustMsrv).
		WithServiceBinding(natsBindHost, m.natsService()).
		WithEnvVariable("NATS_URL", natsURL).
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
		WithFile("01-init-analytics.sql", source.File("tests/sql/postgres/init-analytics.sql"))

	return dag.Container().
		From(pgImage).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithDirectory("/docker-entrypoint-initdb.d", initDir).
		WithExposedPort(5432).
		AsService()
}

// integrationBase mounts the source on rustBaseFor(toolchain), ready to bind
// services into. It uses a dedicated integration target-cache volume (kept apart
// from the Phase-02 gate and Phase-03 unit-test caches, which hold different
// feature/artifact sets) and sets RUST_LOG=debug like the legacy integration jobs.
func (m *FraiseqlCi) integrationBase(source *dagger.Directory, rust string) *dagger.Container {
	toolchain := resolveToolchain(rust)
	targetVol := "fraiseql-rust-target-integ2-" + strings.ReplaceAll(toolchain, ".", "-")
	return m.rustBaseFor(toolchain).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume(targetVol)).
		WithEnvVariable("RUST_LOG", "debug")
}
