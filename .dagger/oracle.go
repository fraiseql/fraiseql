package main

import (
	"context"
	"fmt"
	"strings"

	"dagger/fraiseql-ci/internal/dagger"
)

// GraphqlJsOracle runs the graphql-js differential oracle (tools/oracle/graphql-js):
// FraiseQL and graphql-js, the GraphQL reference implementation, answer the same
// targeted and seeded-random documents over the same schema and rows, and every
// disagreement must match tools/oracle/graphql-js/baseline.json exactly. A new
// disagreement fails the leg, and so does a recorded one that no longer occurs, until
// the baseline is re-blessed in the same change.
//
// `selftest` runs the oracle's own RED proof instead: each gate site must fail when the
// baseline is changed by one entry, and a proxy that makes the server execute one class
// of invalid document, or return one wrong value, must be reported as exactly that.
//
// The binaries come from smokeBinaries. The oracle runs on shellBase plus the distro's
// node, so it adds no base image. Postgres is the same seeded PostgreSQL 18 service the
// integration legs bind; the oracle creates and loads a database of its own in it.
func (m *FraiseqlCi) GraphqlJsOracle(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git", "**/node_modules"]
	source *dagger.Directory,
	// +optional
	// +default=false
	selftest bool,
	// The runner's lane; see TestIntegration.
	// +optional
	// +default=0
	lane int,
) (string, error) {
	if lane < 0 {
		return "", fmt.Errorf("lane must be 0 or more, got %d", lane)
	}
	adminURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	args := ""
	if selftest {
		args = " --selftest"
	}
	script := strings.Join([]string{
		"set -e",
		"echo \"### node: $(node --version) / npm: $(npm --version)\"",
		"echo '### oracle: graphql-js differential'",
		"bash tools/oracle/graphql-js/run.sh" + args,
	}, "\n")

	bins := m.smokeBinaries(source, lane)
	return m.shellBase().
		WithExec(aptInstall("nodejs", "npm", "curl")).
		WithFile("/usr/local/bin/fraiseql-cli", bins.File("fraiseql-cli")).
		WithFile("/usr/local/bin/fraiseql-server", bins.File("fraiseql-server")).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithEnvVariable("FRAISEQL_CLI", "/usr/local/bin/fraiseql-cli").
		WithEnvVariable("FRAISEQL_SERVER", "/usr/local/bin/fraiseql-server").
		WithEnvVariable("ORACLE_ADMIN_URL", adminURL).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// transportsOracleFeatures is the server build the cross-transport oracle needs: every
// transport it compares, and the observer runtime the WS column's events come from.
// SYNC: the Makefile's ORACLE_TRANSPORTS_FEATURES.
const transportsOracleFeatures = "cli,rest,grpc,mcp,arrow,observers"

// transportsUvVersion pins the uv that installs the oracle's locked Python environment
// (tools/oracle/transports/uv.lock, interpreter from .python-version).
const transportsUvVersion = "0.9.18"

// TransportsOracle runs the cross-transport differential oracle (tools/oracle/transports):
// every request of a seeded corpus is sent over POST /graphql, GET, SSE, REST, MCP, Arrow
// Flight and WebSocket subscriptions, and each cell is compared with POST. Every
// disagreement must match tools/oracle/transports/baseline.json exactly, in both
// directions, and each row of the Transports table in docs/graphql-conformance.md must hold
// on both sides of its boundary.
//
// `selftest` runs the oracle's own RED proof instead: each gate site fails when the
// baseline changes by one entry, a proxy that changes one column's answers is reported on
// that column only, tampered subscription events are reported on WS only, and a server
// variant's settings are proven to apply.
//
// The server is built with transportsOracleFeatures; the CLI is the plain build. The
// oracle runs on shellBase plus a pinned uv, which installs the locked environment.
// Postgres is the same seeded PostgreSQL 18 service the integration legs bind; the oracle
// creates and drops a database of its own in it.
func (m *FraiseqlCi) TransportsOracle(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git", "**/node_modules", "**/.venv"]
	source *dagger.Directory,
	// +optional
	// +default=false
	selftest bool,
	// The runner's lane; see TestIntegration.
	// +optional
	// +default=0
	lane int,
) (string, error) {
	if lane < 0 {
		return "", fmt.Errorf("lane must be 0 or more, got %d", lane)
	}
	adminURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)
	args := ""
	if selftest {
		args = " --selftest"
	}
	script := strings.Join([]string{
		"set -e",
		"python3 -m venv /tmp/uv",
		"/tmp/uv/bin/pip install --quiet uv==" + transportsUvVersion,
		"export PATH=/tmp/uv/bin:$PATH",
		"echo \"### uv: $(uv --version)\"",
		"echo '### oracle: cross-transport differential'",
		"bash tools/oracle/transports/run.sh" + args,
	}, "\n")

	bins := m.transportsOracleBinaries(source, lane)
	return m.shellBase().
		WithExec(aptInstall("curl", "python3-venv")).
		WithFile("/usr/local/bin/fraiseql-cli", bins.File("fraiseql-cli")).
		WithFile("/usr/local/bin/fraiseql-server", bins.File("fraiseql-server")).
		WithServiceBinding(pgBindHost, m.pgService(source)).
		WithEnvVariable("FRAISEQL_CLI", "/usr/local/bin/fraiseql-cli").
		WithEnvVariable("FRAISEQL_SERVER", "/usr/local/bin/fraiseql-server").
		WithEnvVariable("ORACLE_ADMIN_URL", adminURL).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// transportsOracleBinaries builds the CLI and a server with every transport the
// cross-transport oracle compares.
func (m *FraiseqlCi) transportsOracleBinaries(source *dagger.Directory, lane int) *dagger.Directory {
	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"bash tools/ci-target-canary.sh -- build -p fraiseql-cli --bin fraiseql-cli", // #880 canary wraps the build
		"cargo build -p fraiseql-server --bin fraiseql-server --features " + transportsOracleFeatures,
		"mkdir -p /out",
		"cp target/debug/fraiseql-cli target/debug/fraiseql-server /out/",
	}, "\n")
	return m.integrationBase(source, rustMsrv, lane).
		WithExec([]string{"bash", "-c", script}).
		Directory("/out")
}
