#!/usr/bin/env bash
# chart-deploy-test.sh — the Helm chart resolves to an image that exists, and
# deploys into a real cluster that answers a real query.
#
# ── Why this exists ──────────────────────────────────────────────────────────
#
# `helm.yml` ran `helm lint` and `helm template … > /dev/null`. A lint never
# resolves an image and a discarded render never runs, which is how the chart
# shipped `repository: fraiseql` — docker.io/library/fraiseql, the Docker Hub
# official-images namespace this project cannot publish to — for several releases
# (#1129).
#
# Fixing the image reference alone would have left the chart just as unusable.
# Measured 2026-08-27 against the chart as it then stood, `helm install` on the
# default values could not start a pod for FOUR independent reasons, none of
# which a lint, a render, or an image pull can see:
#
#   1. DATABASE_URL was mounted from a Secret `<release>-fraiseql-db` that no
#      template created. Default install → CreateContainerConfigError, forever.
#   2. No FRAISEQL_SCHEMA_PATH and no mounted schema. The published image bakes
#      none, so the container exits at startup validation (the #1071 shape).
#   3. containerPort and all three probes on 8815, against an image that binds
#      0.0.0.0:8000. Every probe refused → startupProbe exhausts 30 attempts →
#      CrashLoopBackOff. That is #1216, still live in the chart after the image
#      itself had been corrected.
#   4. `ingress.enabled`, `podDisruptionBudget.enabled`, `serviceAccount.create`
#      and `persistence.enabled` rendered NOTHING. A render with all four set to
#      true was byte-identical to the default render — and `helm.yml`'s own "all
#      features enabled" step set `ingress.enabled=true` and passed on it.
#
# So the bar here is the same one Phase 03 set for the image: end in a question
# only a WORKING artifact can answer, and include a discriminating check —
# change the world, ask again, require the answer to change.
#
# ── Why this is a shell script driven by host docker, and not a Dagger tier ───
#
# Not preference; a measured limit. A kubelet cannot run inside a Dagger exec on
# this engine. Measured 2026-08-27: an exec's `/sys/fs/cgroup/cgroup.controllers`
# is EMPTY, because the engine container's own cgroup root has an empty
# `cgroup.subtree_control` and therefore delegates nothing to the cgroups
# buildkit creates for execs. k3s reaches "Starting k3s v1.36.3+k3s1" and then
# exits with `failed to find cpu cgroup (v2)`. No in-container fix exists —
# delegation must come from the parent, and mutating a live CI engine's cgroups
# is exactly the kind of host state that makes a gate green on one box and red on
# a fresh runner. A privileged container under host docker DOES get delegation
# (verified: node Ready, real pod Running), so the cluster runs there.
#
# The artifact still comes from Dagger: `dagger call image-tarball` exports the
# image `buildVariant` builds — the same construction site as `images`,
# `image-boots` and `image-properties`. A `docker build` here would have been
# shorter and would have been a FOURTH copy of the build arguments, which is the
# drift `tools/check-image-parity.py` exists to prevent.
#
# ── Usage ────────────────────────────────────────────────────────────────────
#
#   tools/chart-deploy-test.sh --run-id <id> --image-tarball <path> [--keep]
#
# `--run-id` is required and must DIFFER between runs. It names every docker
# object this script creates, so two runs cannot collide, and it is stamped into
# the marker row so the printed evidence identifies which run produced it. It is
# the same discipline `--run-id` enforces for the Dagger tiers, for a different
# reason: there, Dagger replays a function call it has seen the arguments for;
# here, a stale container from a previous run would be silently reused.
#
# `--keep` leaves the cluster up for debugging. Off by default: the trap removes
# every object this script created, including on failure.
set -euo pipefail

# ── Pins ─────────────────────────────────────────────────────────────────────

# k3s, pinned by tag AND digest. This is the one image in this repository's CI
# that is not pulled from the ghcr.io/fraiseql/* mirror, because it is pulled by
# the HOST docker rather than by the Dagger engine, and the mirror exists to give
# the engine authenticated, rate-limit-free pulls. The digest pin gives the same
# supply-chain property the mirror would. Override with FRAISEQL_K3S_IMAGE.
K3S_IMAGE="${FRAISEQL_K3S_IMAGE:-rancher/k3s:v1.36.3-k3s1@sha256:d0f79175794edd9694b4a12bafc5c52ae1977369a2f7cf256264e7bd2dae0be9}"

# Postgres: the mirrored plain postgres:16, not the pgvector mirror the Dagger
# rig uses. docker/e2e/init-postgres.sql needs no extension, and this image is
# imported into the cluster layer by layer — a smaller one is a faster import.
PG_IMAGE="${FRAISEQL_PG_IMAGE:-ghcr.io/fraiseql/postgres:16}"

# helm, checksum-pinned exactly as tools' gitleaks is (.dagger/security.go).
# helm 3 rather than 4: this is an `apiVersion: v2` chart, `helm.yml` has always
# pinned a 3.x, and moving the repository to helm 4 is a decision with its own
# blast radius that does not belong inside a gate.
HELM_VERSION="v3.21.4"
HELM_SHA256="61f88ab166748cb19604d7884cb100ae9ccb13804ddeb98e08af167eacbb6a14"

# The fixture, shared with the Phase 03 image-boot tier so both tiers assert
# against the same rows.
FIXTURE_SQL="docker/e2e/init-postgres.sql"
FIXTURE_SCHEMA="docker/e2e/schema.compiled.json"
# Row count docker/e2e/init-postgres.sql seeds. Hardcoded for the reason
# .dagger/image_boot.go gives: a count derived from the file the fixture also
# feeds agrees with itself no matter what actually ran.
FIXTURE_ROWS=3

# Images the chart is allowed to name by default. Same list as
# tools/check-deploy-versions.sh, which greps values.yaml; this one reads the
# RENDER, so an image introduced by a template rather than by values.yaml is
# still caught.
PUBLISHED_IMAGES="ghcr.io/fraiseql/server fraiseql/server"

PG_USER="fraiseql_test"
PG_PASSWORD="fraiseql_test_password"
PG_DATABASE="test_fraiseql"

CHART_DIR="deploy/kubernetes/helm/fraiseql"

# ── Arguments ────────────────────────────────────────────────────────────────

RUN_ID=""
IMAGE_TARBALL=""
KEEP=0

while [ $# -gt 0 ]; do
  case "$1" in
    --run-id)        RUN_ID="${2:-}"; shift 2 ;;
    --run-id=*)      RUN_ID="${1#*=}"; shift ;;
    --image-tarball) IMAGE_TARBALL="${2:-}"; shift 2 ;;
    --image-tarball=*) IMAGE_TARBALL="${1#*=}"; shift ;;
    --chart)         CHART_DIR="${2:-}"; shift 2 ;;
    --chart=*)       CHART_DIR="${1#*=}"; shift ;;
    --keep)          KEEP=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[ -n "$RUN_ID" ] || { echo "ERROR: --run-id is required (and must differ between runs)." >&2; exit 2; }
[ -n "$IMAGE_TARBALL" ] || { echo "ERROR: --image-tarball is required. Produce it with:
  dagger call image-tarball --source=. --variant=fraiseql-server export --path=<path>" >&2; exit 2; }
[ -f "$IMAGE_TARBALL" ] || { echo "ERROR: image tarball not found: $IMAGE_TARBALL" >&2; exit 2; }

if repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then cd "$repo_root"; fi

for f in "$CHART_DIR/Chart.yaml" "$CHART_DIR/values.yaml" "$FIXTURE_SQL" "$FIXTURE_SCHEMA"; do
  [ -f "$f" ] || { echo "ERROR: required file not found: $f" >&2; exit 2; }
done
command -v docker >/dev/null || { echo "ERROR: docker is required (see the header: the cluster cannot run inside Dagger)." >&2; exit 2; }

# Sanitised for use in docker object names and a SQL literal.
SLUG="$(printf '%s' "$RUN_ID" | tr -c 'a-zA-Z0-9._-' '_' | cut -c1-40)"
K3S_NAME="fraiseql-chart-k3s-${SLUG}"
CLIENT_IMAGE="fraiseql-chart-client:${SLUG}"
# The tag the chart is pointed at. Deliberately NOT a published name: the image
# under test is the one this branch builds, and before the tag exists there is no
# published `ghcr.io/fraiseql/server:<version>` to pull. That the DEFAULT values
# name a published repository is asserted separately, offline, in check A1.
LOCAL_IMAGE_REPO="localhost/fraiseql-chart-test/server"
LOCAL_IMAGE_TAG="${SLUG}"
RELEASE="fq"

WORKDIR="$(mktemp -d)"
KUBECONFIG_FILE="$WORKDIR/kubeconfig"
# The server configuration an operator must supply. fraiseql-server refuses to
# start in production mode — the DEFAULT — while CORS is enabled with no origins
# (server_config/methods.rs), and cors_origins has no environment variable, so it
# can only arrive in a fraiseql.toml at FRAISEQL_CONFIG. This tier found that by
# deploying the chart and reading the CrashLoopBackOff.
#
# ⚠ Written here rather than committed as a fixture on purpose: it is operator
# input, like the database URL, not an artifact of this repository.
CONFIG_TOML="$WORKDIR/fraiseql.toml"
cat > "$CONFIG_TOML" <<'TOML'
cors_origins = ["http://localhost"]
TOML
# Created empty up front: `docker run -v <path>:...` on a path that does not
# exist creates a DIRECTORY there, and the kubeconfig write later in B1 would
# then fail on a directory.
: > "$KUBECONFIG_FILE"

failures=0
step() { printf '\n### %s\n' "$*"; }
fail() { echo "CHART-DEPLOY FAILED: $*" >&2; failures=$((failures + 1)); }
die()  { echo "CHART-DEPLOY FAILED: $*" >&2; exit 1; }

cleanup() {
  local rc=$?
  if [ "$rc" -ne 0 ] || [ "$failures" -ne 0 ]; then
    echo
    echo "───── diagnostics (the run did not pass) ─────"
    if docker inspect "$K3S_NAME" >/dev/null 2>&1; then
      kc get pods -A -o wide 2>&1 | head -30 || true
      echo "--- fraiseql pod describe ---"
      kc describe pods -l "app.kubernetes.io/name=fraiseql" 2>&1 | tail -60 || true
      echo "--- fraiseql pod logs (current attempt) ---"
      kc logs -l "app.kubernetes.io/name=fraiseql" --tail=100 --all-containers 2>&1 || true
      # A CrashLoopBackOff's CURRENT logs are the newest attempt, which may not
      # have reached the failure yet. The previous container's are the ones that
      # end at it.
      echo "--- fraiseql pod logs (previous attempt) ---"
      kc logs -l "app.kubernetes.io/name=fraiseql" --tail=100 --all-containers --previous 2>&1 || true
      echo "--- container exit state ---"
      kc get pods -l "app.kubernetes.io/name=fraiseql" \
        -o jsonpath='{range .items[*].status.containerStatuses[*]}{.name}{" lastState="}{.lastState}{"\n"}{end}' 2>&1 || true
      echo "--- events ---"
      kc get events --sort-by=.lastTimestamp 2>&1 | tail -30 || true
    fi
  fi
  if [ "$KEEP" -eq 1 ]; then
    echo
    echo "--keep: cluster left running. kubeconfig: $KUBECONFIG_FILE  container: $K3S_NAME"
    return
  fi
  docker rm -f "$K3S_NAME" >/dev/null 2>&1 || true
  docker rmi -f "$CLIENT_IMAGE" >/dev/null 2>&1 || true
  # ⚠ The host tag too. B2 `docker load`s the artifact and tags it, and without
  # this every run leaves a ~110 MiB image tag behind on the runner — invisible
  # until the disk fills, which on this box has already once been misread as a
  # toolchain fault.
  docker rmi -f "${LOCAL_IMAGE_REPO}:${LOCAL_IMAGE_TAG}" >/dev/null 2>&1 || true
  rm -rf "$WORKDIR" || true
}
trap cleanup EXIT

# kubectl, run inside the k3s container (the k3s binary embeds it, so nothing is
# downloaded and the client version cannot drift from the server's).
kc() { docker exec -i "$K3S_NAME" kubectl "$@"; }

# Phase A needs helm and no cluster — it runs before k3s is started, so it must
# not join a network namespace that does not exist yet.
# ⚠ No `-i`. helm never reads stdin, and `docker run -i` inside a `while read`
# loop CONSUMES that loop's input: the first iteration swallowed the remaining
# toggles, so A2 checked one of two and reported success. The loop below also
# collects into an array first, so neither half of that bug can come back.
helm_cli() {
  docker run --rm -v "$PWD:/workspace:ro" -v "$WORKDIR:/run-tmp:ro" \
    -w /workspace "$CLIENT_IMAGE" helm "$@"
}

# Phase B's client SHARES the k3s container's network namespace, so Service
# ClusterIPs route without a port-forward and the Service's own targetPort
# resolution is part of what is under test.
#
# ⚠ Built from the Postgres image plus curl and jq — deliberately NOT from the
# image under test. A client assembled inside the artifact would be testing the
# artifact with itself and could not tell "the engine answered" from "the
# container has curl". Same reasoning as .dagger/image_boot.go's imageBootClient.
client() {
  docker run --rm -i \
    --network "container:${K3S_NAME}" \
    -v "$KUBECONFIG_FILE:/kubeconfig:ro" \
    -v "$PWD:/workspace:ro" \
    -v "$WORKDIR:/run-tmp:ro" \
    -e KUBECONFIG=/kubeconfig \
    -e PGPASSWORD="$PG_PASSWORD" \
    -w /workspace \
    "$CLIENT_IMAGE" "$@"
}

echo "chart-deploy-test: run ${RUN_ID}"
echo "  chart:   $CHART_DIR"
echo "  image:   $IMAGE_TARBALL"
echo "  k3s:     $K3S_IMAGE"

# ─────────────────────────────────────────────────────────────────────────────
# Phase A — offline. Cheap, and it fails before a cluster is ever started.
# ─────────────────────────────────────────────────────────────────────────────

step "A0/A3  build the client (helm ${HELM_VERSION}, curl, jq, psql)"
docker build -q -t "$CLIENT_IMAGE" \
  --build-arg "PG_IMAGE=$PG_IMAGE" \
  --build-arg "HELM_VERSION=$HELM_VERSION" \
  --build-arg "HELM_SHA256=$HELM_SHA256" \
  -f - "$WORKDIR" <<'DOCKERFILE' >/dev/null
ARG PG_IMAGE
FROM ${PG_IMAGE}
ARG HELM_VERSION
ARG HELM_SHA256
RUN apt-get update \
 && apt-get install -y --no-install-recommends curl jq ca-certificates \
 && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL "https://get.helm.sh/helm-${HELM_VERSION}-linux-amd64.tar.gz" -o /tmp/helm.tgz \
 && echo "${HELM_SHA256}  /tmp/helm.tgz" | sha256sum -c - \
 && tar -xzf /tmp/helm.tgz -C /tmp linux-amd64/helm \
 && install -m0755 /tmp/linux-amd64/helm /usr/local/bin/helm \
 && rm -rf /tmp/helm.tgz /tmp/linux-amd64 \
 && helm version --short
DOCKERFILE
echo "client image built: $CLIENT_IMAGE"

# What an operator supplies, and nothing more. One definition, used by every
# render AND by the install below, so the thing checked offline cannot diverge
# from the thing deployed.
#
# ⚠ Note what is absent: the port, the bind address, every probe, and the whole
# Service. Those are the chart's own business. A gate that supplies them tests
# its own configuration — which is exactly how the Phase 03 boot tier stayed
# green across the six months #1216 was live (#1216, and see the header).
OPERATOR_VALUES=(
  --set-file schema.compiled="$FIXTURE_SCHEMA"
  --set-file config.content=/run-tmp/fraiseql.toml
)

# helm with no cluster. --dry-run is not used: these renders must not need one.
helm_render() {
  helm_cli template "$RELEASE" "$CHART_DIR" \
    --set-string database.url="postgresql://u:p@postgres:5432/db" \
    "${OPERATOR_VALUES[@]}" "$@"
}

step "A1/A3  every image the DEFAULT chart names is one this project publishes"
# The default render, with only the two values the chart requires supplied — the
# image reference is untouched, which is the point. This is #1129's exact class:
# `repository: fraiseql` rendered `fraiseql:2.8.0`, resolved to
# docker.io/library/fraiseql, and pulled nothing.
default_render="$WORKDIR/default-render.yaml"
if ! helm_render > "$default_render" 2>"$WORKDIR/default-render.err"; then
  cat "$WORKDIR/default-render.err" >&2
  die "the chart does not render with a database, a schema and a config supplied"
fi

image_refs="$(grep -oE '^[[:space:]]*image:[[:space:]]*"?[^"[:space:]]+' "$default_render" \
  | sed -E 's/^[[:space:]]*image:[[:space:]]*"?//' | sort -u)"
ref_count="$(printf '%s\n' "$image_refs" | grep -c . || true)"
if [ "$ref_count" -eq 0 ]; then
  die "no image: reference found in the render — the extractor matched nothing, which would make this check vacuous"
fi
echo "found $ref_count image reference(s):"
printf '%s\n' "$image_refs" | sed 's/^/  /'
while IFS= read -r ref; do
  [ -n "$ref" ] || continue
  repo="${ref%:*}"
  [ "$repo" = "$ref" ] && repo="$ref"   # no tag
  ok=0
  for pub in $PUBLISHED_IMAGES; do [ "$repo" = "$pub" ] && ok=1; done
  if [ "$ok" -ne 1 ]; then
    fail "the chart renders image '$ref', whose repository '$repo' is not one this project publishes."
    echo "       Published server images: $PUBLISHED_IMAGES"
    echo "       (A bare name like 'fraiseql' resolves to docker.io/library/fraiseql —"
    echo "        the Docker Hub official-images namespace — and pulls nothing.)"
  fi
done <<< "$image_refs"
[ "$failures" -eq 0 ] && echo "every rendered image names a published repository"

step "A2/A3  every values toggle CHANGES the render"
# The generalisation of defect 4. A `<path>.enabled` key that no template reads
# renders identically whichever way it is set, which is configuration an operator
# believes they have set. This discovers the toggles from values.yaml rather than
# listing them, so a newly added dead toggle is caught without anyone remembering.
toggles="$(python3 - "$CHART_DIR/values.yaml" <<'PY'
import re, sys
stack, out = [], []
for raw in open(sys.argv[1]):
    if not raw.strip() or raw.lstrip().startswith('#'):
        continue
    indent = len(raw) - len(raw.lstrip())
    m = re.match(r'^([A-Za-z_][\w-]*):\s*(.*)$', raw.strip())
    if not m:
        continue
    key, val = m.group(1), m.group(2).strip()
    # A toggle may be written quoted; missing those would make this check blind
    # for exactly the key someone added by hand.
    val = val.strip('"\'')
    while stack and stack[-1][0] >= indent:
        stack.pop()
    path = '.'.join([k for _, k in stack] + [key])
    if key == 'enabled' and val in ('true', 'false'):
        out.append(f'{path}\t{val}')
    stack.append((indent, key))
print('\n'.join(out))
PY
)"
toggle_count="$(printf '%s\n' "$toggles" | grep -c . || true)"
if [ "$toggle_count" -eq 0 ]; then
  die "no '<path>.enabled' key found in values.yaml — this check would be vacuous, and it was written because four such keys rendered nothing"
fi
# Every `enabled:` line in the file must have been CLASSIFIED. A parser that
# silently skips a key it cannot read leaves that key unchecked while reporting
# success for the ones it could — the same shape as a suite that skips a test.
raw_enabled="$(grep -cE '^[[:space:]]*enabled:' "$CHART_DIR/values.yaml" || true)"
if [ "$toggle_count" -ne "$raw_enabled" ]; then
  die "values.yaml has $raw_enabled 'enabled:' line(s) but the parser classified $toggle_count.
       Every toggle must be discoverable, or this check silently covers less than it claims."
fi
echo "found $toggle_count toggle(s) in values.yaml (all $raw_enabled 'enabled:' lines classified)"
# Collected into an array BEFORE any docker call, so nothing inside the loop can
# eat the list out from under it.
toggle_lines=()
while IFS= read -r line; do [ -n "$line" ] && toggle_lines+=("$line"); done <<< "$toggles"
checked=0
for line in "${toggle_lines[@]}"; do
  path="${line%%$'\t'*}"; val="${line##*$'\t'}"
  [ -n "$path" ] || continue
  checked=$((checked + 1))
  flipped="true"; [ "$val" = "true" ] && flipped="false"
  other="$WORKDIR/toggle-$(printf '%s' "$path" | tr '.' '_').yaml"
  if ! helm_render --set "$path=$flipped" > "$other" 2>&1; then
    fail "setting $path=$flipped makes the chart fail to render:"; sed 's/^/       /' "$other" >&2
    continue
  fi
  if cmp -s "$default_render" "$other"; then
    fail "$path is a dead toggle: the render with $path=$flipped is byte-identical to the render with $path=$val."
    echo "       Either a template must read it, or the value must be removed. A value that"
    echo "       does nothing is worse than an absent one — it is configuration an operator"
    echo "       believes they have set (#1129 follow-up: four such keys shipped at once)."
  else
    echo "  $path: $val -> $flipped changes the render"
  fi
done
# Counts, not exit codes: if the loop silently processed fewer toggles than were
# discovered, this check covered less than it claims to.
[ "$checked" -eq "$toggle_count" ] \
  || die "A2 discovered $toggle_count toggle(s) but only checked $checked — the loop lost input, and a check that skips its subjects reports success for them"

step "A3/A3  the chart REFUSES to render an unstartable release"
# The two guards that turn a silent runtime failure into an install-time error.
# Proving they fire is what makes them a gate rather than a comment.
if helm_cli template "$RELEASE" "$CHART_DIR" \
     "${OPERATOR_VALUES[@]}" >"$WORKDIR/nodb.out" 2>&1; then
  fail "the chart rendered with NO database configured. It must fail: the Deployment would mount DATABASE_URL from a Secret that does not exist, and the pod would never start."
else
  if grep -q "no database configured" "$WORKDIR/nodb.out"; then
    echo "  no database  -> refused, with an instruction"
  else
    fail "the chart refused to render without a database, but not with the expected message:"; sed 's/^/       /' "$WORKDIR/nodb.out" >&2
  fi
fi
if helm_cli template "$RELEASE" "$CHART_DIR" \
     --set-string database.url="postgresql://u:p@h:5432/d" \
     --set-file config.content=/run-tmp/fraiseql.toml >"$WORKDIR/noschema.out" 2>&1; then
  fail "the chart rendered with NO compiled schema configured. It must fail: fraiseql-server exits at startup when FRAISEQL_SCHEMA_PATH names no file."
else
  if grep -q "no compiled schema configured" "$WORKDIR/noschema.out"; then
    echo "  no schema    -> refused, with an instruction"
  else
    fail "the chart refused to render without a schema, but not with the expected message:"; sed 's/^/       /' "$WORKDIR/noschema.out" >&2
  fi
fi
# The third guard, and the one this tier discovered by deploying: a
# production-mode release with no fraiseql.toml starts a pod that exits on its
# first line with "cors_enabled is true but cors_origins is empty".
if helm_cli template "$RELEASE" "$CHART_DIR" \
     --set-string database.url="postgresql://u:p@h:5432/d" \
     --set-file schema.compiled="$FIXTURE_SCHEMA" >"$WORKDIR/noconfig.out" 2>&1; then
  fail "the chart rendered a PRODUCTION-mode release with no configuration file. It must fail: fraiseql-server exits at startup with 'cors_enabled is true but cors_origins is empty in production mode', and cors_origins has no environment variable."
else
  if grep -q "no configuration file supplied" "$WORKDIR/noconfig.out"; then
    echo "  no config    -> refused, with an instruction"
  else
    fail "the chart refused to render without a config, but not with the expected message:"; sed 's/^/       /' "$WORKDIR/noconfig.out" >&2
  fi
fi
# ...and the documented escape hatch actually works: a development-mode release
# needs no config file. A guard that cannot be satisfied is not a guard.
if ! helm_cli template "$RELEASE" "$CHART_DIR" \
     --set-string database.url="postgresql://u:p@h:5432/d" \
     --set-file schema.compiled="$FIXTURE_SCHEMA" \
     --set env.FRAISEQL_ENV=development >"$WORKDIR/devmode.out" 2>&1; then
  fail "env.FRAISEQL_ENV=development is the escape hatch the guard's own message names, and the chart still refused to render:"; sed 's/^/       /' "$WORKDIR/devmode.out" >&2
else
  echo "  development  -> renders without a config, as the guard's message promises"
fi

if [ "$failures" -ne 0 ]; then
  die "$failures offline check(s) failed; not starting a cluster."
fi

# ─────────────────────────────────────────────────────────────────────────────
# Phase B — a real cluster. Everything above is satisfiable by a chart that
# cannot start a pod; this is the part that is not.
# ─────────────────────────────────────────────────────────────────────────────

step "B1/B9  start k3s"
# ⚠ No --snapshotter=native. It is the usual advice for k3s-in-a-container, and
# measured here it made the cluster unusable: unpacking a Postgres image took
# longer than the kubelet's CreateContainer deadline, so the pod sat in
# CreateContainerError with "context deadline exceeded" and the node reported
# "invalid capacity 0 on image filesystem". The default overlayfs snapshotter
# works inside a privileged container on this kernel.
docker rm -f "$K3S_NAME" >/dev/null 2>&1 || true
# ⚠ /lib/modules is load-bearing, and its absence fails in a way that looks like
# a chart defect. Without it `modprobe br_netfilter` fails inside the container,
# so `net/bridge/bridge-nf-call-iptables` does not exist, so kube-proxy's DNAT is
# never applied to traffic crossing the CNI bridge. Measured: pod-to-POD-IP
# worked and pod-to-CLUSTER-IP hung, which surfaced as the server exiting with
# "Connection pool error: Failed to acquire connection" — a message that names
# neither Kubernetes nor the bridge. B5 below asserts the routing directly so
# this can never again be diagnosed through the artifact.
modules_mount=()
[ -d /lib/modules ] && modules_mount=(-v /lib/modules:/lib/modules:ro)
docker run -d --name "$K3S_NAME" --privileged \
  --tmpfs /run --tmpfs /var/run \
  "${modules_mount[@]}" \
  "$K3S_IMAGE" server \
  --disable=traefik --disable=metrics-server --disable=servicelb \
  --disable-network-policy --write-kubeconfig-mode=644 \
  >/dev/null

node_ready=0
for _ in $(seq 1 120); do
  if docker exec "$K3S_NAME" kubectl get nodes 2>/dev/null | grep -q ' Ready '; then node_ready=1; break; fi
  if [ "$(docker inspect -f '{{.State.Running}}' "$K3S_NAME" 2>/dev/null)" != "true" ]; then
    echo "--- k3s container exited ---"; docker logs "$K3S_NAME" 2>&1 | tail -40
    die "the k3s container exited before the node was Ready"
  fi
  sleep 1
done
[ "$node_ready" -eq 1 ] || { docker logs "$K3S_NAME" 2>&1 | tail -40; die "the k3s node never became Ready"; }
docker exec "$K3S_NAME" cat /etc/rancher/k3s/k3s.yaml > "$KUBECONFIG_FILE"
kc get nodes --no-headers | sed 's/^/  /'
# The chart's DATABASE_URL names the `postgres` Service, not an address, so
# cluster DNS has to be serving before the release is installed. A node that is
# Ready is not a cluster that resolves — and it goes Ready in about a second,
# BEFORE k3s's deploy controller has even created the CoreDNS Deployment, so the
# object has to be waited into existence before it can be waited on.
coredns_seen=0
for _ in $(seq 1 120); do
  if kc get deploy coredns -n kube-system >/dev/null 2>&1; then coredns_seen=1; break; fi
  sleep 1
done
[ "$coredns_seen" -eq 1 ] || die "k3s never created the CoreDNS Deployment"
kc wait --for=condition=Available deploy/coredns -n kube-system --timeout=180s | sed 's/^/  /'

step "B2/B9  import the artifact under test, and Postgres, into the cluster"
# Both images are imported rather than pulled, so the cluster needs no registry
# credentials and the deploy cannot silently test a DIFFERENT image than the one
# this branch built.
loaded="$(docker load -i "$IMAGE_TARBALL" 2>&1 | tail -1)"
echo "  docker load: $loaded"
image_id="$(printf '%s' "$loaded" | sed -nE 's/.*(Loaded image ID|Loaded image): *//p')"
[ -n "$image_id" ] || die "could not read an image id out of 'docker load' output: $loaded"
docker tag "$image_id" "${LOCAL_IMAGE_REPO}:${LOCAL_IMAGE_TAG}"
docker save "${LOCAL_IMAGE_REPO}:${LOCAL_IMAGE_TAG}" \
  | docker exec -i "$K3S_NAME" ctr -n k8s.io images import - >/dev/null
echo "  imported ${LOCAL_IMAGE_REPO}:${LOCAL_IMAGE_TAG}"

docker image inspect "$PG_IMAGE" >/dev/null 2>&1 || docker pull -q "$PG_IMAGE" >/dev/null
docker save "$PG_IMAGE" | docker exec -i "$K3S_NAME" ctr -n k8s.io images import - >/dev/null
echo "  imported $PG_IMAGE"

step "B3/B9  deploy Postgres into the cluster"
kc apply -f - <<YAML >/dev/null
apiVersion: apps/v1
kind: Deployment
metadata:
  name: postgres
spec:
  replicas: 1
  selector:
    matchLabels: {app: postgres}
  template:
    metadata:
      labels: {app: postgres}
    spec:
      containers:
      - name: postgres
        image: $PG_IMAGE
        imagePullPolicy: Never
        env:
        - {name: POSTGRES_USER,     value: "$PG_USER"}
        - {name: POSTGRES_PASSWORD, value: "$PG_PASSWORD"}
        - {name: POSTGRES_DB,       value: "$PG_DATABASE"}
        - {name: PGDATA,            value: /var/lib/postgresql/data/pgdata}
        ports: [{containerPort: 5432}]
        readinessProbe:
          exec: {command: ["pg_isready", "-U", "$PG_USER", "-d", "$PG_DATABASE"]}
          initialDelaySeconds: 2
          periodSeconds: 2
          failureThreshold: 60
        volumeMounts: [{name: data, mountPath: /var/lib/postgresql/data}]
      volumes: [{name: data, emptyDir: {}}]
---
apiVersion: v1
kind: Service
metadata:
  name: postgres
spec:
  selector: {app: postgres}
  ports: [{port: 5432, targetPort: 5432}]
YAML
kc rollout status deploy/postgres --timeout=180s | sed 's/^/  /'

step "B4/B9  seed a FRESH schema under ON_ERROR_STOP=1, and count the rows"
# Drop and rebuild first, and assert the count. docker/e2e/init-postgres.sql is
# CREATE TABLE IF NOT EXISTS plus a bare INSERT: against a dirty database it
# applies partially and reports success (#1214). From an empty schema the count
# below is a real assertion.
psql_pod() { kc exec -i deploy/postgres -- env PGPASSWORD="$PG_PASSWORD" psql -U "$PG_USER" -d "$PG_DATABASE" -v ON_ERROR_STOP=1 "$@"; }
psql_pod -q -c 'DROP SCHEMA IF EXISTS public CASCADE; CREATE SCHEMA public;' >/dev/null
psql_pod -q -f - < "$FIXTURE_SQL" >/dev/null
rows="$(psql_pod -tAc 'SELECT count(*) FROM tb_user' | tr -d '[:space:]')"
[ "$rows" = "$FIXTURE_ROWS" ] \
  || die "fixture loaded $rows row(s) into tb_user, expected $FIXTURE_ROWS ($FIXTURE_SQL)"
echo "  seeded $FIXTURE_ROWS row(s) into a freshly created schema"

step "B5/B9  the CLUSTER routes ClusterIP traffic (a rig check, not a chart check)"
# The chart's pod reaches Postgres through a Service, so if Service routing is
# broken every assertion after this point fails for a reason that has nothing to
# do with the artifact. Asserting it here, from a POD — the path the chart's pod
# actually takes, and not the node's own netns, which uses different iptables
# chains — means a broken rig reports itself instead of being read as a defect in
# the thing under test.
pg_cluster_ip="$(kc get svc postgres -o jsonpath='{.spec.clusterIP}')"
[ -n "$pg_cluster_ip" ] || die "the postgres Service has no ClusterIP"
if ! kc run rig-check --attach --rm --restart=Never --image="$PG_IMAGE" \
      --image-pull-policy=Never --command -- \
      env PGPASSWORD="$PG_PASSWORD" timeout 30 \
      psql -h "$pg_cluster_ip" -U "$PG_USER" -d "$PG_DATABASE" -tAc 'SELECT 1' \
      > "$WORKDIR/rig.out" 2>&1; then
  sed 's/^/       /' "$WORKDIR/rig.out" >&2
  die "RIG FAILURE, not a chart failure: a pod cannot reach a Service ClusterIP
       (${pg_cluster_ip}:5432) in this cluster. The usual cause is that
       br_netfilter could not be loaded, so net/bridge/bridge-nf-call-iptables
       does not exist and kube-proxy's DNAT is never applied to bridged pod
       traffic. Check that /lib/modules exists on this host and is mounted into
       the k3s container (see B1), and that the container is --privileged."
fi
echo "  a pod reached postgres through its ClusterIP ${pg_cluster_ip}:5432"

step "B6/B9  every rendered object is accepted by a real Kubernetes API server"
# The DEFAULT render — autoscaling on, so the HorizontalPodAutoscaler is
# validated here even though the install below turns it off for a single-node
# throwaway cluster. `--dry-run=server` runs real schema validation and admission,
# which `helm template` and `helm lint` do not.
kc apply --dry-run=server -f - < "$default_render" | sed 's/^/  /'

step "B7/B9  helm install, and wait for the Deployment to become available"
# ⚠ What is passed here, and what is deliberately NOT.
#
# Passed: the image (this branch's build, which no registry serves before the
# tag), the database, the schema, and a single replica with autoscaling off
# because this is a one-node throwaway cluster.
#
# NOT passed: the port, the bind address, any probe, anything about the Service.
# Those are the chart's own business, and a gate that supplies them tests its own
# configuration rather than the artifact's. That is precisely how the Phase 03
# boot tier stayed green across the six months #1216 was live: it set
# FRAISEQL_BIND_ADDR itself and so agreed with the Dockerfile's mistake.
client helm install "$RELEASE" "$CHART_DIR" \
  --set image.repository="$LOCAL_IMAGE_REPO" \
  --set image.tag="$LOCAL_IMAGE_TAG" \
  --set-string database.url="postgresql://${PG_USER}:${PG_PASSWORD}@postgres:5432/${PG_DATABASE}" \
  "${OPERATOR_VALUES[@]}" \
  --set autoscaling.enabled=false \
  --set replicaCount=1 \
  --wait --timeout 4m | sed 's/^/  /'

# `helm --wait` returns on Available, which for this chart means the readiness
# probe passed — and readiness is /readiness, which is 503 while the database is
# unreachable. So reaching this line already means: the image was resolvable, the
# Secret existed, the schema was mounted, the process bound the port the probes
# address, and it reached Postgres. Every one of the four defects would stop here.
kc rollout status "deploy/${RELEASE}-fraiseql" --timeout=120s | sed 's/^/  /'

svc_ip="$(kc get svc "${RELEASE}-fraiseql" -o jsonpath='{.spec.clusterIP}')"
svc_port="$(kc get svc "${RELEASE}-fraiseql" -o jsonpath='{.spec.ports[0].port}')"
[ -n "$svc_ip" ] || die "the Service has no ClusterIP"
echo "  service: ${svc_ip}:${svc_port}"

step "B8/B9  the deployed release answers through its Service"
# Through the Service's ClusterIP, not a pod IP: the Service's targetPort
# resolution is part of what is under test, and `targetPort: http` pointing at a
# containerPort the process does not listen on is exactly what shipped.
BASE="http://${svc_ip}:${svc_port}"
client bash -c "
set -euo pipefail
code=\$(curl -sS -o /tmp/health.json -w '%{http_code}' '$BASE/health')
echo \"GET /health -> HTTP \$code\"; cat /tmp/health.json; echo
[ \"\$code\" = 200 ] || { echo 'FAIL: /health did not answer 200' >&2; exit 1; }
jq -e '.database.connected == true' /tmp/health.json >/dev/null \
  || { echo 'FAIL: /health does not report database.connected: true' >&2; exit 1; }

code=\$(curl -sS -o /tmp/ready.json -w '%{http_code}' '$BASE/readiness')
echo \"GET /readiness -> HTTP \$code\"; cat /tmp/ready.json; echo
[ \"\$code\" = 200 ] || { echo 'FAIL: /readiness did not answer 200' >&2; exit 1; }

code=\$(curl -sS -o /tmp/q1.json -w '%{http_code}' -X POST '$BASE/graphql' \
  -H 'Content-Type: application/json' -d '{\"query\":\"{ users { id name } }\"}')
echo \"POST /graphql -> HTTP \$code\"; cat /tmp/q1.json; echo
[ \"\$code\" = 200 ] || { echo 'FAIL: /graphql did not answer 200' >&2; exit 1; }
if jq -e 'has(\"errors\")' /tmp/q1.json >/dev/null; then
  echo 'FAIL: /graphql answered 200 with an errors payload' >&2; exit 1
fi
jq -e '[.data.users[].name] | sort == [\"Alice\",\"Bob\",\"Charlie\"]' /tmp/q1.json >/dev/null \
  || { echo 'FAIL: /graphql did not return the seeded rows' >&2; exit 1; }
echo 'the deployed release returned the $FIXTURE_ROWS seeded rows'
"

step "B9/B9  DISCRIMINATOR — change the database, ask again, require the answer to change"
# Everything above is satisfiable by a pod serving a fixed or cached response.
# This is not: the marker is minted here, written to Postgres by a client the
# deployed pod knows nothing about, and the engine has to go and find it.
MARKER="phase05-${SLUG}-$(date +%s%N)-$$"
if grep -q "$MARKER" "$default_render" 2>/dev/null; then
  die "the marker already appears in the render — it is not discriminating"
fi
psql_pod -q -c "INSERT INTO tb_user (name) VALUES ('$MARKER');" >/dev/null
after="$(psql_pod -tAc 'SELECT count(*) FROM tb_user' | tr -d '[:space:]')"
[ "$after" = "$((FIXTURE_ROWS + 1))" ] || die "the INSERT did not land: tb_user holds $after row(s)"
echo "  inserted $MARKER"

client bash -c "
set -euo pipefail
code=\$(curl -sS -o /tmp/q2.json -w '%{http_code}' -X POST '$BASE/graphql' \
  -H 'Content-Type: application/json' -d '{\"query\":\"{ users { id name } }\"}')
echo \"POST /graphql -> HTTP \$code\"; cat /tmp/q2.json; echo
[ \"\$code\" = 200 ] || { echo 'FAIL: /graphql did not answer 200 on the re-query' >&2; exit 1; }
if jq -e 'has(\"errors\")' /tmp/q2.json >/dev/null; then
  echo 'FAIL: /graphql answered 200 with an errors payload on the re-query' >&2; exit 1
fi
jq --arg m '$MARKER' -e '[.data.users[].name] | index(\$m) != null' /tmp/q2.json >/dev/null \
  || { echo 'FAIL: the deployed release did not return the row inserted after it started — it served a stale or fabricated answer, not the database' >&2; exit 1; }
jq -e '.data.users | length == $((FIXTURE_ROWS + 1))' /tmp/q2.json >/dev/null \
  || { echo 'FAIL: the re-query did not return $((FIXTURE_ROWS + 1)) rows' >&2; exit 1; }
"

if [ "$failures" -ne 0 ]; then
  die "$failures check(s) failed."
fi

echo
echo "chart-deploy OK (run ${RUN_ID}): the chart names only published images, has no dead"
echo "toggles, refuses to render an unstartable release, was accepted by a real API server,"
echo "deployed into k3s on the image this branch builds, and answered a GraphQL query with a"
echo "row inserted after the pod was already serving."
