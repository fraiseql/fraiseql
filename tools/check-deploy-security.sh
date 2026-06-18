#!/usr/bin/env bash
#
# Phase 13 — deployment & ops hardening gate (H46 + sweep regressions).
#
# Static checks over the shipped deployment artifacts so a regression cannot
# re-expose what the audit closed. Pure grep/find — no kube/yaml tooling — so it
# runs in the minimal ShellGates container.
#
# Checks:
#   A. Root compose files publish only loopback (127.0.0.1) or the app port (8815)
#      to host interfaces — Docker port publishing bypasses host firewalls, so an
#      unqualified "5432:5432" exposes a backing service to the network (H46).
#   B. The production compose runs Redis with --requirepass (no unauthenticated Redis).
#   C. The production compose guards ${DB_PASSWORD} with a fail-loud `:?` (no empty
#      default that yields a passwordless database).
#   D. No prod/k8s/deploy manifest pins an image to :latest (reproducible deploys).
#   E. No k8s/deploy manifest sets readOnlyRootFilesystem: false.
set -euo pipefail

# The app's own public port — the one mapping a compose file is allowed to publish
# to all interfaces.
APP_PORT="8815:8815"

# Root compose files subject to the loopback port rule (every shipped compose).
COMPOSE_PORT_FILES=(docker-compose.prod.yml docker-compose.yml)

# The production compose, subject to the stricter auth/secret rules.
PROD_COMPOSE="docker-compose.prod.yml"

# Manifest files (k8s + deploy/kubernetes, recursive incl. the Helm chart) subject
# to the :latest and readOnlyRootFilesystem rules. `.example` templates are skipped.
mapfile -t MANIFEST_FILES < <(find k8s deploy/kubernetes -type f \( -name '*.yaml' -o -name '*.yml' \) 2>/dev/null | sort)

rc=0

# ── A. compose port publishing ──────────────────────────────────────────────
for f in "${COMPOSE_PORT_FILES[@]}"; do
  [ -f "$f" ] || continue
  # Inspect only YAML sequence entries (`  - "..."`), so prose/comments are
  # ignored, then keep only host:container port mappings (`[ip:]hostport:
  # containerport`, all numeric) — excludes volume mounts and command arrays.
  while IFS= read -r mapping; do
    [ -n "$mapping" ] || continue
    case "$mapping" in
      127.0.0.1:*) ;;      # loopback — OK
      "$APP_PORT") ;;      # the app's public port — OK
      *)
        echo "FAIL (H46): $f publishes '$mapping' to a non-loopback interface."
        echo "    Bind backing services to 127.0.0.1: (or remove the published port)."
        rc=1
        ;;
    esac
  done < <(grep -E '^[[:space:]]*-[[:space:]]*"[^"]+"[[:space:]]*$' "$f" \
             | grep -oE '"[^"]+"' | tr -d '"' \
             | grep -E '^([0-9.]+:)?[0-9]+:[0-9]+$' || true)
done

# ── B. Redis requires a password in the production compose ───────────────────
if [ -f "$PROD_COMPOSE" ] && grep -q 'redis-server' "$PROD_COMPOSE"; then
  if ! grep -q 'requirepass' "$PROD_COMPOSE"; then
    echo "FAIL (H46): $PROD_COMPOSE runs redis-server without --requirepass."
    rc=1
  fi
fi

# ── C. DB_PASSWORD must fail loud (no empty default) in the production compose ─
if [ -f "$PROD_COMPOSE" ] && grep -qE '\$\{DB_PASSWORD\}|\$\{DB_PASSWORD:-' "$PROD_COMPOSE"; then
  echo "FAIL: $PROD_COMPOSE uses \${DB_PASSWORD} without a fail-loud guard."
  echo "    Use \${DB_PASSWORD:?DB_PASSWORD must be set} so an unset password aborts startup."
  rc=1
fi

# ── D. no :latest image tags in prod/k8s/deploy manifests ───────────────────
latest_targets=("$PROD_COMPOSE" "${MANIFEST_FILES[@]}")
for f in "${latest_targets[@]}"; do
  [ -f "$f" ] || continue
  # `image: repo:latest` (compose/manifests) or `tag: latest` (Helm values).
  if grep -nE '^[[:space:]]*image:[[:space:]]*[^[:space:]#]+:latest([[:space:]]|$)' "$f" \
     || grep -nE '^[[:space:]]*tag:[[:space:]]*"?latest"?[[:space:]]*$' "$f"; then
    echo "FAIL: $f pins an image to :latest — pin a version for reproducible deploys."
    rc=1
  fi
done

# ── E. no readOnlyRootFilesystem: false in k8s/deploy manifests ─────────────
for f in "${MANIFEST_FILES[@]}"; do
  [ -f "$f" ] || continue
  if grep -nE 'readOnlyRootFilesystem:[[:space:]]*false' "$f"; then
    echo "FAIL: $f sets readOnlyRootFilesystem: false — the workload runs read-only."
    rc=1
  fi
done

if [ "$rc" -ne 0 ]; then
  echo ""
  echo "Deployment-security gate FAILED. See messages above."
  exit 1
fi

echo "OK: deployment artifacts publish only loopback/app ports, Redis is authenticated,"
echo "    DB_PASSWORD fails loud, no :latest pins, no readOnlyRootFilesystem: false."
