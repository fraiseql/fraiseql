package main

// ── Delivery: the shipped image ANSWERS ───────────────────────────────────────
//
// image.go builds the published images and claims exactly "this Dockerfile
// builds". This file is what makes a built image MEAN something: it boots the
// artifact against a real PostgreSQL and requires an answer only a working
// engine can give.
//
// The bar is set by what the deleted jobs did NOT do (#1206):
//
//   - `test-images` built three images and asserted `docker image inspect`,
//     i.e. that the artifact exists. Compiling an artifact is not testing it.
//   - `verify-deployment` booted a Compose stack behind `|| true`, asserted
//     `{ __typename }` — which the GraphQL layer answers WITHOUT touching the
//     database — and piped the response to `jq .` without inspecting it, so an
//     `errors` payload passed.
//   - #1071: an image built green behind `|| true` and shipped with no compiled
//     schema at the path its own env names, so the container exited immediately
//     at schema validation while `docker compose build` and `up -d` both reported
//     success. That is the artifact class this tier is pointed at — and it is the
//     first RED proof below, reproduced with a bad FRAISEQL_SCHEMA_PATH.
//
// ⚠ The plan's Phase 03 attributed a different story to #1071 ("booted healthy
// and answered ordinary queries while refusing the one query that made it a
// subgraph"). The issue does not say that; it is the `|| true` / missing-schema
// finding above. Rule 1 — an issue's premise is a claim, not a fact — applies to
// this program's own plan too. The bar is unchanged; only the citation is.
//
// So this tier asks a question that has to reach Postgres, and then asks it a
// second time across a change to the database:
//
//	seed a FRESH schema  →  /health says database.connected  →  a query that
//	resolves through SQL to rows  →  INSERT a uniquely-named row  →  ask again
//	and REQUIRE the new row back.
//
// That last step is the whole point. Every assertion before it can be satisfied
// by a cached, replayed or fabricated response; only "mutate the world, re-ask,
// require the answer to change" cannot. Without it this leg is `verify-deployment`
// with better prose.

import (
	"context"
	"fmt"
	"strings"

	"dagger/fraiseql-ci/internal/dagger"
)

const (
	// imageBootBindHost — the service alias the BUILT IMAGE is reached at. It is
	// deliberately not serverBindHost: that alias is the cargo-built binary the
	// http-e2e integration leg drives, and sharing it would make a failure here
	// read as a failure there.
	imageBootBindHost = "fraiseql-image"

	// imageBootPgBindHost — a Postgres alias distinct from pgBindHost so this tier
	// can never be handed the integration seed by accident. Its schema is dropped
	// and rebuilt below, which would be a hostile thing to do to a shared service.
	imageBootPgBindHost = "postgres-image-boot"

	// imageBootPort is the port the image's own ENV binds and its own EXPOSE
	// declares. Nothing below sets FRAISEQL_BIND_ADDR — see the server
	// construction — so this constant is only how the CLIENT addresses the
	// service; that the IMAGE declares it is asserted by image_props.go.
	//
	// ⚠ It was 8815, matching a Dockerfile that named a port its own process
	// never listened on (#1216). This tier supplied FRAISEQL_BIND_ADDR=0.0.0.0:8815
	// and so agreed with the mistake for as long as it existed: a fixture that
	// agrees with the artifact cannot see the artifact's defect. That is why the
	// override is gone rather than merely corrected.
	imageBootPort = 8000

	// imageBootFixtureRows is the number of rows docker/e2e/init-postgres.sql
	// seeds into tb_user. Hardcoded on purpose: the point of the assertion is to
	// catch a fixture that half-applied, and a count derived from the file the
	// fixture also feeds would agree with itself no matter what ran. If the
	// fixture legitimately changes, this constant changes with it — an
	// acknowledgement, not drift.
	imageBootFixtureRows = 3
)

// ⚠ Why every function below takes a `runID`, measured 2026-08-27.
//
// Dagger caches a MODULE FUNCTION CALL on its arguments. Called twice against a
// byte-identical context, the second `dagger call image-boot` returned in 2.1s
// and printed the FIRST run's marker verbatim: the Go body never executed, so no
// container was started, no query was sent, and nothing was asserted — and it
// reported success. A cache-buster computed inside the function (`time.Now()`)
// cannot help, because the function is what is skipped.
//
// For `dagger call images` that replay is sound — a build is a pure function of
// its context, so "this context builds" stays true. For a tier whose claim is
// "the artifact ANSWERED", it is not: the claim is about an execution that did
// not happen. That is the shape of every job this program deleted.
//
// So the caller supplies a value that differs per run, and CI passes
// `${{ github.run_id }}-${{ github.run_attempt }}` — which makes a re-dispatch of
// an unchanged commit (the way anyone re-verifies after fixing infrastructure)
// execute for real rather than replay. The image BUILD inside stays cached, since
// `DockerBuild` is content-addressed independently of this argument; only the
// asking is forced.
//
// The `+default="local"` on each parameter below repeats this value because a
// Dagger annotation must be a literal; this const is the fallback the script path
// uses and the name to grep for.
const imageBootRunIDDefault = "local"

// bootableVariants are the variants this tier boots.
//
// DERIVED from the table in image.go rather than listed again: a second list of
// image names is exactly the drift `tools/check-image-parity.py` exists to
// prevent, and a new server variant added to the matrix must be booted without
// anyone remembering to add it here.
//
// The root `Dockerfile` builds the fraiseql-server binary; `tutorial/Dockerfile`
// is a node image serving the tutorial site, which speaks no GraphQL and has no
// database. It is built by `dagger call images` and is out of scope here.
func bootableVariants() []imageVariant {
	out := make([]imageVariant, 0, len(imageVariants))
	for _, v := range imageVariants {
		if v.dockerfile == "Dockerfile" {
			out = append(out, v)
		}
	}
	return out
}

// ImageBoots boots every bootable variant and is what the CI leg calls.
//
// Every variant is REQUIRED, including the ones docker-build.yml marks
// `optional` — the same deliberate divergence Images documents: `optional`
// exists so a broken best-effort image does not block the publish of a working
// one at tag time, and before the tag there is no publish to protect.
func (m *FraiseqlCi) ImageBoots(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// A value that DIFFERS between runs. Without it Dagger replays this
	// function's previous result without executing it — see imageBootRunIDDefault.
	// +optional
	// +default="local"
	runID string,
) (string, error) {
	variants := bootableVariants()
	if len(variants) == 0 {
		return "", fmt.Errorf(
			"no bootable image variant found in the imageVariants table — a tier that " +
				"boots nothing passes everything")
	}

	var report strings.Builder
	booted := 0
	for _, v := range variants {
		out, err := m.ImageBoot(ctx, source, v.name, runID)
		fmt.Fprintf(&report, "\n===== %s =====\n%s", v.name, out)
		if err != nil {
			fmt.Fprintf(&report, "FAILED: %v\n", err)
			return report.String(), fmt.Errorf(
				"image variant %q did not answer (%d of %d booted); docker-build.yml "+
					"marks it optional=%t, but this tier requires every published server "+
					"image to serve a real query before the tag: %w",
				v.name, booted, len(variants), v.optional, err)
		}
		booted++
	}

	names := make([]string, 0, len(variants))
	for _, v := range variants {
		names = append(names, v.name)
	}
	fmt.Fprintf(&report, "\nimage-boots OK: %d of %d bootable variant(s) answered a real query (%s)\n",
		booted, len(variants), strings.Join(names, ", "))
	return report.String(), nil
}

// ImageBoot boots one published image variant against a real PostgreSQL and
// requires it to answer a query that resolves through SQL to rows — then proves
// the answer is live by changing the database underneath it and asking again.
func (m *FraiseqlCi) ImageBoot(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// The variant to boot: one of the names in docker-build.yml's matrix that is
	// built from the root Dockerfile.
	// +optional
	// +default="fraiseql-server"
	variant string,
	// A value that DIFFERS between runs. Without it Dagger replays this
	// function's previous result without executing it — see imageBootRunIDDefault.
	// +optional
	// +default="local"
	runID string,
) (string, error) {
	v, err := lookupVariant(variant)
	if err != nil {
		return "", err
	}
	if v.dockerfile != "Dockerfile" {
		return "", fmt.Errorf(
			"variant %q is built from %s and does not serve GraphQL; this tier boots the "+
				"fraiseql-server images only (see bootableVariants)", v.name, v.dockerfile)
	}

	built, err := buildVariant(ctx, source, v)
	if err != nil {
		return "", err
	}

	// One Postgres, constructed once and bound to BOTH the server and the client.
	// Dagger content-addresses services, but relying on two identical definitions
	// resolving to one instance would make "the client wrote to the database the
	// server reads" an implementation detail of the engine rather than something
	// this code states. The discriminator is worthless if they are two databases.
	postgres := m.imageBootPgService()

	server := built.
		WithFile("/schema.compiled.json", source.File("docker/e2e/schema.compiled.json")).
		WithServiceBinding(imageBootPgBindHost, postgres).
		WithEnvVariable("DATABASE_URL", fmt.Sprintf(
			"postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, imageBootPgBindHost, pgDatabase)).
		WithEnvVariable("FRAISEQL_SCHEMA_PATH", "/schema.compiled.json").
		// No FRAISEQL_BIND_ADDR. DATABASE_URL and FRAISEQL_SCHEMA_PATH are supplied
		// by every deployment of this image and supplying them is faithful; the
		// address it listens on is the image's own business, and a tier that sets
		// it is a tier that agrees with itself about the one thing the image's
		// EXPOSE and HEALTHCHECK hardcode (#1216).
		WithEnvVariable("FRAISEQL_ENV", "development").
		WithEnvVariable("RUST_LOG", "info").
		WithExposedPort(imageBootPort).
		// No Args and no UseEntrypoint: the image is started by its OWN declared
		// CMD. Overriding it would boot a command the publish path never runs, which
		// is the class of hole where an image ships with a broken entrypoint and
		// every gate is green because every gate supplied its own.
		AsService()

	return m.imageBootClient(source, v).
		WithServiceBinding(imageBootPgBindHost, postgres).
		WithServiceBinding(imageBootBindHost, server).
		WithExec([]string{"bash", "-c", imageBootScript(v, runID)}).
		Stdout(ctx)
}

// imageBootPgService is a BARE postgres:16 — no /docker-entrypoint-initdb.d.
//
// ⚠ The fixture is loaded by the client below instead, under `ON_ERROR_STOP=1`,
// into a schema it drops first, and the row count is asserted afterwards. That is
// not ceremony. `docker/e2e/init-postgres.sql` is `CREATE TABLE IF NOT EXISTS`
// plus a bare INSERT, and measured against PG16 on 2026-08-27 it behaves three
// ways, two of them silent:
//
//	dirty + incompatible table, no ON_ERROR_STOP  →  psql exits 0, 0 rows inserted
//	the same load with ON_ERROR_STOP=1            →  psql exits 3
//	clean schema, fixture applied twice           →  psql exits 0, 6 rows
//
// Seeding through the Postgres entrypoint would hand this tier a database whose
// contents it never checked, and every assertion downstream would be an assertion
// about data nobody looked at. Note `release-smoke.yml` loads it the first way
// (#1214).
func (m *FraiseqlCi) imageBootPgService() *dagger.Service {
	return dag.Container().
		From(pgImage).
		WithEnvVariable("POSTGRES_USER", pgUser).
		WithEnvVariable("POSTGRES_PASSWORD", pgPassword).
		WithEnvVariable("POSTGRES_DB", pgDatabase).
		WithExposedPort(5432).
		AsService()
}

// imageBootClient is the container that drives the artifact: psql to change the
// database, curl to ask the engine, jq to read the answer.
//
// It is built from the pinned Postgres image (already mirrored, already pulled by
// this runner) plus curl and jq — deliberately NOT from the image under test. A
// client assembled inside the artifact would be testing the artifact with itself,
// and could not tell "the engine answered" from "the container has curl".
//
// jq rather than grep on the raw body: `grep -q '"name":"Alice"'` passes or fails
// on serializer whitespace, and a substring match on a JSON document is how an
// `errors` payload gets read as success.
func (m *FraiseqlCi) imageBootClient(source *dagger.Directory, v imageVariant) *dagger.Container {
	return dag.Container().
		From(pgImage).
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{"apt-get", "install", "-y", "--no-install-recommends", "curl", "jq", "ca-certificates"}).
		WithFile("/fixture/init-postgres.sql", source.File("docker/e2e/init-postgres.sql")).
		WithEnvVariable("PGPASSWORD", pgPassword).
		WithEnvVariable("IMAGE_VARIANT", v.name)
}

// imageBootScript is the tier itself. Written out here rather than assembled from
// fragments so the order of the argument — seed, health, query, MUTATE, re-query —
// is readable as one thing.
func imageBootScript(v imageVariant, runID string) string {
	if runID == "" {
		runID = imageBootRunIDDefault
	}
	replacer := strings.NewReplacer(
		"@PGHOST@", imageBootPgBindHost,
		"@PGUSER@", pgUser,
		"@PGDATABASE@", pgDatabase,
		"@BASE@", fmt.Sprintf("http://%s:%d", imageBootBindHost, imageBootPort),
		"@ROWS@", fmt.Sprintf("%d", imageBootFixtureRows),
		"@VARIANT@", v.name,
		"@RUNID@", sanitizeRunID(runID),
	)
	return replacer.Replace(imageBootScriptTemplate)
}

// sanitizeRunID keeps the run identifier to characters that survive a shell
// single-quoted string and a SQL literal. The value reaches both, and a caller
// passing `${{ github.ref_name }}` on a branch named with a quote should get a
// mangled marker rather than an injected statement.
func sanitizeRunID(runID string) string {
	var b strings.Builder
	for _, r := range runID {
		switch {
		case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z', r >= '0' && r <= '9', r == '-', r == '.':
			b.WriteRune(r)
		default:
			b.WriteRune('_')
		}
	}
	out := b.String()
	if len(out) > 48 {
		out = out[:48]
	}
	if out == "" {
		out = imageBootRunIDDefault
	}
	return out
}

const imageBootScriptTemplate = `
set -euo pipefail

PSQL="psql -h @PGHOST@ -U @PGUSER@ -d @PGDATABASE@ -v ON_ERROR_STOP=1"
BASE="@BASE@"

# A marker no cached, replayed or fabricated response can already contain: it is
# minted here, at exec time, and only reaches the database in step 5. It carries
# the caller's run id so the printed marker identifies WHICH run produced this
# output — the evidence that distinguishes a real execution from a replay.
MARKER="phase03-@VARIANT@-@RUNID@-$(date +%s%N)-$$"
echo "run: @RUNID@"

fail() { echo "IMAGE-BOOT FAILED (@VARIANT@): $*" >&2; exit 1; }

echo "### 1/6  wait for postgres"
for _ in $(seq 1 60); do
  if pg_isready -h @PGHOST@ -U @PGUSER@ -d @PGDATABASE@ >/dev/null 2>&1; then break; fi
  sleep 1
done
pg_isready -h @PGHOST@ -U @PGUSER@ -d @PGDATABASE@ \
  || fail "postgres never accepted connections"

echo "### 2/6  seed a FRESH schema under ON_ERROR_STOP=1"
# Drop and rebuild the schema first. The fixture is CREATE TABLE IF NOT EXISTS +
# a bare INSERT, which against a dirty database applies partially and reports
# success; from an empty schema the row count below is a real assertion.
$PSQL -q -c 'DROP SCHEMA IF EXISTS public CASCADE; CREATE SCHEMA public;'
$PSQL -q -f /fixture/init-postgres.sql
rows=$($PSQL -tAc 'SELECT count(*) FROM tb_user')
[ "$rows" = "@ROWS@" ] \
  || fail "fixture loaded $rows row(s) into tb_user, expected @ROWS@ (docker/e2e/init-postgres.sql)"
echo "seeded @ROWS@ row(s) into a freshly created schema"

echo "### 3/6  the image's own CMD serves /health, and it reached the database"
for _ in $(seq 1 60); do
  if curl -sf -o /tmp/health.json "$BASE/health" >/dev/null 2>&1; then break; fi
  sleep 1
done
code=$(curl -s -o /tmp/health.json -w '%{http_code}' "$BASE/health" || echo 000)
echo "GET /health -> HTTP $code"; cat /tmp/health.json; echo
[ "$code" = "200" ] || fail "/health returned HTTP $code (expected 200)"
# The point of this assertion is the DATABASE field, not the 200: /health answers
# 503 with a body when the pool is down, and a tier that only checked the status
# code would report "not healthy" for a dead engine and "healthy" for one that
# cannot reach Postgres at all.
jq -e '.database.connected == true' /tmp/health.json >/dev/null \
  || fail "/health does not report database.connected: true"
echo "database.connected: true"

echo "### 4/6  a GraphQL query that resolves THROUGH SQL to rows"
# { users { id name } } reads v_users' jsonb data column. Contrast { __typename },
# which the GraphQL layer answers without touching the database — that is what the
# deleted verify-deployment job asserted, and it is why this tier exists.
code=$(curl -sS -o /tmp/q1.json -w '%{http_code}' \
  -X POST "$BASE/graphql" \
  -H 'Content-Type: application/json' \
  -d '{"query":"{ users { id name } }"}')
echo "POST /graphql -> HTTP $code"; cat /tmp/q1.json; echo
[ "$code" = "200" ] || fail "/graphql returned HTTP $code (expected 200)"
if jq -e 'has("errors")' /tmp/q1.json >/dev/null; then
  fail "/graphql answered 200 with an errors payload"
fi
jq -e '[.data.users[].name] | sort == ["Alice","Bob","Charlie"]' /tmp/q1.json >/dev/null \
  || fail "/graphql did not return the seeded rows"
jq -e '[.data.users[].id] | all(type == "number")' /tmp/q1.json >/dev/null \
  || fail "/graphql returned users without numeric ids — the jsonb column did not resolve"
echo "the engine returned the @ROWS@ seeded rows"

echo "### 5/6  DISCRIMINATOR — mutate the world behind the engine"
# Everything above this line is satisfiable by a cached, replayed or fabricated
# response. This is the assertion that is not: the marker is minted in this
# process, written to Postgres by a client the server does not know about, and
# the engine has to go and find it.
if grep -q "$MARKER" /tmp/q1.json; then
  fail "the marker was already in the pre-insert response — it is not discriminating"
fi
$PSQL -q -c "INSERT INTO tb_user (name) VALUES ('$MARKER');"
after=$($PSQL -tAc 'SELECT count(*) FROM tb_user')
[ "$after" = "$((@ROWS@ + 1))" ] || fail "the INSERT did not land: tb_user holds $after row(s)"
echo "inserted $MARKER"

echo "### 6/6  ask again, and REQUIRE the new row back"
code=$(curl -sS -o /tmp/q2.json -w '%{http_code}' \
  -X POST "$BASE/graphql" \
  -H 'Content-Type: application/json' \
  -d '{"query":"{ users { id name } }"}')
echo "POST /graphql -> HTTP $code"; cat /tmp/q2.json; echo
[ "$code" = "200" ] || fail "/graphql returned HTTP $code on the re-query (expected 200)"
if jq -e 'has("errors")' /tmp/q2.json >/dev/null; then
  fail "/graphql answered 200 with an errors payload on the re-query"
fi
jq --arg m "$MARKER" -e '[.data.users[].name] | index($m) != null' /tmp/q2.json >/dev/null \
  || fail "the engine did not return the row inserted in step 5 — it served a stale or fabricated answer, not the database"
jq -e ".data.users | length == $((@ROWS@ + 1))" /tmp/q2.json >/dev/null \
  || fail "the re-query returned $(jq '.data.users | length' /tmp/q2.json) row(s), expected $((@ROWS@ + 1))"

echo
echo "image-boot OK (@VARIANT@): the built image booted on its own CMD, reached Postgres,"
echo "resolved a GraphQL query through SQL to rows, and returned a row inserted after it started."
`

// ── The tutorial image (#1221) ───────────────────────────────────────────────
//
// `tutorial` is one of the three variants docker-build.yml publishes to BOTH
// ghcr.io and Docker Hub, and it is built before the tag by `dagger call images`.
// Nothing ever started it: `bootableVariants` filters on `dockerfile ==
// "Dockerfile"`, so the boot tier above and the properties tier both skip it, and
// the chart and Compose tiers pin `--variant=fraiseql-server`. Of the three
// published images, two were booted against a real Postgres and the third was
// only asserted to build.
//
// The exclusion was defensible — it is a Node/Express site, not a fraiseql-server,
// and "resolve a GraphQL query through SQL" does not apply to it. But "those
// assertions do not apply" is not "nothing is asserted", and #1205's lesson was
// that an image nobody builds ships broken; this is one step along, an image
// nobody starts.
//
// So it gets a tier that fits WHAT IT IS: does the container start on its own CMD,
// does the `/health` its own HEALTHCHECK polls actually answer, and does the
// documented first thing a reader does — open the tutorial and get its chapters —
// return content rather than an empty shell.
const (
	// The port the tutorial image's own ENV and EXPOSE declare. Not overridden
	// here, for the #1216 reason: a tier that supplies the bind address agrees
	// with itself rather than with the image.
	tutorialPort     = 3001
	tutorialBindHost = "fraiseql-tutorial"
)

// TutorialImageBoot starts the published `tutorial` image and requires it to serve.
//
// No database and no fraiseql-server binding: the tutorial site renders its own
// chapter content, and its FRAISEQL_API_URL points at a server only for the live
// schema panes. Binding a fake one would assert something this tier is not about,
// and leaving it unset is the honest shape — the site must still start and serve
// its own content when no engine is reachable, which is exactly what a reader
// running `docker run` for the first time has.
func (m *FraiseqlCi) TutorialImageBoot(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// A value that DIFFERS between runs, for the same reason ImageBoot needs one:
	// without it Dagger replays the previous stdout without executing anything.
	// +optional
	// +default="local"
	runID string,
) (string, error) {
	v, err := lookupVariant("tutorial")
	if err != nil {
		return "", err
	}
	if v.dockerfile == "Dockerfile" {
		return "", fmt.Errorf(
			"the `tutorial` variant is now built from the root Dockerfile; it belongs to " +
				"ImageBoots, not to this tier — reconcile bootableVariants and delete this")
	}

	built, err := buildVariant(ctx, source, v)
	if err != nil {
		return "", err
	}

	site := built.
		WithExposedPort(tutorialPort).
		// Started by its OWN CMD, like ImageBoot: overriding it would boot a command
		// the publish path never runs.
		AsService()

	if runID == "" {
		runID = imageBootRunIDDefault
	}

	// The client is NOT built from the image under test — a curl that came from the
	// artifact could not tell "the site answered" from "the container has curl".
	script := strings.NewReplacer(
		"@BASE@", fmt.Sprintf("http://%s:%d", tutorialBindHost, tutorialPort),
		"@RUNID@", runID,
	).Replace(`set -euo pipefail
fail() { echo "TUTORIAL-BOOT FAILED: $*" >&2; exit 1; }
BASE="@BASE@"
echo "### run @RUNID@"

echo "### 1/3  wait for the site to accept a connection on its own port"
for i in $(seq 1 60); do
  curl -sS -o /dev/null "$BASE/health" 2>/dev/null && break
  [ "$i" = "60" ] && fail "the tutorial image never accepted a connection on its own EXPOSEd port"
  sleep 1
done

echo "### 2/3  /health — the endpoint the image's OWN HEALTHCHECK polls"
code=$(curl -sS -o /tmp/h.json -w '%{http_code}' "$BASE/health")
echo "GET /health -> HTTP $code"; cat /tmp/h.json; echo
[ "$code" = "200" ] || fail "/health returned HTTP $code (expected 200) — the image's own HEALTHCHECK would never pass"
jq -e '.status == "healthy"' /tmp/h.json >/dev/null \
  || fail "/health did not report status=healthy"
jq -e '.service == "fraiseql-tutorial"' /tmp/h.json >/dev/null \
  || fail "/health answered, but not as the tutorial service — something else is on this port"

echo "### 3/3  the documented first thing a reader does: open it, and get chapters"
code=$(curl -sS -o /tmp/c.json -w '%{http_code}' "$BASE/api/chapters")
echo "GET /api/chapters -> HTTP $code"
[ "$code" = "200" ] || fail "/api/chapters returned HTTP $code (expected 200)"
# Assert CONTENT, not shape. An empty array is valid JSON and is exactly what a
# tutorial image with its assets left out of the build context would serve — the
# #1214 shape, where a 200 with nothing in it read as success.
n=$(jq '[.. | objects | select(has("id") and has("title"))] | length' /tmp/c.json)
echo "chapters carrying an id and a title: $n"
[ "$n" -gt 0 ] || fail "/api/chapters answered 200 with no chapter carrying an id and a title — the site starts and has no content"

code=$(curl -sS -o /tmp/index.html -w '%{http_code}' "$BASE/")
echo "GET / -> HTTP $code"
[ "$code" = "200" ] || fail "/ returned HTTP $code (expected 200) — the web UI is not served"
[ -s /tmp/index.html ] || fail "/ returned an empty body"

echo
echo "tutorial-image-boot OK: the image booted on its own CMD, answered the /health its"
echo "own HEALTHCHECK polls, served $n chapter(s) of real content, and served its web UI."
`)

	return dag.Container().
		From(pgImage).
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{"apt-get", "install", "-y", "--no-install-recommends", "curl", "jq", "ca-certificates"}).
		WithServiceBinding(tutorialBindHost, site).
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
}
