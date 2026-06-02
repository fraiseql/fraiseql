package main

// ── Phase 05: Feature-Flag Check Matrix ───────────────────────────────────────
//
// Ports ci.yml's feature-flags.yml `cargo check --features …` matrix (the
// feature-matrix / database-matrix / storage-matrix / functions-matrix jobs) onto
// ONE parameterized Dagger function. Every combination is data in `featureCombos`
// below, not duplicated YAML: adding a combo is a one-line struct literal, and the
// `dagger-feature-matrix.yml` workflow generates its `strategy.matrix.combo` from
// `dagger call list-combos`, so a new combo propagates to CI without a YAML edit.
//
// These are `cargo check` (and, for the functions combos, `cargo clippy`) only —
// no test binaries run, so no backing services are needed. That makes this matrix
// immune to the Docker Hub anonymous pull rate-limit that gates the integration
// suites (it only ever pulls the already-cached rust:1.92 base). See parity-notes.md.

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"dagger/fraiseql-ci/internal/dagger"
)

// featureTargetVol is the shared target cache volume for every feature-check combo.
// Kept apart from the Phase-02 gate / Phase-03 unit-test / Phase-04 integration
// caches: those hold `--all-features` or test artifacts, whereas the feature combos
// compile narrow `--no-default-features` slices. All combos share this one volume
// (the matrix runs serially, so a warm cache is reused combo-to-combo; sccache backs
// the cross-feature object reuse for the unchanged upstream dependency graph).
const featureTargetVol = "fraiseql-rust-target-features-1-92"

// featureCombo is one entry of the feature-flag check matrix: a crate plus the
// feature set to compile it with. It mirrors one matrix row of feature-flags.yml.
type featureCombo struct {
	// name is the unique, GH-status-legible slug used by `--combo=` and as the
	// generated workflow matrix value.
	name string
	// crate is the `cargo -p` package.
	crate string
	// features is the `--features` list (empty ⇒ none passed).
	features []string
	// noDefaultFeatures adds `--no-default-features`. The server combos keep the
	// crate defaults ON (matching feature-flags.yml's `cargo check -p fraiseql-server
	// --features …`); core/storage/functions check isolated `--no-default-features`
	// slices.
	noDefaultFeatures bool
	// clippy runs `cargo clippy … --all-targets -- -D warnings` instead of `cargo
	// check`. Only the functions combos do this (their legacy job ran clippy too);
	// clippy is a superset of check, so we run clippy alone rather than both (one
	// compile, not two — cost over speed).
	clippy bool
}

// featureCombos is the whole matrix, ported verbatim from feature-flags.yml:
//   - server  (17): the `feature-matrix` job — `cargo check -p fraiseql-server`,
//     crate defaults ON, except the explicit `--no-default-features` case.
//   - core    (7):  the `database-matrix` job — `cargo check -p fraiseql-core
//     --no-default-features --features …`.
//   - storage (3):  the `storage-matrix` job — `cargo check -p fraiseql-storage
//     --no-default-features --features …`.
//   - functions (5): the `functions-matrix` job — `cargo clippy -p fraiseql-functions
//     --no-default-features --features … --all-targets -- -D warnings`.
//
// The 4 `feature-integration-tests` combos (mcp/metrics/apq-memory/tracing) from
// feature-flags.yml are NOT here: they run `cargo test` against test binaries (some
// service-backed), which belongs to the integration matrix, not this check-only
// matrix. Logged as a deferred gap in parity-notes.md, not silently dropped.
var featureCombos = []featureCombo{
	// ── server: feature-matrix (cargo check -p fraiseql-server) ──────────────
	{name: "server-no-default", crate: "fraiseql-server", noDefaultFeatures: true},
	{name: "server-auth-secrets", crate: "fraiseql-server", features: []string{"auth", "secrets"}},
	{name: "server-observers-rate-limiting", crate: "fraiseql-server", features: []string{"observers", "redis-rate-limiting"}},
	{name: "server-observers-enterprise-otel", crate: "fraiseql-server", features: []string{"observers-enterprise", "redis-rate-limiting", "tracing-opentelemetry"}},
	{name: "server-arrow-wire", crate: "fraiseql-server", features: []string{"arrow", "wire-backend"}},
	{name: "server-mcp-auth", crate: "fraiseql-server", features: []string{"mcp", "auth"}},
	{name: "server-observers-enterprise-redis", crate: "fraiseql-server", features: []string{"observers-enterprise", "redis-apq", "redis-pkce"}},
	{name: "server-federation", crate: "fraiseql-server", features: []string{"federation"}},
	{name: "server-secrets", crate: "fraiseql-server", features: []string{"secrets"}},
	{name: "server-wire-backend", crate: "fraiseql-server", features: []string{"wire-backend"}},
	{name: "server-azure-blob", crate: "fraiseql-server", features: []string{"azure-blob"}},
	{name: "server-gcs", crate: "fraiseql-server", features: []string{"gcs"}},
	{name: "server-metrics-observers", crate: "fraiseql-server", features: []string{"metrics", "observers"}},
	{name: "server-webhooks-auth", crate: "fraiseql-server", features: []string{"webhooks", "auth"}},
	{name: "server-otel", crate: "fraiseql-server", features: []string{"tracing-opentelemetry"}},
	{name: "server-functions-rest-testing", crate: "fraiseql-server", features: []string{"functions", "rest", "testing"}},
	{name: "server-kitchen-sink", crate: "fraiseql-server", features: []string{"auth", "observers", "secrets", "federation"}},

	// ── core: database-matrix (cargo check -p fraiseql-core --no-default-features) ──
	{name: "core-postgres", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"postgres"}},
	{name: "core-mysql", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"mysql"}},
	{name: "core-sqlite-sqlserver", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"sqlite", "sqlserver"}},
	{name: "core-all-backends", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"postgres", "mysql", "sqlite", "sqlserver"}},
	{name: "core-postgres-audit-syslog", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"postgres", "audit-syslog"}},
	{name: "core-postgres-audit-webhook", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"postgres", "audit-webhook"}},
	{name: "core-postgres-jwt-replay", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"postgres", "jwt-replay"}},

	// ── storage: storage-matrix (cargo check -p fraiseql-storage --no-default-features) ──
	{name: "storage-transforms", crate: "fraiseql-storage", noDefaultFeatures: true, features: []string{"transforms"}},
	{name: "storage-gcs", crate: "fraiseql-storage", noDefaultFeatures: true, features: []string{"gcs"}},
	{name: "storage-azure-blob", crate: "fraiseql-storage", noDefaultFeatures: true, features: []string{"azure-blob"}},

	// ── functions: functions-matrix (cargo clippy -p fraiseql-functions --no-default-features) ──
	{name: "functions-host-live", crate: "fraiseql-functions", noDefaultFeatures: true, clippy: true, features: []string{"host-live"}},
	{name: "functions-host-storage", crate: "fraiseql-functions", noDefaultFeatures: true, clippy: true, features: []string{"host-storage"}},
	{name: "functions-runtime-wasm", crate: "fraiseql-functions", noDefaultFeatures: true, clippy: true, features: []string{"runtime-wasm"}},
	{name: "functions-runtime-deno", crate: "fraiseql-functions", noDefaultFeatures: true, clippy: true, features: []string{"runtime-deno"}},
	{name: "functions-host-combined", crate: "fraiseql-functions", noDefaultFeatures: true, clippy: true, features: []string{"host-live", "host-storage"}},
}

// cargoArgs builds the `cargo check|clippy` invocation for this combo, mirroring the
// corresponding feature-flags.yml step exactly.
func (c featureCombo) cargoArgs() []string {
	sub := "check"
	if c.clippy {
		sub = "clippy"
	}
	args := []string{"cargo", sub, "-p", c.crate}
	if c.noDefaultFeatures {
		args = append(args, "--no-default-features")
	}
	if len(c.features) > 0 {
		args = append(args, "--features", strings.Join(c.features, ","))
	}
	if c.clippy {
		args = append(args, "--all-targets", "--", "-D", "warnings")
	}
	return args
}

// comboNames returns the combo slugs in declaration order (the workflow matrix order).
func comboNames() []string {
	names := make([]string, len(featureCombos))
	for i, c := range featureCombos {
		names[i] = c.name
	}
	return names
}

// lookupCombo finds a combo by name, returning a clear error (with the known names)
// when it doesn't exist — so a typo in `--combo=` fails fast, not with a confusing
// cargo error.
func lookupCombo(name string) (featureCombo, error) {
	for _, c := range featureCombos {
		if c.name == name {
			return c, nil
		}
	}
	return featureCombo{}, fmt.Errorf("unknown feature combo %q (known: %s)", name, strings.Join(comboNames(), ", "))
}

// featureBase mounts the source on the MSRV rustBase with the shared feature-check
// target cache volume. Reuses the Phase-02 rustBase (toolchain + native deps + mold +
// sccache + cargo registry/git caches).
func (m *FraiseqlCi) featureBase(source *dagger.Directory) *dagger.Container {
	return m.rustBaseFor(rustMsrv).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume(featureTargetVol))
}

// FeatureCheck compiles one named feature combo with `cargo check` (or `cargo clippy`
// for the functions combos) and returns its output. It is the per-combo unit the
// self-hosted `dagger-feature-matrix.yml` calls once per matrix entry. An unknown
// combo name fails fast with the list of known names.
func (m *FraiseqlCi) FeatureCheck(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	combo string,
) (string, error) {
	c, err := lookupCombo(combo)
	if err != nil {
		return "", err
	}

	cmd := strings.Join(c.cargoArgs(), " ")
	lines := []string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		fmt.Sprintf("echo '### feature-check: %s'", c.name),
		"echo '### " + cmd + "'",
	}
	if c.clippy {
		// rustBaseFor pins RUSTUP_TOOLCHAIN=1.92, but rustBase installs clippy on the
		// base image's *default* toolchain — not the instance RUSTUP_TOOLCHAIN selects
		// (`1.92-x86_64-unknown-linux-gnu`), which ships only rustc. So ensure clippy
		// for the active toolchain before running it (idempotent: a no-op once present).
		// Scoped here, not in rustBase: the Phase-02 clippy/fmt gates use rustBase()
		// directly (no RUSTUP_TOOLCHAIN) and are unaffected; only these clippy combos hit
		// the gap. Promote to rustBase if a future phase runs clippy under rustBaseFor.
		lines = append(lines, "rustup component add clippy")
	}
	lines = append(lines, cmd, fmt.Sprintf("echo 'feature-check OK: %s'", c.name))
	script := strings.Join(lines, "\n")

	return m.featureBase(source).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// FeatureMatrix runs every combo and returns a pass/fail summary. It runs SERIALLY
// over the single shared feature-check target cache volume — a deliberate divergence
// from the plan's errgroup/--max-parallel design:
//   - cost over speed is a hard project rule (the self-hosted runner is ~$0/min);
//   - cargo holds a per-target build lock, so combos sharing one target volume would
//     serialize on it anyway — real parallelism would need a target volume per worker,
//     multiplying disk on a disk-pressured box;
//   - CI runs one job per combo at max-parallel:1 regardless (RAM-bound box).
//
// fail-fast is OFF (every combo runs even after one fails), matching feature-flags.yml's
// `fail-fast: false`, so a single run reports the full matrix.
func (m *FraiseqlCi) FeatureMatrix(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	var report strings.Builder
	var failed []string

	for _, c := range featureCombos {
		out, err := m.FeatureCheck(ctx, source, c.name)
		if err != nil {
			failed = append(failed, c.name)
			fmt.Fprintf(&report, "\n===== %s (FAILED) =====\n%s\n%v\n", c.name, out, err)
			continue
		}
		fmt.Fprintf(&report, "\n===== %s (ok) =====\n%s\n", c.name, out)
	}

	if len(failed) > 0 {
		fmt.Fprintf(&report, "\nfeature-matrix FAILED: %d/%d combos failed: %s\n",
			len(failed), len(featureCombos), strings.Join(failed, ", "))
		return report.String(), fmt.Errorf("feature-matrix: %d combo(s) failed: %s",
			len(failed), strings.Join(failed, ", "))
	}
	fmt.Fprintf(&report, "\nfeature-matrix OK: all %d combos passed\n", len(featureCombos))
	return report.String(), nil
}

// ListCombos prints the combo names as a JSON array, for the
// `dagger-feature-matrix.yml` workflow to expand into `strategy.matrix.combo` via
// fromJSON. This is what makes a new Go combo show up as a CI status row with no
// YAML edit.
func (m *FraiseqlCi) ListCombos(ctx context.Context) (string, error) {
	data, err := json.Marshal(comboNames())
	if err != nil {
		return "", fmt.Errorf("marshal combo names: %w", err)
	}
	return string(data), nil
}
