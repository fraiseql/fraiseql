package main

import (
	"context"
	"errors"
	"fmt"
	"regexp"
	"strconv"
	"strings"
	"sync"

	"dagger/fraiseql-ci/internal/dagger"
)

// guardMutantsTargetVol names the guard-mutant sweep's target volumes, one per lane. They
// are apart from every other leg's: the sweep compiles its own builds (one per package and
// feature set its shards name), most of which no other leg compiles, and would evict
// theirs.
const guardMutantsTargetVol = "fraiseql-rust-target-guard-mutants"

// guardMutantsLaneEnv is set on a lane's services, so each lane binds servers of its own:
// two lanes binding one service definition would share one database.
const guardMutantsLaneEnv = "FRAISEQL_GUARD_MUTANTS_LANE"

// A lane's exec always exits 0, so its whole report comes back; the runner's exit code is
// read from this line.
var guardMutantsLaneRC = regexp.MustCompile(`(?m)^GUARD-MUTANTS-LANE-RC=(\d+)$`)

// guardMutantsChangedFile holds the `changed` paths in a lane, outside the tree it commits.
const guardMutantsChangedFile = "/tmp/guard-mutants-changed"

// guardMutantsShard is the form of a shard name, checked before it is put in a script.
var guardMutantsShard = regexp.MustCompile(`^[a-z0-9][a-z0-9-]*$`)

// GuardMutants runs the mutant of every guard site in tools/guard-sites.toml and fails
// unless each is killed by a test the site names (tools/run-guard-mutants.py, #1846).
//
// The sites run in `lanes` lanes side by side. tools/run-guard-mutants.py --lane splits
// them by the cargo build their shard compiles, so each build is compiled in one lane;
// each lane has its own target volume and its own Postgres (pgvector), Redis and NATS.
// Every lane runs to its end even when another fails, and the report names each lane's
// verdicts and its runtime.
func (m *FraiseqlCi) GuardMutants(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// How many lanes run side by side.
	// +optional
	// +default=2
	lanes int,
	// Only the sites of this shard (every site when empty).
	// +optional
	shard string,
	// Only the sites in these files: the paths a change touched, separated by
	// whitespace, as `git diff --name-only` prints them. The per-change run passes them,
	// since the source arrives without the history to compute them from.
	// +optional
	changed string,
) (string, error) {
	if lanes < 1 {
		return "", fmt.Errorf("lanes must be 1 or more, got %d", lanes)
	}
	if shard != "" && !guardMutantsShard.MatchString(shard) {
		return "", fmt.Errorf("shard %q is not a shard name", shard)
	}

	type laneResult struct {
		out string
		err error
	}
	results := make([]laneResult, lanes)
	var wg sync.WaitGroup
	for lane := 1; lane <= lanes; lane++ {
		wg.Add(1)
		go func(lane int) {
			defer wg.Done()
			out, err := m.guardMutantsLane(ctx, source, lane, lanes, shard, changed)
			results[lane-1] = laneResult{out: out, err: err}
		}(lane)
	}
	wg.Wait()

	var report strings.Builder
	var failed []string
	for i, result := range results {
		lane := fmt.Sprintf("%d/%d", i+1, lanes)
		fmt.Fprintf(&report, "\n===== guard mutants, lane %s =====\n%s\n", lane, result.out)
		rc := guardMutantsLaneRC.FindStringSubmatch(result.out)
		switch {
		case result.err != nil:
			failed = append(failed, lane)
			fmt.Fprintf(&report, "lane %s did not run: %v\n", lane, result.err)
		case rc == nil:
			failed = append(failed, lane)
			fmt.Fprintf(&report, "lane %s printed no exit code\n", lane)
		case rc[1] != "0":
			failed = append(failed, lane)
		}
	}
	if len(failed) > 0 {
		fmt.Fprintf(&report, "\nguard-mutants FAILED: lane(s) %s\n", strings.Join(failed, ", "))
		return "", errors.New(report.String())
	}
	fmt.Fprintf(&report, "\nguard-mutants OK: every selected mutant killed, %d lane(s)\n", lanes)
	return report.String(), nil
}

// guardMutantsLane runs lane `lane` of `lanes`. The source is mounted without `.git`, and
// the runner reads git (the files a shard run leaves untracked, and where it records a
// mutant in place), so the lane commits the tree it was given first.
func (m *FraiseqlCi) guardMutantsLane(
	ctx context.Context,
	source *dagger.Directory,
	lane, lanes int,
	shard, changed string,
) (string, error) {
	tag := strconv.Itoa(lane)
	postgres := m.pgContainer(source).WithEnvVariable(guardMutantsLaneEnv, tag).AsService()
	redis := dag.Container().
		From(redisImage).
		WithEnvVariable(guardMutantsLaneEnv, tag).
		WithExposedPort(6379).
		AsService()
	nats := dag.Container().
		From(natsImage).
		WithEnvVariable(guardMutantsLaneEnv, tag).
		WithExposedPort(4222).
		AsService(dagger.ContainerAsServiceOpts{UseEntrypoint: true, Args: []string{"-js", "-m", "8222"}})
	dbURL := fmt.Sprintf("postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, pgBindHost, pgDatabase)

	selection := fmt.Sprintf("--lane %d/%d", lane, lanes)
	if shard != "" {
		selection += " --shard " + shard
	}
	if changed != "" {
		selection += " --changed-files " + guardMutantsChangedFile
	}
	script := strings.Join([]string{
		"set -uo pipefail",
		"git init -q && git add -A && " +
			"git -c user.name=guard-mutants -c user.email=guard-mutants@example.invalid commit -q -m tree " +
			"|| { echo 'GUARD-MUTANTS-LANE-RC=2'; exit 0; }",
		"started=$(date +%s)",
		"python3 tools/run-guard-mutants.py " + selection,
		"rc=$?",
		fmt.Sprintf("echo \"lane %d/%d ran in $(( $(date +%%s) - started ))s\"", lane, lanes),
		"echo \"GUARD-MUTANTS-LANE-RC=${rc}\"",
	}, "\n")

	// psql: the runner asks the database whether it ships `vector` before any shard runs.
	base := m.rustBaseFor(rustMsrv).WithExec(aptInstall("postgresql-client"))
	return rustSource(base, source, laneVolume(guardMutantsTargetVol, lane)).
		WithServiceBinding(pgBindHost, postgres).
		WithServiceBinding(redisBindHost, redis).
		WithServiceBinding(natsBindHost, nats).
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("TEST_DATABASE_URL", dbURL).
		WithEnvVariable(requirePostgresEnv, "1").
		WithEnvVariable("REDIS_URL", fmt.Sprintf("redis://%s:6379", redisBindHost)).
		WithEnvVariable("NATS_URL", fmt.Sprintf("nats://%s:4222", natsBindHost)).
		WithNewFile(guardMutantsChangedFile, changed).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}
