#!/usr/bin/env bash
# compose-stack-test.sh — the canonical Compose stack comes up on the image this
# branch builds, and answers a question only a working engine can answer.
#
# ── Why this exists ──────────────────────────────────────────────────────────
#
# Six operator-facing Compose stacks shipped in this repository. Measured
# 2026-08-28, **not one of the six could serve a query**, and nothing in CI
# would have said so — the only job that ever ran a compose stack was
# `verify-deployment` in docker-build.yml, which was structurally unreachable
# (deleted in #1206), ran `docker compose up -d 2>&1 || true`, and asserted
# `{ __typename }` — a query the GraphQL layer answers without touching the
# database.
#
# The blocker they all shared was neither of the two issues filed against them:
#
#   * NOT ONE set FRAISEQL_ENV or mounted a fraiseql.toml. Unset FRAISEQL_ENV
#     means production; cors_enabled defaults true and cors_origins defaults
#     empty, so the server exits on its first line with "cors_enabled is true but
#     cors_origins is empty in production mode" — and cors_origins has NO
#     environment variable. Measured: this fires BEFORE the schema check, so
#     #1202's fix (compile the schema first) would have changed nothing
#     observable; it only moves the error.
#
# Beyond that: the two root files mounted no compiled schema at all;
# `docker/docker-compose.{demo,examples}.yml` built an admin-dashboard/Dockerfile
# that exists nowhere (#1189); `docker/docker-compose.prod{,-examples}.yml` pulled
# `fraiseql/dashboard:latest`, never published (`docker manifest inspect` →
# "no such manifest"); all four `docker/` stacks pointed FRAISEQL_SCHEMA_PATH at a
# gitignored artifact no step builds (#1202); all four ran
# `graphql/graphql-playground`, a repository Docker Hub no longer serves; and the
# root production template bind-mounted `./tools/prometheus.yml`, which does not
# exist (the real file is deploy/docker/prometheus.yml).
#
# Five of the six were deleted rather than repaired. This gate covers the one
# that remains, to the bar the rest of this program sets: end in a question only
# a WORKING artifact can answer, and include a DISCRIMINATING check — change the
# world, ask again, require the answer to change.
#
# ── What this supplies, and what it deliberately does not ────────────────────
#
# It supplies exactly what an operator supplies: a database password, a path to a
# compiled schema, and a path to a fraiseql.toml. Nothing else.
#
# It does NOT supply the port, the bind address, or a healthcheck. Those belong
# to the image, and a gate that supplies them agrees with the image by
# construction and can never disagree with it. That is precisely how the boot
# tier stayed green through the six months the published image was UNHEALTHY
# forever, its EXPOSE and HEALTHCHECK naming 8815 while the process listened on
# 8000 (#1216): the tier set FRAISEQL_BIND_ADDR itself. A4 below asserts that the
# compose file does not restate either of them, and B2 asserts the image really
# does carry the healthcheck whose absence A4 requires — otherwise "nobody
# declares a healthcheck" would pass on a stack that has none at all.
#
# ── Why host docker, and not a `dagger call` ─────────────────────────────────
#
# `docker compose` needs a docker daemon, and the daemon Dagger runs execs
# against is not one a compose project can be created in. Same resolution as
# tools/chart-deploy-test.sh: the orchestration runs under host docker, and the
# IMAGE still comes from Dagger — `dagger call image-tarball` exports what
# `buildVariant` builds, so this is not a second copy of the build arguments.
#
# ── Usage ────────────────────────────────────────────────────────────────────
#
#   tools/compose-stack-test.sh --run-id <id> --image-tarball <path> [--keep]
#
# `--run-id` is required and must DIFFER between runs. It names the compose
# project and the image tag, so two runs cannot collide and a stale container
# from a previous run cannot be silently reused, and it is stamped into the
# marker row so the printed evidence identifies which run produced it.
set -euo pipefail

# ── Subject and fixtures ─────────────────────────────────────────────────────

COMPOSE_FILE="docker-compose.yml"

# The fixture, shared with the Phase 03 image-boot tier and the Phase 05 chart
# tier so all three assert against the same rows.
FIXTURE_SQL="docker/e2e/init-postgres.sql"
FIXTURE_SCHEMA="docker/e2e/schema.compiled.json"
# Row count docker/e2e/init-postgres.sql seeds. Hardcoded for the reason
# .dagger/image_boot.go gives: a count derived from the file the fixture also
# feeds agrees with itself no matter what actually ran.
FIXTURE_ROWS=3

# Repositories this project publishes a server image to. Same list as
# tools/chart-deploy-test.sh.
PUBLISHED_IMAGES="ghcr.io/fraiseql/server fraiseql/server"

# The service under test and the database service. The PORTS are derived from the
# compose file itself (A2), never assumed here: a constant would make this tier agree
# with a number rather than with the artifact, which is the #1216 shape one level up.
APP_SERVICE="fraiseql"
DB_SERVICE="postgres"

# The host side of each published port is `${VAR:-default}` in the compose file, so a
# collision on a shared host does not make the stack unstartable. These are the names
# of those variables; B1 uses them ONLY when the default is already taken, and says so.
APP_HOST_PORT_VAR="FRAISEQL_HOST_PORT"
DB_HOST_PORT_VAR="FRAISEQL_DB_HOST_PORT"

# Every compose file in the repository, classified. A file that is neither the
# canonical stack nor a CI-driven rig must SAY it is not CI-verified, in its own
# header, where the person copying it will read it.
#
# The classification is checked for exhaustiveness in A5: a compose file that
# matches nothing here fails the gate rather than being skipped. That is the
# manifest pattern check-suite-coverage.py established, scoped to compose.
CANONICAL="docker-compose.yml"
# Rigs CI actually drives. Each names what drives it, so an entry that stops
# being true is visible to the next reader.
declare -A CI_DRIVEN=(
  # `make db-up`, every local integration run, and .dagger/main.go's pgService
  # mounts the same tests/sql/postgres/*.sql this file mounts.
  ["docker/docker-compose.test.yml"]="make db-up + the Dagger integration legs"
)
# The header line a non-canonical, non-CI stack must carry.
UNVERIFIED_MARKER="Not CI-verified"

# ── Arguments ────────────────────────────────────────────────────────────────

RUN_ID=""
IMAGE_TARBALL=""
KEEP=0

while [ $# -gt 0 ]; do
  case "$1" in
    --run-id)          RUN_ID="${2:-}"; shift 2 ;;
    --run-id=*)        RUN_ID="${1#*=}"; shift ;;
    --image-tarball)   IMAGE_TARBALL="${2:-}"; shift 2 ;;
    --image-tarball=*) IMAGE_TARBALL="${1#*=}"; shift ;;
    --keep)            KEEP=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[ -n "$RUN_ID" ] || { echo "ERROR: --run-id is required (and must differ between runs)." >&2; exit 2; }
[ -n "$IMAGE_TARBALL" ] || { echo "ERROR: --image-tarball is required. Produce it with:
  dagger call image-tarball --source=. --variant=fraiseql-server export --path=<path>" >&2; exit 2; }

if repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then cd "$repo_root"; fi

[ -f "$IMAGE_TARBALL" ] || { echo "ERROR: image tarball not found: $IMAGE_TARBALL" >&2; exit 2; }
for f in "$COMPOSE_FILE" "$FIXTURE_SQL" "$FIXTURE_SCHEMA"; do
  [ -f "$f" ] || { echo "ERROR: required file not found: $f" >&2; exit 2; }
done
command -v docker >/dev/null || { echo "ERROR: docker is required (see the header)." >&2; exit 2; }
command -v python3 >/dev/null || { echo "ERROR: python3 is required." >&2; exit 2; }

# Sanitised for use in a compose project name (lowercase, alnum/dash) and a SQL
# literal.
SLUG="$(printf '%s' "$RUN_ID" | tr 'A-Z' 'a-z' | tr -c 'a-z0-9-' '-' | sed 's/^-*//' | cut -c1-40)"
PROJECT="fq-compose-${SLUG}"

WORKDIR="$(mktemp -d)"
ENV_FILE="$WORKDIR/.env"
CONFIG_TOML="$WORKDIR/fraiseql.toml"

# The server configuration an operator must supply. Written here rather than
# committed as a fixture on purpose: it is operator input, like the database
# password, not an artifact of this repository.
cat > "$CONFIG_TOML" <<'TOML'
cors_origins = ["http://localhost"]
TOML

DB_PASSWORD="compose-stack-${SLUG}"

# Empty until B1 finds the shipped default host port already taken; write_env reads
# both, so they must exist before the first call under `set -u`.
APP_HOST_PORT_OVERRIDE=""
DB_HOST_PORT_OVERRIDE=""
PREEXISTING_IMAGE_ID=""

failures=0
step() { printf '\n### %s\n' "$*"; }
fail() { echo "COMPOSE-STACK FAILED: $*" >&2; failures=$((failures + 1)); }
die()  { echo "COMPOSE-STACK FAILED: $*" >&2; exit 1; }

# Compose, always with the same project and env-file. `--env-file` rather than
# the ambient `.env`: a developer's own `.env` in the repo root would otherwise
# silently become part of what is under test.
dc() { docker compose -p "$PROJECT" -f "$COMPOSE_FILE" --env-file "$ENV_FILE" "$@"; }

cleanup() {
  local rc=$?
  if [ "$rc" -ne 0 ] || [ "$failures" -ne 0 ]; then
    echo
    echo "───── diagnostics (the run did not pass) ─────"
    dc ps -a 2>&1 | head -20 || true
    echo "--- ${APP_SERVICE} logs ---"
    dc logs --no-color --tail=100 "$APP_SERVICE" 2>&1 || true
    echo "--- ${DB_SERVICE} logs (tail) ---"
    dc logs --no-color --tail=30 "$DB_SERVICE" 2>&1 || true
    if cid="$(dc ps -aq "$APP_SERVICE" 2>/dev/null)" && [ -n "$cid" ]; then
      echo "--- ${APP_SERVICE} state ---"
      docker inspect -f 'exit={{.State.ExitCode}} health={{if .State.Health}}{{.State.Health.Status}}{{else}}<none>{{end}}' "$cid" 2>&1 || true
      echo "--- healthcheck log ---"
      docker inspect -f '{{if .State.Health}}{{range .State.Health.Log}}{{.ExitCode}} {{.Output}}{{end}}{{end}}' "$cid" 2>&1 | tail -10 || true
    fi
  fi
  if [ "$KEEP" -eq 1 ]; then
    echo
    echo "--keep: stack left running. project: $PROJECT  env-file: $ENV_FILE"
    return
  fi
  dc down -v --remove-orphans --timeout 10 >/dev/null 2>&1 || true
  # ⚠ The host tag too. B2 `docker load`s the artifact and tags it, and without this
  # every run leaves a ~110 MiB image tag behind on the runner — invisible until the
  # disk fills, which on this box has already once been misread as a toolchain fault.
  #
  # ⚠⚠ But the tag is a REAL published name, and after the release it will exist on
  # any machine that has pulled it. Removing it unconditionally would delete an image
  # this script did not create. B2 records what was there first, and it is put back.
  if [ -n "${IMAGE_REF:-}" ]; then
    if [ -n "${PREEXISTING_IMAGE_ID:-}" ]; then
      docker tag "$PREEXISTING_IMAGE_ID" "$IMAGE_REF" >/dev/null 2>&1 || true
    else
      docker rmi -f "$IMAGE_REF" >/dev/null 2>&1 || true
    fi
  fi
  rm -rf "$WORKDIR" || true
}
trap cleanup EXIT

echo "compose-stack-test: run ${RUN_ID}"
echo "  stack:   $COMPOSE_FILE"
echo "  image:   $IMAGE_TARBALL"
echo "  project: $PROJECT"

# ─────────────────────────────────────────────────────────────────────────────
# Phase A — offline. Cheap, and it fails before a container is ever started.
# ─────────────────────────────────────────────────────────────────────────────

# The three inputs an operator must supply, and nothing more.
write_env() {
  : > "$ENV_FILE"
  [ "${1:-}" = "no-password" ] || echo "DB_PASSWORD=$DB_PASSWORD" >> "$ENV_FILE"
  [ "${1:-}" = "no-schema" ]   || echo "FRAISEQL_SCHEMA_FILE=$PWD/$FIXTURE_SCHEMA" >> "$ENV_FILE"
  [ "${1:-}" = "no-config" ]   || echo "FRAISEQL_CONFIG_FILE=$CONFIG_TOML" >> "$ENV_FILE"
  # Set only when B1 found the shipped default already taken on this host.
  [ -z "${APP_HOST_PORT_OVERRIDE:-}" ] || echo "${APP_HOST_PORT_VAR}=$APP_HOST_PORT_OVERRIDE" >> "$ENV_FILE"
  [ -z "${DB_HOST_PORT_OVERRIDE:-}" ]  || echo "${DB_HOST_PORT_VAR}=$DB_HOST_PORT_OVERRIDE" >> "$ENV_FILE"
}

# `env -u` so an ambient value on the runner cannot satisfy a variable this check
# is trying to leave unset. Compose reads the shell environment ahead of
# --env-file, so without this the negative cases below could silently pass.
compose_config() {
  env -u DB_PASSWORD -u FRAISEQL_SCHEMA_FILE -u FRAISEQL_CONFIG_FILE \
    docker compose -p "$PROJECT" -f "$COMPOSE_FILE" --env-file "$ENV_FILE" config "$@"
}

step "A1/A6  the stack REFUSES to come up without each input, with an instruction"
# The compose analogue of the chart's `fail` guards. An unset input must abort
# with a message naming it — not start a container that exits, which is what all
# six previous stacks did.
declare -A MISSING=(
  ["no-password"]="DB_PASSWORD"
  ["no-schema"]="FRAISEQL_SCHEMA_FILE"
  ["no-config"]="FRAISEQL_CONFIG_FILE"
)
guards_checked=0
for case_name in "${!MISSING[@]}"; do
  var="${MISSING[$case_name]}"
  guards_checked=$((guards_checked + 1))
  write_env "$case_name"
  if compose_config >"$WORKDIR/$case_name.out" 2>&1; then
    fail "$COMPOSE_FILE resolved with $var unset. It must refuse: a stack that starts without it runs a container that exits, which is the shape every deleted stack had."
    continue
  fi
  if grep -q "$var" "$WORKDIR/$case_name.out"; then
    echo "  $var unset -> refused, naming the variable"
  else
    fail "$COMPOSE_FILE refused with $var unset, but the message does not name $var:"
    sed 's/^/       /' "$WORKDIR/$case_name.out" >&2
  fi
done
# Counts, not exit codes.
[ "$guards_checked" -eq "${#MISSING[@]}" ] \
  || die "A1 declared ${#MISSING[@]} guard(s) but checked $guards_checked"

write_env
resolved="$WORKDIR/resolved.json"
if ! compose_config --format json > "$resolved" 2>"$WORKDIR/resolve.err"; then
  sed 's/^/       /' "$WORKDIR/resolve.err" >&2
  die "$COMPOSE_FILE does not resolve with all three inputs supplied"
fi
echo "  all three supplied -> resolves"

WORKSPACE_VERSION="$(sed -n '/^\[workspace\.package\]/,/^\[/p' Cargo.toml | sed -nE 's/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' | head -1)"
[ -n "$WORKSPACE_VERSION" ] || die "could not read the workspace version out of Cargo.toml"

step "A2/A6  every image is pinned, and the server image is one this project publishes"
# #1129's class, in compose: `fraiseql/dashboard:latest` resolved to a repository
# that has never existed, and two shipped stacks pulled it. An image reference
# nobody can pull is a stack nobody can start.
IMAGE_REF="$(python3 - "$resolved" "$APP_SERVICE" <<'PY'
import json, sys
cfg = json.load(open(sys.argv[1]))
print(cfg["services"][sys.argv[2]].get("image", ""))
PY
)"
[ -n "$IMAGE_REF" ] || die "the $APP_SERVICE service declares no image"
python3 - "$resolved" "$APP_SERVICE" "$WORKSPACE_VERSION" "$PUBLISHED_IMAGES" <<'PY' || fail "image references are not all pinned/publishable (see above)"
import json, sys
cfg, app, version, published = json.load(open(sys.argv[1])), sys.argv[2], sys.argv[3], sys.argv[4].split()
bad = 0
services = cfg.get("services", {})
if not services:
    print("FAIL: the resolved config has no services — this check would be vacuous")
    raise SystemExit(1)
for name, svc in sorted(services.items()):
    ref = svc.get("image")
    if not ref:
        print(f"FAIL: service {name} declares no image (a build: here would mean the gate"
              f" tests a second build rather than the shipped artifact)")
        bad += 1
        continue
    repo, _, tag = ref.rpartition(":")
    if not repo or "/" in tag:
        print(f"FAIL: service {name} image {ref!r} is untagged — pin a version")
        bad += 1
        continue
    if tag == "latest":
        print(f"FAIL: service {name} pins {ref!r} to :latest — pin a version for reproducible deploys")
        bad += 1
        continue
    print(f"  {name}: {ref}")
    if name == app:
        if repo not in published:
            print(f"FAIL: the {app} service names repository {repo!r}, which this project does not"
                  f" publish. Published: {', '.join(published)}. (A bare name like 'fraiseql'"
                  f" resolves to docker.io/library/fraiseql and pulls nothing.)")
            bad += 1
        if tag != version:
            print(f"FAIL: the {app} service pins tag {tag!r} but the workspace is at {version!r}."
                  f" A stack that names a version this repository is not building is a stack"
                  f" whose image does not exist yet (#1129).")
            bad += 1
raise SystemExit(1 if bad else 0)
PY

step "A3/A6  every bind-mount source exists"
# Docker creates a DIRECTORY for a missing bind-mount source and mounts that.
# The deleted root production template mounted ./tools/prometheus.yml, which does
# not exist — the real file is deploy/docker/prometheus.yml — and nothing caught
# it, because tools/check-examples-integrity.sh scans `find docker examples` and
# the root compose files are outside its scope. Same shape as #1052 and #1213.
mount_count="$(python3 - "$resolved" <<'PY'
import json, os, sys
cfg = json.load(open(sys.argv[1]))
n = 0
for name, svc in sorted(cfg.get("services", {}).items()):
    for vol in svc.get("volumes", []) or []:
        if isinstance(vol, dict):
            if vol.get("type") != "bind":
                continue
            src = vol.get("source", "")
        else:
            src = str(vol).split(":", 1)[0]
            if not src.startswith(("/", "./", "../")):
                continue
        n += 1
        if not os.path.exists(src):
            print(f"FAIL: service {name} bind-mounts {src!r}, which does not exist."
                  f" Docker would create an empty DIRECTORY here and mount that.")
            raise SystemExit(1)
        print(f"  {name}: {src}")
print(f"COUNT={n}")
PY
)" || { printf '%s\n' "$mount_count" >&2; fail "a bind-mount source does not exist"; }
printf '%s\n' "$mount_count" | grep -v '^COUNT=' || true
mounts="$(printf '%s\n' "$mount_count" | sed -n 's/^COUNT=//p')"
[ "${mounts:-0}" -ge 2 ] \
  || die "A3 found ${mounts:-0} bind mount(s); the stack mounts a schema and a config, so a
       lower number means the extractor matched nothing and this check is vacuous"

step "A4/A6  the published ports name a fixed container port"
# Derived from the resolved config, never assumed: a constant here would make this
# tier agree with a number instead of with the artifact. The CONTAINER side must also
# be a LITERAL in the file — parameterising it would let a deployment move the port
# the image's process binds and the port its HEALTHCHECK addresses out from under
# each other, which is #1216 with a variable in front of it.
ports_json="$(python3 - "$resolved" "$APP_SERVICE" "$DB_SERVICE" <<'PYPORTS'
import json, sys
cfg, app, db = json.load(open(sys.argv[1])), sys.argv[2], sys.argv[3]
out = {}
for svc_name, key in ((app, "app"), (db, "db")):
    ports = cfg["services"][svc_name].get("ports") or []
    if len(ports) != 1:
        print(f"FAIL: service {svc_name} publishes {len(ports)} port(s); expected exactly 1", file=sys.stderr)
        raise SystemExit(1)
    p = ports[0]
    if isinstance(p, dict):
        out[key] = {"host": str(p.get("published")), "container": str(p.get("target"))}
    else:
        bits = str(p).split(":")
        out[key] = {"host": bits[-2], "container": bits[-1]}
print(json.dumps(out))
PYPORTS
)" || die "could not read the published ports out of the resolved config"
read_port() { printf '%s' "$ports_json" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['$1']['$2'])"; }
APP_HOST_PORT="$(read_port app host)"
APP_PORT="$(read_port app container)"
DB_HOST_PORT="$(read_port db host)"
DB_PORT="$(read_port db container)"
case "$APP_PORT" in ''|*[!0-9]*) die "the $APP_SERVICE container port resolved to '$APP_PORT'" ;; esac
echo "  $APP_SERVICE: host ${APP_HOST_PORT} -> container ${APP_PORT}"
echo "  $DB_SERVICE: host ${DB_HOST_PORT} -> container ${DB_PORT}"
# The RAW file, not the resolved config: after resolution a parameterised container
# port is indistinguishable from a literal one.
if grep -nE '^[[:space:]]*-[[:space:]]*"[^"]*\$\{[^}]*\}"[[:space:]]*$' "$COMPOSE_FILE" >"$WORKDIR/varports.out"; then
  fail "$COMPOSE_FILE ends a published port on a variable, so the CONTAINER side is
       parameterised. That half is the port the image binds and the port its
       HEALTHCHECK addresses; it must be a literal, or a deployment can move the two
       apart (#1216):"
  sed 's/^/       /' "$WORKDIR/varports.out" >&2
fi
# The shipped default must be the simple case: an operator who sets nothing gets
# host == container. B1 moves the host side only when the default is already taken.
[ "$APP_HOST_PORT" = "$APP_PORT" ] \
  || fail "the default host port ($APP_HOST_PORT) differs from the container port ($APP_PORT).
       An operator who sets nothing should get the straightforward mapping."

step "A4b/A6 the stack does not restate what the IMAGE owns"
# The #1216 discipline, made mechanical. A gate that supplies the bind address
# and the healthcheck agrees with the image by construction; it cannot report
# that the image binds a port its own HEALTHCHECK does not address. B2 asserts
# the image really does carry a healthcheck, so this absence means "the image's,
# unmodified" rather than "none at all".
python3 - "$resolved" "$APP_SERVICE" <<'PY' || fail "the stack restates image-owned configuration (see above)"
import json, sys
cfg, app = json.load(open(sys.argv[1])), sys.argv[2]
svc = cfg["services"][app]
bad = 0
if svc.get("healthcheck"):
    print(f"FAIL: the {app} service declares its own healthcheck. The image carries one;"
          f" restating it here means this gate can never disagree with the image, which is"
          f" how the published image stayed UNHEALTHY forever (#1216).")
    bad += 1
env = svc.get("environment") or {}
if isinstance(env, list):
    env = dict(e.split("=", 1) for e in env if "=" in e)
if "FRAISEQL_BIND_ADDR" in env:
    print(f"FAIL: the {app} service sets FRAISEQL_BIND_ADDR. The image sets it"
          f" (0.0.0.0:8000); overriding it here means the published port mapping is tested"
          f" against this file's opinion rather than against the artifact (#1216).")
    bad += 1
print(f"  no healthcheck override, no FRAISEQL_BIND_ADDR override")
raise SystemExit(1 if bad else 0)
PY

step "A5/A6  every compose file in the repository is classified"
# One canonical stack, and everything else says what it is. A reader copying a
# stack out of this repository must be able to tell whether anything has ever run
# it. Exhaustive by construction: a compose file matching no rule FAILS.
discovered=0
classified=0
while IFS= read -r f; do
  discovered=$((discovered + 1))
  if [ "$f" = "$CANONICAL" ]; then
    classified=$((classified + 1)); echo "  canonical:     $f"; continue
  fi
  if [ -n "${CI_DRIVEN[$f]:-}" ]; then
    classified=$((classified + 1)); echo "  CI-driven:     $f  (${CI_DRIVEN[$f]})"; continue
  fi
  if head -25 "$f" | grep -qF "$UNVERIFIED_MARKER"; then
    classified=$((classified + 1)); echo "  unverified:    $f"; continue
  fi
  fail "$f is neither the canonical stack nor a CI-driven rig, and its header does not say \"$UNVERIFIED_MARKER\".
       Add that line to its header, or add it to CI_DRIVEN here naming what drives it.
       A stack a reader may copy must say whether anything has ever run it."
done < <(find . \( -name 'docker-compose*.yml' -o -name 'docker-compose*.yaml' \
                -o -name 'compose.yml' -o -name 'compose.yaml' \) \
           -not -path './target/*' -not -path './.git/*' -not -path '*/node_modules/*' \
         | sed 's|^\./||' | sort)
[ "$discovered" -gt 1 ] || die "A5 discovered $discovered compose file(s) — the search matched almost nothing, so this check is vacuous"
[ "$classified" -eq "$discovered" ] \
  || fail "A5 discovered $discovered compose file(s) and classified $classified"
echo "  $classified/$discovered classified"

step "A6/A6  nothing in this tier swallows a failure"
# #1071's shape, at the level of the tier itself: the deleted `verify-deployment`
# job ran `docker compose up -d 2>&1 || true`, so a stack that never came up
# reported success. The success criterion for this phase is "no || true
# anywhere"; this is that criterion, checked rather than promised.
swallow=0
for f in tools/compose-stack-test.sh Makefile .github/workflows/dagger-image.yml; do
  [ -f "$f" ] || continue
  case "$f" in
    Makefile)  block="$(sed -n '/^compose-stack:/,/^$/p' "$f")" ;;
    *.yml)     block="$(sed -n '/compose-stack-test\.sh/,/^$/p' "$f")" ;;
    *)         block="$(grep -nE 'docker compose' "$f" || true)" ;;
  esac
  if printf '%s' "$block" | grep -qE '\|\|[[:space:]]*(true|:)[[:space:]]*$'; then
    fail "$f swallows a failure in the compose-stack path:"
    printf '%s\n' "$block" | grep -nE '\|\|[[:space:]]*(true|:)[[:space:]]*$' | sed 's/^/       /' >&2
    swallow=$((swallow + 1))
  fi
done
[ "$swallow" -eq 0 ] && echo "  no '|| true' in the make target, the workflow step, or this script's compose calls"

if [ "$failures" -ne 0 ]; then
  die "$failures offline check(s) failed; not starting a stack."
fi

# ─────────────────────────────────────────────────────────────────────────────
# Phase B — a real stack. Everything above is satisfiable by a compose file that
# cannot start a container; this is the part that is not.
# ─────────────────────────────────────────────────────────────────────────────

step "B1/B7  rig check (a rig fault must not read as an artifact defect)"
docker compose version >/dev/null 2>&1 \
  || die "RIG FAILURE: 'docker compose' (v2) is not available. This tier does not use docker-compose v1."
command -v curl >/dev/null \
  || die "RIG FAILURE: curl is required — the stack is queried through its PUBLISHED host port, which is the thing under test."

# ⚠ The probe runs in a SUBSHELL, and fd 3 is never opened in this one. An earlier
# version closed it here with `exec 3>&- 3<&- 2>/dev/null`; `exec` with no command
# makes its redirections PERMANENT, so that silently sent the rest of the script's
# stderr to /dev/null — including the die() that was about to fire. The failure then
# printed nothing at all.
port_free() { ! (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }

# The CONTAINER port is fixed and is what is under test. The HOST port is not: a
# collision on a shared runner is a property of the host, and failing on it would
# make this tier permanently red anywhere something already listens on 8000. The
# shipped default is used whenever it is free, so the ordinary CI run exercises the
# literal mapping an operator gets.
# ⚠ CLAIMED, not merely probed, and assigned through a NAMED VARIABLE rather than a
# command substitution. Nothing listens on a port this picks until `docker compose up`
# binds it, so a second call would otherwise hand out the same free port again —
# measured: both services were assigned 20000. A command substitution would have put
# `claimed_ports` in a subshell and lost it, which is the same bug wearing a hat.
claimed_ports=""
pick_host_port() {   # pick_host_port <out-var> <preferred> <service-name>
  local __out="$1" preferred="$2" name="$3" p
  case " $claimed_ports " in *" $preferred "*) ;; *)
    if port_free "$preferred"; then
      claimed_ports="$claimed_ports $preferred"
      printf -v "$__out" '%s' "$preferred"; return
    fi ;;
  esac
  for p in $(seq 20000 20040); do
    case " $claimed_ports " in *" $p "*) continue ;; esac
    if port_free "$p"; then
      echo "  note: host port $preferred is already in use; publishing $name on $p instead"
      claimed_ports="$claimed_ports $p"
      printf -v "$__out" '%s' "$p"; return
    fi
  done
  die "RIG FAILURE: host port $preferred is in use for $name and no free port was found in 20000-20040."
}
pick_host_port APP_HOST_PORT "$APP_HOST_PORT" "$APP_SERVICE"
pick_host_port DB_HOST_PORT "$DB_HOST_PORT" "$DB_SERVICE"
[ "$APP_HOST_PORT" = "$APP_PORT" ] || APP_HOST_PORT_OVERRIDE="$APP_HOST_PORT"
[ "$DB_HOST_PORT" = "$DB_PORT" ]   || DB_HOST_PORT_OVERRIDE="$DB_HOST_PORT"
write_env
echo "  docker compose v2 present, curl present, publishing ${APP_HOST_PORT}->${APP_PORT} and ${DB_HOST_PORT}->${DB_PORT}"
dc down -v --remove-orphans --timeout 10 >/dev/null 2>&1 || true

step "B2/B7  load the artifact under test under the tag the stack names"
# Loaded, never pulled: before the tag there is no published
# fraiseql/server:<version> to pull, and pulling a previous release would assert
# something about an artifact nobody is shipping. Tagging the tarball as the
# reference the compose file names means the stack under test resolves to the
# image THIS BRANCH built, with the compose file unmodified.
# What this tag pointed at before, if anything, so cleanup can put it back rather
# than delete an image belonging to whoever ran this.
PREEXISTING_IMAGE_ID="$(docker image inspect -f '{{.Id}}' "$IMAGE_REF" 2>/dev/null || true)"
[ -z "$PREEXISTING_IMAGE_ID" ] \
  || echo "  note: $IMAGE_REF already exists here; it is restored on exit"
loaded="$(docker load -i "$IMAGE_TARBALL" 2>&1 | tail -1)"
echo "  docker load: $loaded"
image_id="$(printf '%s' "$loaded" | sed -nE 's/.*(Loaded image ID|Loaded image): *//p')"
[ -n "$image_id" ] || die "could not read an image id out of 'docker load' output: $loaded"
docker tag "$image_id" "$IMAGE_REF"
echo "  tagged as $IMAGE_REF"
# A4 asserted the compose file declares no healthcheck. That is only meaningful
# if the image carries one — otherwise the stack has no healthcheck at all and
# both checks pass on nothing.
img_health="$(docker inspect -f '{{if .Config.Healthcheck}}{{.Config.Healthcheck.Test}}{{end}}' "$IMAGE_REF")"
[ -n "$img_health" ] \
  || die "the image carries NO HEALTHCHECK, so A4's 'the stack must not declare one' would leave
       the stack with none at all. Either the image regressed or A4 must change."
echo "  the image's own healthcheck: $img_health"
# The image and the compose file must agree about the container port, and NEITHER
# side is supplied by this script: the port comes from the compose file (A4), the
# ExposedPorts from the built artifact. This is #1216 stated directly — an image
# whose EXPOSE named a port its process never listened on, published by compose
# files that named a third.
img_exposed="$(docker inspect -f '{{range $p, $_ := .Config.ExposedPorts}}{{$p}} {{end}}' "$IMAGE_REF")"
case " $img_exposed " in
  *" ${APP_PORT}/tcp "*) echo "  the image EXPOSEs ${APP_PORT}/tcp, which is the port the stack maps" ;;
  *) fail "the image EXPOSEs '$img_exposed' but $COMPOSE_FILE maps container port ${APP_PORT}.
       An image whose EXPOSE names a port the stack does not map — or does not bind —
       publishes a dead port and can never become healthy (#1216)." ;;
esac

step "B3/B7  bring up the database service"
dc up -d "$DB_SERVICE"
db_cid="$(dc ps -q "$DB_SERVICE")"
[ -n "$db_cid" ] || die "the $DB_SERVICE service did not produce a container"
db_ready=0
for _ in $(seq 1 90); do
  st="$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$db_cid" 2>/dev/null || echo gone)"
  [ "$st" = "healthy" ] && { db_ready=1; break; }
  [ "$(docker inspect -f '{{.State.Running}}' "$db_cid" 2>/dev/null)" = "true" ] \
    || die "the $DB_SERVICE container exited before becoming healthy"
  sleep 2
done
[ "$db_ready" -eq 1 ] || die "the $DB_SERVICE container never reported healthy"
echo "  $DB_SERVICE healthy"

step "B4/B7  seed a FRESH schema under ON_ERROR_STOP=1, and count the rows"
# Drop and rebuild first, and assert the count. docker/e2e/init-postgres.sql is
# CREATE TABLE IF NOT EXISTS plus a bare INSERT: against a dirty database it
# applies partially and reports success (#1214). From an empty schema the count
# below is a real assertion.
psql_db() { dc exec -T "$DB_SERVICE" psql -U fraiseql -d fraiseql -v ON_ERROR_STOP=1 "$@"; }
psql_db -q -c 'DROP SCHEMA IF EXISTS public CASCADE; CREATE SCHEMA public;' >/dev/null
psql_db -q -f - < "$FIXTURE_SQL" >/dev/null
rows="$(psql_db -tAc 'SELECT count(*) FROM tb_user' | tr -d '[:space:]')"
[ "$rows" = "$FIXTURE_ROWS" ] \
  || die "fixture loaded $rows row(s) into tb_user, expected $FIXTURE_ROWS ($FIXTURE_SQL)"
echo "  seeded $FIXTURE_ROWS row(s) into a freshly created schema"

step "B5/B7  bring up the whole stack, and wait for the IMAGE's healthcheck"
# Not "the container is running" — healthy, on the healthcheck baked into the
# image. That is the assertion the published image could not have satisfied for
# six months (#1216), and the one `docker image inspect` in the deleted
# test-images job could never have made.
dc up -d
app_cid="$(dc ps -q "$APP_SERVICE")"
[ -n "$app_cid" ] || die "the $APP_SERVICE service did not produce a container"
app_healthy=0
for _ in $(seq 1 90); do
  st="$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$app_cid" 2>/dev/null || echo gone)"
  [ "$st" = "healthy" ] && { app_healthy=1; break; }
  if [ "$(docker inspect -f '{{.State.Running}}' "$app_cid" 2>/dev/null)" != "true" ]; then
    echo "--- $APP_SERVICE logs ---"; dc logs --no-color --tail=60 "$APP_SERVICE" 2>&1 || true
    die "the $APP_SERVICE container EXITED before becoming healthy (exit $(docker inspect -f '{{.State.ExitCode}}' "$app_cid" 2>/dev/null))"
  fi
  sleep 2
done
[ "$app_healthy" -eq 1 ] || die "the $APP_SERVICE container never reported healthy on the image's own healthcheck"
echo "  $APP_SERVICE healthy on the image's own healthcheck"

step "B6/B7  the stack answers through its PUBLISHED port"
# 127.0.0.1:8000 from the host — the operator's vantage point, and the one that
# exercises the port mapping. A container that serves only inside its own network
# namespace passes every in-container check and publishes a dead port (#1216).
BASE="http://127.0.0.1:${APP_HOST_PORT}"
# ⚠ Never a bare `$(curl …)`: under `set -e` a connection failure aborts the script
# with curl's exit code and no message of ours — measured, the expose-mismatch proof
# exited 56 silently. This reports the transport failure as a named one.
req() {   # req <out-file> <label> [curl args...]
  local out="$1" label="$2"; shift 2
  local code rc
  code="$(curl -sS -o "$out" -w '%{http_code}' "$@" 2>"$WORKDIR/curl.err")" && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    sed 's/^/       /' "$WORKDIR/curl.err" >&2
    die "$label: the request never completed (curl exit $rc). The stack is not reachable
       at $BASE — a published port that routes nowhere is exactly what #1216 shipped."
  fi
  printf '%s' "$code"
}
code="$(req "$WORKDIR/health.json" "GET /health" "$BASE/health")"
echo "  GET /health -> HTTP $code"; sed 's/^/    /' "$WORKDIR/health.json"; echo
[ "$code" = 200 ] || die "/health did not answer 200 through the published port"
python3 -c "
import json,sys
d=json.load(open(sys.argv[1]))
c=d.get('database',{}).get('connected')
sys.exit(0 if c is True else print(f'FAIL: /health reports database.connected={c!r}') or 1)
" "$WORKDIR/health.json" || die "/health does not report database.connected: true"

code="$(req "$WORKDIR/ready.json" "GET /readiness" "$BASE/readiness")"
echo "  GET /readiness -> HTTP $code"
[ "$code" = 200 ] || die "/readiness did not answer 200"

code="$(req "$WORKDIR/q1.json" "POST /graphql" -X POST "$BASE/graphql" \
  -H 'Content-Type: application/json' -d '{"query":"{ users { id name } }"}')"
echo "  POST /graphql -> HTTP $code"; sed 's/^/    /' "$WORKDIR/q1.json"; echo
[ "$code" = 200 ] || die "/graphql did not answer 200"
python3 -c "
import json,sys
d=json.load(open(sys.argv[1]))
if 'errors' in d: print('FAIL: /graphql answered 200 with an errors payload:', d['errors']); sys.exit(1)
names=sorted(u['name'] for u in d['data']['users'])
if names != ['Alice','Bob','Charlie']:
    print(f'FAIL: /graphql returned {names!r}, not the seeded rows'); sys.exit(1)
" "$WORKDIR/q1.json" || die "/graphql did not resolve through SQL to the seeded rows"
echo "  the stack returned the $FIXTURE_ROWS seeded rows"

step "B7/B7  DISCRIMINATOR — change the database, ask again, require the answer to change"
# Everything above is satisfiable by a container serving a fixed or cached
# response. This is not: the marker is minted here, written to Postgres by a
# client the running server knows nothing about, and the engine has to go and
# find it.
MARKER="phase06-${SLUG}-$(date +%s%N)-$$"
grep -q "$MARKER" "$WORKDIR/q1.json" 2>/dev/null \
  && die "the marker already appears in the first answer — it is not discriminating"
psql_db -q -c "INSERT INTO tb_user (name) VALUES ('$MARKER');" >/dev/null
after="$(psql_db -tAc 'SELECT count(*) FROM tb_user' | tr -d '[:space:]')"
[ "$after" = "$((FIXTURE_ROWS + 1))" ] || die "the INSERT did not land: tb_user holds $after row(s)"
echo "  inserted $MARKER"

code="$(req "$WORKDIR/q2.json" "POST /graphql (re-query)" -X POST "$BASE/graphql" \
  -H 'Content-Type: application/json' -d '{"query":"{ users { id name } }"}')"
echo "  POST /graphql -> HTTP $code"; sed 's/^/    /' "$WORKDIR/q2.json"; echo
[ "$code" = 200 ] || die "/graphql did not answer 200 on the re-query"
python3 -c "
import json,sys
d=json.load(open(sys.argv[1])); marker=sys.argv[2]; expected=int(sys.argv[3])
if 'errors' in d: print('FAIL: errors payload on the re-query:', d['errors']); sys.exit(1)
names=[u['name'] for u in d['data']['users']]
if marker not in names:
    print('FAIL: the stack did not return the row inserted after it started serving —'
          ' it served a stale or fabricated answer, not the database'); sys.exit(1)
if len(names) != expected:
    print(f'FAIL: the re-query returned {len(names)} rows, expected {expected}'); sys.exit(1)
" "$WORKDIR/q2.json" "$MARKER" "$((FIXTURE_ROWS + 1))" \
  || die "the discriminator failed — the running stack is not reading the database"
echo "  the stack returned the row inserted after it was already serving"

if [ "$failures" -ne 0 ]; then
  die "$failures check(s) failed."
fi

echo
echo "compose-stack OK (run ${RUN_ID}): the canonical stack refuses each missing input,"
echo "names only pinned images this project publishes at the workspace version, restates"
echo "nothing the image owns, came up on the image this branch builds, became healthy on"
echo "the image's OWN healthcheck, answered through its published host port, and returned"
echo "a row inserted after it was already serving."
