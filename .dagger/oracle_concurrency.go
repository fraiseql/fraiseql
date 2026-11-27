package main

import (
	"context"
	"fmt"
	"strings"

	"dagger/fraiseql-ci/internal/dagger"
)

// concurrencyOracleFeatures is the server build the concurrency oracle needs: the
// observer runtime its subscription cells read from.
// SYNC: the Makefile's ORACLE_CONCURRENCY_FEATURES.
const concurrencyOracleFeatures = "cli,observers"

// concurrencyUvVersion pins the uv that installs the oracle's locked Python environment
// (tools/oracle/concurrency/uv.lock, interpreter from .python-version).
const concurrencyUvVersion = "0.9.18"

// concurrencyStandbyBindHost is the oracle's own streaming standby of the primary it binds.
const concurrencyStandbyBindHost = "postgres-oracle-standby"

// ConcurrencyOracle runs the concurrency oracle (tools/oracle/concurrency): sequential
// cells for the result cache, the mutation path, Idempotency-Key and read replicas, and
// concurrent register, pagination and subscription runs judged by a linearizability-lite
// checker. Every cell that does not hold must match tools/oracle/concurrency/baseline.json,
// in both directions.
//
// `selftest` runs the oracle's own RED proof instead: the checker on hand-built histories
// and each gate site on a baseline differing by one entry. The product mutants
// (tools/oracle/concurrency/mutants.sh) rebuild the server once per mutant and run locally
// only.
//
// The server is built with concurrencyOracleFeatures; the CLI is the plain build. The
// oracle runs on shellBase plus a pinned uv. Postgres is the seeded PostgreSQL 18 service
// the integration legs bind, which the repo's replication init prepares for streaming,
// with one real standby of it (pgStandbyService, on its own slot) for the replica cells.
// The oracle creates and drops databases of its own on the primary.
func (m *FraiseqlCi) ConcurrencyOracle(
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
	replicaURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, concurrencyStandbyBindHost, pgDatabase)
	args := ""
	if selftest {
		args = " --selftest"
	}
	script := strings.Join([]string{
		"set -e",
		"python3 -m venv /tmp/uv",
		"/tmp/uv/bin/pip install --quiet uv==" + concurrencyUvVersion,
		"export PATH=/tmp/uv/bin:$PATH",
		"echo \"### uv: $(uv --version)\"",
		"echo '### oracle: concurrency linearizability'",
		"bash tools/oracle/concurrency/run.sh" + args,
	}, "\n")

	// One primary, bound to both the oracle and its standby: a second pgService(source)
	// would leave the standby streaming from a database the oracle never writes to.
	primary := m.pgService(source)
	bins := m.concurrencyOracleBinaries(source, lane)
	return m.shellBase().
		WithExec(aptInstall("curl", "python3-venv")).
		WithFile("/usr/local/bin/fraiseql-cli", bins.File("fraiseql-cli")).
		WithFile("/usr/local/bin/fraiseql-server", bins.File("fraiseql-server")).
		WithServiceBinding(pgBindHost, primary).
		WithServiceBinding(concurrencyStandbyBindHost, m.pgStandbyService(source, primary, "oracle_concurrency")).
		WithEnvVariable("FRAISEQL_CLI", "/usr/local/bin/fraiseql-cli").
		WithEnvVariable("FRAISEQL_SERVER", "/usr/local/bin/fraiseql-server").
		WithEnvVariable("ORACLE_ADMIN_URL", adminURL).
		WithEnvVariable("ORACLE_PRIMARY_URL", adminURL).
		WithEnvVariable("ORACLE_REPLICA_URL", replicaURL).
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}

// concurrencyOracleBinaries builds the CLI and a server with the observer runtime.
func (m *FraiseqlCi) concurrencyOracleBinaries(source *dagger.Directory, lane int) *dagger.Directory {
	script := strings.Join([]string{
		"set -e",
		"echo \"### toolchain: $(rustc --version)\"",
		"bash tools/ci-target-canary.sh -- build -p fraiseql-cli --bin fraiseql-cli", // the target-cache canary wraps the build
		"cargo build -p fraiseql-server --bin fraiseql-server --features " + concurrencyOracleFeatures,
		"mkdir -p /out",
		"cp target/debug/fraiseql-cli target/debug/fraiseql-server /out/",
	}, "\n")
	return m.integrationBase(source, rustMsrv, lane).
		WithExec([]string{"bash", "-c", script}).
		Directory("/out")
}
