#!/usr/bin/env bash
# check-deploy-versions.sh — fail when a shipped deployment artifact names a version
# that is not the one being released, or an image that is not published.
#
# Background (#1129): `tools/release.sh` bumped the workspace, the crates, the fuzz
# crates and the Rust/Python/TypeScript SDK manifests — and no deployment artifact. So
# the Dockerfile's OCI version label sat at 2.1.1 and the Helm chart at 2.1.1/2.1.0
# while the product shipped 2.14.1, and `values.yaml` pinned `fraiseql:2.8.0`.
#
# Worse than stale: `repository: fraiseql` resolves to `docker.io/library/fraiseql`, the
# Docker Hub OFFICIAL-IMAGES namespace, which this project does not and cannot publish
# to. `helm install` on the shipped defaults pulled nothing at all.
#
# Nothing caught it. `helm.yml` is workflow_dispatch:-only and only lints — and a lint
# never resolves an image. `docker-build.yml` is tag-only, so the label is never
# inspected on a branch.
#
# Pure grep/sed, no toolchain, no git history → Dagger ShellGates.
#
# Overrides, for testing:
#   DEPLOY_VERSIONS_ROOT=<dir>   treat this directory as the repo root
set -euo pipefail

# ShellGates runs `git init -q .`, so rev-parse works there; the override exists for the
# unit test, which builds a fixture tree in a temp dir.
if [ -n "${DEPLOY_VERSIONS_ROOT:-}" ]; then
  cd "$DEPLOY_VERSIONS_ROOT"
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root"
fi

CARGO_TOML="Cargo.toml"
DOCKERFILE="Dockerfile"
CHART="deploy/kubernetes/helm/fraiseql/Chart.yaml"
VALUES="deploy/kubernetes/helm/fraiseql/values.yaml"
# The canonical Compose stack pins the same version, for the same reason the chart
# does: a stack naming a version this repository is not building names an image that
# does not exist. Added 2026-08-28, when the six shipped stacks were collapsed to one
# — the deleted ones pinned `fraiseql/server:latest` and `fraiseql/dashboard:latest`,
# the second of which has never been published at all.
COMPOSE="docker-compose.yml"

for f in "$CARGO_TOML" "$DOCKERFILE" "$CHART" "$VALUES" "$COMPOSE"; do
  if [ ! -f "$f" ]; then
    echo "ERROR: deploy-version scan target not found: $f" >&2
    exit 1
  fi
done

# ⚠ Anchored, and only the [workspace.package] one. An unanchored `version =` match
# would happily be satisfied by the first dependency pin in the file.
version="$(grep -m1 '^version = ' "$CARGO_TOML" | sed -E 's/^version = "([^"]+)".*/\1/')"
if [ -z "$version" ]; then
  echo "ERROR: could not read the workspace version from $CARGO_TOML" >&2
  exit 1
fi

# Images this repository actually publishes (.github/workflows/docker-build.yml:
# ghcr.io/<owner>/{server,server-full,tutorial} plus the Docker Hub mirrors). The chart
# deploys the server, so those are the only two acceptable defaults; the runbooks
# (docs/runbooks/01-deployment.md) use the ghcr form.
PUBLISHED_IMAGES="ghcr.io/fraiseql/server fraiseql/server"

status=0

fail() {
  echo "ERROR: $1"
  status=1
}

# ── Dockerfile OCI version label ─────────────────────────────────────────────
# ⚠ Anchored on the LABEL key. `Dockerfile:8` is `FROM rust:1.94.1-slim`, and a
# version-shaped match anywhere in the file would rewrite the toolchain pin — which is
# #1107's subject, not this gate's.
label="$(grep -oE 'org\.opencontainers\.image\.version="[^"]+"' "$DOCKERFILE" \
  | head -1 | sed -E 's/.*="([^"]+)"/\1/' || true)"
if [ -z "$label" ]; then
  fail "$DOCKERFILE has no org.opencontainers.image.version label."
elif [ "$label" != "$version" ]; then
  fail "$DOCKERFILE org.opencontainers.image.version is '$label', workspace is '$version'."
fi

# ── Helm chart ───────────────────────────────────────────────────────────────
# Lockstep: `version` (the chart's own packaging version) and `appVersion` (the app it
# deploys) both track the release. Recorded in the chart header; the alternative —
# moving the chart version only when templates change — is defensible but was never
# stated, which is how these drifted to 2.1.1/2.1.0 unnoticed.
chart_version="$(grep -oE '^version: *[^ ]+' "$CHART" | head -1 | sed -E 's/^version: *//' | tr -d '"' || true)"
chart_app="$(grep -oE '^appVersion: *[^ ]+' "$CHART" | head -1 | sed -E 's/^appVersion: *//' | tr -d '"' || true)"

[ "$chart_version" = "$version" ] || fail "$CHART version is '$chart_version', workspace is '$version'."
[ "$chart_app" = "$version" ] || fail "$CHART appVersion is '$chart_app', workspace is '$version'."

# ── values.yaml ──────────────────────────────────────────────────────────────
image_tag="$(grep -oE '^ *tag: *"?[^"]+"?' "$VALUES" | head -1 | sed -E 's/^ *tag: *//' | tr -d '"' || true)"
image_repo="$(grep -oE '^ *repository: *"?[^"]+"?' "$VALUES" | head -1 | sed -E 's/^ *repository: *//' | tr -d '"' || true)"

[ "$image_tag" = "$version" ] || fail "$VALUES image.tag is '$image_tag', workspace is '$version'."

repo_ok=0
for img in $PUBLISHED_IMAGES; do
  [ "$image_repo" = "$img" ] && repo_ok=1
done
if [ "$repo_ok" -ne 1 ]; then
  fail "$VALUES image.repository is '$image_repo', which this project does not publish."
  echo "       Published server images: $PUBLISHED_IMAGES"
  echo "       (A bare name like 'fraiseql' resolves to docker.io/library/fraiseql —"
  echo "        the Docker Hub official-images namespace — and pulls nothing.)"
fi

# ── docker-compose.yml ───────────────────────────────────────────────────────
# The service name is not assumed: the reference is taken from whichever `image:`
# line names a repository this project publishes, and it is an error to find none —
# otherwise a renamed service would make this check silently pass on nothing.
compose_ref=""
while IFS= read -r ref; do
  for img in $PUBLISHED_IMAGES; do
    case "$ref" in "$img":*) compose_ref="$ref" ;; esac
  done
done < <(grep -oE '^[[:space:]]*image:[[:space:]]*"?[^"[:space:]]+' "$COMPOSE" \
         | sed -E 's/^[[:space:]]*image:[[:space:]]*"?//')

if [ -z "$compose_ref" ]; then
  fail "$COMPOSE names no image from a repository this project publishes."
  echo "       Published server images: $PUBLISHED_IMAGES"
  echo "       (A bare name like 'fraiseql' resolves to docker.io/library/fraiseql —"
  echo "        the Docker Hub official-images namespace — and pulls nothing.)"
else
  compose_tag="${compose_ref##*:}"
  [ "$compose_tag" = "$version" ] \
    || fail "$COMPOSE pins '$compose_ref', workspace is '$version'."
fi

if [ "$status" -eq 0 ]; then
  echo "OK: deploy artifacts all name v${version}, and the chart and compose images are"
  echo "    published ones."
fi
exit "$status"
