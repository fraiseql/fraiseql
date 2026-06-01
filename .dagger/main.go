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
	rustImage = "rust:1.92"
	// unwrapAllowLimit mirrors the ci.yml clippy job's `make lint-unwrap UNWRAP_ALLOW_LIMIT=3`.
	unwrapAllowLimit = "3"
	// sccacheVersion pins the prebuilt sccache binary fetched into rustBase.
	sccacheVersion = "v0.8.2"
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
		From("ubuntu:24.04").
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
		"bash tools/check-test-imports.sh",
		"bash tools/check-route-syntax.sh",
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
		From("ubuntu:24.04").
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{
			"apt-get", "install", "-y", "--no-install-recommends",
			"make", "git", "gawk", "findutils", "grep", "ca-certificates",
		})
}
