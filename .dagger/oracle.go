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
