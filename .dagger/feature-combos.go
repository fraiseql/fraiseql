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
// suites (it only ever pulls the already-cached rust:1.94.1 base). See parity-notes.md.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"
	"sync"

	"dagger/fraiseql-ci/internal/dagger"
)

// featureTargetVol is the BASE name of the feature-check target cache volume. Single
// combo runs (FeatureCheck) mount it directly; the parallel FeatureMatrix derives one
// "<base>-lane-N" volume per worker so concurrent combos never contend on cargo's
// per-target build lock. Kept apart from the Phase-02 gate / Phase-03 unit-test /
// Phase-04 integration caches (those hold `--all-features` or test artifacts). sccache
// (a separate shared volume) backs cross-combo/cross-lane object reuse for the
// unchanged upstream dependency graph.
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
//   - server  (18): the `feature-matrix` job — `cargo check -p fraiseql-server`,
//     crate defaults ON, except the explicit `--no-default-features` case. 17 are
//     ported from feature-flags.yml; `server-default-nonwire` guards the non-wire
//     dispatch arm (see below).
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
	{name: "server-auth-saml", crate: "fraiseql-server", features: []string{"auth-saml"}},
	// #367: `[auth.local]`'s mail bridge is `inbound-email`-gated. This combo is
	// the one that compiles the `cfg(not(feature = "inbound-email"))` arm — the
	// refusal path that tells an operator to rebuild with the feature.
	{name: "server-auth-no-email", crate: "fraiseql-server", features: []string{"auth"}, noDefaultFeatures: true},
	{name: "server-observers-rate-limiting", crate: "fraiseql-server", features: []string{"observers", "redis-rate-limiting"}},
	{name: "server-observers-enterprise-otel", crate: "fraiseql-server", features: []string{"observers-enterprise", "redis-rate-limiting", "tracing-opentelemetry"}},
	{name: "server-arrow-wire", crate: "fraiseql-server", features: []string{"arrow", "wire-backend"}},
	// Arrow WITHOUT wire-backend and without the crate defaults: the only shape in
	// which `run_postgres`'s `#[cfg(feature = "arrow")]` branch is the compiled one.
	// Nothing built it, so a signature change to `make_executor_factory` (#801)
	// updated the non-arrow call site and left this one uncompilable, and
	// `with_flight_service`'s `cfg(not(auth))` OIDC drift (#783) had no build that
	// would have shown it.
	{name: "server-arrow-no-auth", crate: "fraiseql-server", noDefaultFeatures: true, features: []string{"cli", "arrow"}},
	{name: "server-mcp-auth", crate: "fraiseql-server", features: []string{"mcp", "auth"}},
	{name: "server-observers-enterprise-redis", crate: "fraiseql-server", features: []string{"observers-enterprise", "redis-apq", "redis-pkce"}},
	{name: "server-federation", crate: "fraiseql-server", features: []string{"federation"}},
	{name: "server-secrets", crate: "fraiseql-server", features: []string{"secrets"}},
	{name: "server-wire-backend", crate: "fraiseql-server", features: []string{"wire-backend"}},
	{name: "server-azure-blob", crate: "fraiseql-server", features: []string{"azure-blob"}},
	{name: "server-gcs", crate: "fraiseql-server", features: []string{"gcs"}},
	{name: "server-metrics-observers", crate: "fraiseql-server", features: []string{"metrics", "observers"}},
	// #985: the Redis cache/invalidate mount. Without a combo naming it, only
	// the all-features clippy leg ever compiles the `observers-cache` ON path.
	{name: "server-observers-cache", crate: "fraiseql-server", features: []string{"observers-cache"}},
	{name: "server-webhooks-auth", crate: "fraiseql-server", features: []string{"webhooks", "auth"}},
	// Poll-IMAP email adapter: pulls async-imap + a rustls
	// TLS transport, so clippy it in isolation to catch feature-gating regressions.
	{name: "server-inbound-email", crate: "fraiseql-server", clippy: true, features: []string{"inbound-email"}},
	{name: "server-otel", crate: "fraiseql-server", features: []string{"tracing-opentelemetry"}},
	{name: "server-functions-rest-testing", crate: "fraiseql-server", features: []string{"functions", "rest", "testing"}},
	{name: "server-kitchen-sink", crate: "fraiseql-server", features: []string{"auth", "observers", "secrets", "federation"}},
	// server-default-nonwire covers the runtime URL-scheme dispatch (the real
	// `dispatch_server` arm), which is gated `not(wire-backend)` and compiled by NO
	// other leg: preflight clippy is `--all-features` (wire ON ⇒ the dispatch is
	// cfg'd out). Defaults stay ON (auth,cli) and wire stays OFF, so the non-wire arm
	// actually compiles. This replaced `server-multidb` when the non-PostgreSQL
	// backends were removed (G2, #374) — the combo it guarded is still the only one
	// that builds that arm, so it is renamed rather than deleted. check-only on
	// purpose — clippy would drag in the pre-existing bare-`arrow` lint debt.
	{name: "server-default-nonwire", crate: "fraiseql-server", features: []string{}},
	// server-rest-arrow is the ONE binary feature combo no other leg builds: preflight
	// clippy is `--all-features` (wire-backend ON ⇒ run_postgres is cfg'd out, so the
	// arrow path never compiles), server-arrow-wire pairs arrow WITH wire-backend (same
	// cfg-out), and server-functions-rest-testing has rest but not arrow. The
	// fraiseql-server-full Docker image is the only artifact that builds rest+arrow, and
	// it broke on the #330 tenancy wiring (the arrow path keeps a raw PostgresAdapter
	// while the tenant factory was typed for the cached adapter). check-only on purpose:
	// arrow stays out of the clippy combos (see the server-multidb note above).
	{name: "server-rest-arrow", crate: "fraiseql-server", features: []string{"rest", "arrow"}},
	// #920: `export-csv` and `export-xlsx` are independently selectable, and no leg
	// built either one alone — preflight and the unit legs are `--all-features`, the
	// Docker image ships `rest,arrow`. The XLSX writer imported the formula-injection
	// guard from the CSV module, so `rest,export-xlsx` did not compile at all. One
	// combo per writer, so a cross-module import in EITHER direction reddens here.
	// (`export-parquet` gets no combo on purpose: it has zero `cfg` usage, so building
	// it exercises nothing — that ghost is #1012's to delete or allowlist.)
	{name: "server-rest-export-xlsx", crate: "fraiseql-server", noDefaultFeatures: true, features: []string{"rest", "export-xlsx"}},
	{name: "server-rest-export-csv", crate: "fraiseql-server", noDefaultFeatures: true, features: []string{"rest", "export-csv"}},

	// ── core: database-matrix (cargo check -p fraiseql-core --no-default-features) ──
	{name: "core-postgres", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"postgres"}},
	{name: "core-postgres-audit-syslog", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"postgres", "audit-syslog"}},
	{name: "core-postgres-audit-webhook", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"postgres", "audit-webhook"}},
	{name: "core-postgres-jwt-replay", crate: "fraiseql-core", noDefaultFeatures: true, features: []string{"postgres", "jwt-replay"}},

	// ── storage: storage-matrix (cargo check -p fraiseql-storage --no-default-features) ──
	{name: "storage-transforms", crate: "fraiseql-storage", noDefaultFeatures: true, features: []string{"transforms"}},
	// #370: the server-side render endpoint passthrough — the storage crate
	// combo above cannot catch a break in fraiseql-server's mount or config.
	{name: "server-storage-transforms", crate: "fraiseql-server", features: []string{"storage-transforms"}},
	// #382: the server-side outbound-CDC mount (its own feature, pulling
	// fraiseql-cdc-sinks with the NATS sink).
	{name: "server-cdc-outbound", crate: "fraiseql-server", features: []string{"cdc-outbound"}},
	// #975: the Kafka sink's server mount. Distinct from server-cdc-outbound
	// because the delegating ConfiguredSink enum, the kafka arm of build_one and
	// the feature-on half of validate_kind only compile here — and the
	// feature-OFF half only compiles in the combo above. Neither covers both.
	{name: "server-cdc-kafka", crate: "fraiseql-server", features: []string{"cdc-kafka"}},
	// #975: the Kinesis sink's server mount, and the same argument as the kafka
	// combo above — the Kinesis arm of ConfiguredSink/build_one and the
	// feature-ON half of validate_kind compile only here, while the feature-OFF
	// half compiles only where cdc-kinesis is absent.
	{name: "server-cdc-kinesis", crate: "fraiseql-server", features: []string{"cdc-kinesis"}},
	{name: "storage-gcs", crate: "fraiseql-storage", noDefaultFeatures: true, features: []string{"gcs"}},
	{name: "storage-azure-blob", crate: "fraiseql-storage", noDefaultFeatures: true, features: []string{"azure-blob"}},

	// ── functions: functions-matrix (cargo clippy -p fraiseql-functions --no-default-features) ──
	{name: "functions-host-live", crate: "fraiseql-functions", noDefaultFeatures: true, clippy: true, features: []string{"host-live"}},
	{name: "functions-host-storage", crate: "fraiseql-functions", noDefaultFeatures: true, clippy: true, features: []string{"host-storage"}},
	{name: "functions-runtime-wasm", crate: "fraiseql-functions", noDefaultFeatures: true, clippy: true, features: []string{"runtime-wasm"}},
	{name: "functions-runtime-deno", crate: "fraiseql-functions", noDefaultFeatures: true, clippy: true, features: []string{"runtime-deno"}},
	{name: "functions-host-combined", crate: "fraiseql-functions", noDefaultFeatures: true, clippy: true, features: []string{"host-live", "host-storage"}},

	// ── federation: the gated #429 wired saga forward executor ────────────────
	// clippy `--all-targets` compiles the wired execute_step/execute_saga + their
	// SQLite/Postgres tests under -D warnings; the default-feature (fail-loud) path
	// is covered by preflight `--all-features` and the workspace test leg.
	{name: "fed-saga", crate: "fraiseql-federation", clippy: true, features: []string{"saga"}},
	// ── cdc: the gated #382 NATS JetStream outbound sink ──────────────────────
	// clippy `--all-targets` compiles the NatsJetStreamSink + its real-PG/NATS
	// e2e test under -D warnings on MSRV; the default (broker-free) drain path is
	// covered by preflight `--all-features` and the workspace test leg.
	{name: "cdc-nats-jetstream", crate: "fraiseql-cdc-sinks", clippy: true, features: []string{"cdc-nats-jetstream"}},
	// #975 the gated Kafka outbound sink. Its endpoint guard and topic charset are
	// pure and always compiled (covered by the default test leg); this combo is
	// what compiles KafkaSink itself, its produce-error classification and its
	// rdkafka-bound tests under -D warnings on MSRV.
	{name: "cdc-kafka", crate: "fraiseql-cdc-sinks", clippy: true, features: []string{"cdc-kafka"}},
	// #975 the gated Kinesis outbound sink. Its endpoint/region guard, the
	// endpoint-URL override guard and the stream charset are pure and always
	// compiled (covered by the default test leg); this combo is what compiles
	// KinesisSink itself, its PutRecord-error classification and its aws-sdk-bound
	// tests under -D warnings on MSRV.
	{name: "cdc-kinesis", crate: "fraiseql-cdc-sinks", clippy: true, features: []string{"cdc-kinesis"}},
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

// featureBase mounts the source on the MSRV rustBase with the given feature-check
// target cache volume. Reuses the Phase-02 rustBase (toolchain + native deps + mold +
// sccache + cargo registry/git caches). The target volume is a parameter so the
// parallel FeatureMatrix can give each lane its own (avoiding cargo's per-target build
// lock); single-combo FeatureCheck passes the shared featureTargetVol.
func (m *FraiseqlCi) featureBase(source *dagger.Directory, targetVol string) *dagger.Container {
	return m.rustBaseFor(rustMsrv).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithMountedCache("/src/target", dag.CacheVolume(targetVol))
}

// comboResultMarker is the OK/FAIL line runCombo's script prints; the Go layer parses
// it instead of relying on the cargo exit code (see runCombo for why).
func comboResultMarker(name, status string) string {
	return "=== COMBO-RESULT " + name + ": " + status + " ==="
}

// comboOK reports whether a runCombo output carries the success marker for `name`.
func comboOK(name, out string) bool {
	return strings.Contains(out, comboResultMarker(name, "OK"))
}

// FeatureCheck compiles one named feature combo with `cargo check` (or `cargo clippy`
// for the functions combos) and returns its output. Still callable standalone
// (`dagger call feature-check --combo=X`) for local single-combo runs on the shared
// target volume; the CI matrix now goes through FeatureMatrix. An unknown combo name
// fails fast with the list of known names; a compile failure returns the captured cargo
// output as the error.
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
	out, err := m.runCombo(ctx, source, c, featureTargetVol)
	if err != nil {
		return out, err
	}
	if !comboOK(c.name, out) {
		return out, fmt.Errorf("feature-check %s FAILED:\n%s", c.name, out)
	}
	return out, nil
}

// runCombo compiles one combo on the given target cache volume and returns its output.
// Factored out of FeatureCheck so the parallel FeatureMatrix can run each lane on its
// own target volume while single-combo FeatureCheck keeps the shared one.
//
// The compile runs in a captured subshell that ALWAYS exits 0; the pass/fail is encoded
// in a `COMBO-RESULT` marker line the caller parses. Why not let a non-zero `cargo` exit
// fail the WithExec? Because Dagger then surfaces ITS exec error in the TUI and discards
// FeatureMatrix's returned report — so a failing combo's cargo error would be buried in
// interleaved per-lane TUI output instead of the clean, ordered report. Exiting 0 + a
// marker keeps the full per-combo detail in the function's own output. On success the
// cargo output is suppressed (just the marker) to keep green logs compact; on failure the
// captured output is printed in full.
func (m *FraiseqlCi) runCombo(
	ctx context.Context,
	source *dagger.Directory,
	c featureCombo,
	targetVol string,
) (string, error) {
	cmd := strings.Join(c.cargoArgs(), " ")
	var setup string
	if c.clippy {
		// rustBaseFor pins RUSTUP_TOOLCHAIN=1.94.1, which since the patch pin IS the
		// base image's own default toolchain (`1.94.1-x86_64-unknown-linux-gnu`), so
		// rustBase's `rustup component add clippy` now lands on it. Kept anyway, and
		// idempotent: it costs a no-op, and it is the only thing between a future image
		// whose default diverges from RUSTUP_TOOLCHAIN and a clippy-less combo run. Under
		// the old floating `1.94` pin the two genuinely differed and this was load-bearing.
		// Scoped here, not in rustBase: the Phase-02 clippy/fmt gates use rustBase()
		// directly (no RUSTUP_TOOLCHAIN) and are unaffected; only these clippy combos hit
		// the gap. Promote to rustBase if a future phase runs clippy under rustBaseFor.
		setup = "rustup component add clippy >/dev/null\n"
	}
	script := fmt.Sprintf(`set -e
echo "### toolchain: $(rustc --version)"
echo '### feature-check: %[1]s'
echo '### %[2]s'
%[3]sset +e
out=$(%[2]s 2>&1); rc=$?
set -e
if [ "$rc" = "0" ]; then
  echo '%[4]s'
else
  printf '%%s\n' "$out"
  echo '%[5]s'
fi
exit 0`, c.name, cmd, setup, comboResultMarker(c.name, "OK"), comboResultMarker(c.name, "FAIL"))

	return m.featureBase(source, targetVol).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// featureMatrixLanes bounds how many combos compile concurrently in FeatureMatrix.
// Sized for the self-hosted box (i7-13700K, 24 threads, 31 GiB): `cargo check` is light
// (no final-binary codegen) and sccache (a shared volume) serves cross-lane object
// reuse, so 3 lanes peak well under the RAM ceiling — leaving headroom for a second leg
// running concurrently on a 2nd runner. Each lane gets its OWN target cache volume, so
// cargo's per-target build lock never serializes across lanes (the reason the previous
// design ran serially over one shared volume).
const featureMatrixLanes = 3

// FeatureMatrix runs every combo and returns a pass/fail summary. It fans the combos
// across featureMatrixLanes workers, each pinned to its own target cache volume, so the
// matrix is no longer serialized by the shared-volume cargo build lock. This is the
// single self-hosted `dagger-feature-matrix.yml` job (one GH status row); the per-combo
// pass/fail still appears in this report in declaration order, and a failure names every
// bad combo.
//
// fail-fast is OFF (every combo runs even after one fails), matching feature-flags.yml's
// `fail-fast: false`, so a single run reports the full matrix.
func (m *FraiseqlCi) FeatureMatrix(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	type comboResult struct {
		name string
		out  string
		err  error
	}

	jobs := make(chan featureCombo)
	results := make(chan comboResult)

	// Each lane owns a dedicated target volume; combos are pulled off a shared queue so
	// fast combos don't idle a lane waiting on a slow one (work-stealing, not round-robin).
	var wg sync.WaitGroup
	for lane := 0; lane < featureMatrixLanes; lane++ {
		wg.Add(1)
		go func(lane int) {
			defer wg.Done()
			targetVol := fmt.Sprintf("%s-lane-%d", featureTargetVol, lane)
			for c := range jobs {
				out, err := m.runCombo(ctx, source, c, targetVol)
				results <- comboResult{name: c.name, out: out, err: err}
			}
		}(lane)
	}

	go func() {
		for _, c := range featureCombos {
			jobs <- c
		}
		close(jobs)
	}()
	go func() {
		wg.Wait()
		close(results)
	}()

	reports := make(map[string]string)
	var failed []string
	for r := range results {
		switch {
		case r.err != nil:
			// Infra error (engine / Stdout), not a compile failure — runCombo's script
			// exits 0, so this is rare (e.g. the container couldn't start).
			failed = append(failed, r.name)
			reports[r.name] = fmt.Sprintf("\n===== %s (ERROR) =====\n%s\n%v\n", r.name, r.out, r.err)
		case !comboOK(r.name, r.out):
			failed = append(failed, r.name)
			reports[r.name] = fmt.Sprintf("\n===== %s (FAILED) =====\n%s\n", r.name, r.out)
		default:
			// Compact on success: the marker, not the (suppressed) cargo output.
			reports[r.name] = fmt.Sprintf("===== %s (ok) =====\n", r.name)
		}
	}

	// Emit in declaration order (stable, matches the combo table) regardless of the
	// order lanes finished in.
	var report strings.Builder
	for _, c := range featureCombos {
		report.WriteString(reports[c.name])
	}

	if len(failed) > 0 {
		sort.Strings(failed)
		fmt.Fprintf(&report, "\nfeature-matrix FAILED: %d/%d combo(s) failed: %s\n",
			len(failed), len(featureCombos), strings.Join(failed, ", "))
		// Every combo's WithExec exits 0 (pass/fail is in the COMBO-RESULT marker), so no
		// Dagger exec error shadows this — `dagger call` prints THIS report (each failed
		// combo's `===== name (FAILED) =====` block carries its full cargo error) as the
		// error, landing it cleanly at the tail of the CI log for `gh run view --log-failed`.
		return "", errors.New(report.String())
	}
	fmt.Fprintf(&report, "\nfeature-matrix OK: all %d combos passed across %d lanes\n",
		len(featureCombos), featureMatrixLanes)
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
