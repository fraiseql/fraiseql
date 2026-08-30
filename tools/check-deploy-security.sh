#!/usr/bin/env bash
#
# Phase 13 — deployment & ops hardening gate (H46 + sweep regressions).
#
# Static checks over the shipped deployment artifacts so a regression cannot
# re-expose what the audit closed. Pure grep/find — no kube/yaml tooling — so it
# runs in the minimal ShellGates container.
#
# Checks:
#   A. The canonical compose stack publishes only loopback (127.0.0.1) or the app
#      port (8000) to host interfaces — Docker port publishing bypasses host
#      firewalls, so an unqualified "5432:5432" exposes a backing service to the
#      network (H46).
#   B. It guards ${DB_PASSWORD} with a fail-loud `:?` (no empty default that yields
#      a passwordless database).
#   C. No compose/k8s/deploy manifest pins an image to :latest (reproducible deploys).
#   D. No k8s/deploy manifest sets readOnlyRootFilesystem: false.
#
# ⚠ There used to be a check E: "the production compose runs Redis with
# --requirepass". It is GONE rather than kept, because as of 2026-08-28 no shipped
# stack runs Redis at all — the server binary reads no REDIS_URL, only tests do, so
# the service the deleted docker-compose.prod.yml ran was one nothing connected to.
# The check was written `if grep -q redis-server; then ...`, so with no subject it
# would have SKIPPED silently while still printing its name in this file's success
# line — a gate that cannot fail, reported as coverage. If Redis returns to a
# shipped stack, so does the rule.
set -euo pipefail

# The app's own public port — the one mapping a compose file is allowed to publish
# to all interfaces.
#
# ⚠ 8000, and it was 8815. This constant is why the port half of #1216 could not
# be fixed piecemeal: the image was corrected to bind 0.0.0.0:8000 while this gate
# still REQUIRED the compose files to publish 8815:8815, so correcting them turned
# the security gate red. A gate that pins a number must move with the artifact that
# defines it — here, the Dockerfile's EXPOSE and FRAISEQL_BIND_ADDR.
APP_PORT="8000:8000"

# The canonical compose stack — the ONE stack this repository ships and verifies
# (tools/compose-stack-test.sh). Five others were deleted on 2026-08-28: measured
# against a real docker, not one of the six could serve a query.
#
# A list rather than a scalar so adding a second shipped stack is a one-line change
# here, and so `[ -f ]` below cannot silently reduce this gate to nothing.
COMPOSE_PORT_FILES=(docker-compose.yml)
PROD_COMPOSE="docker-compose.yml"

# The subject must exist. Every check below is guarded by `[ -f ]`, so a renamed or
# deleted stack would turn this gate into a no-op that still prints "OK".
for f in "${COMPOSE_PORT_FILES[@]}" "$PROD_COMPOSE"; do
  [ -f "$f" ] || { echo "FAIL: $f does not exist; this gate would check nothing."; exit 1; }
done

# Manifest files under deploy/kubernetes (recursive, incl. the Helm chart) subject
# to the :latest and readOnlyRootFilesystem rules. `.example` templates are skipped.
#
# The scan used to name `k8s` as well. That directory held a third, ungated copy of
# the deployment and was deleted in #1218; `find` swallowed its absence through
# `2>/dev/null`, which is the shape that lets a scan quietly cover less than it
# says. Hence the count assertion below: this gate must never report OK having
# looked at nothing.
mapfile -t MANIFEST_FILES < <(find deploy/kubernetes -type f \( -name '*.yaml' -o -name '*.yml' \) 2>/dev/null | sort)

if [ "${#MANIFEST_FILES[@]}" -eq 0 ]; then
  echo "FAIL: found no Kubernetes manifests under deploy/kubernetes."
  echo "      The scan is wrong, or the chart is gone. Either way this gate would"
  echo "      report OK having checked nothing."
  exit 1
fi

rc=0

# ── A. compose port publishing ──────────────────────────────────────────────
for f in "${COMPOSE_PORT_FILES[@]}"; do
  [ -f "$f" ] || continue
  # Inspect only YAML sequence entries (`  - "..."`), so prose/comments are
  # ignored, then keep only host:container port mappings (`[ip:]hostport:
  # containerport`, all numeric) — excludes volume mounts and command arrays.
  while IFS= read -r raw; do
    [ -n "$raw" ] || continue
    # ⚠ Resolve `${VAR:-default}` to its DEFAULT before classifying, and only then.
    # The host side of each published port is a variable with a default so a port
    # collision on a shared host does not make the stack unstartable; the numeric
    # filter below would have silently SKIPPED those mappings, which would leave the
    # app port and the database port unchecked while this gate still printed OK.
    mapping="$(printf '%s' "$raw" | sed -E 's/\$\{[A-Za-z_][A-Za-z0-9_]*:-([^}]*)\}/\1/g')"
    # A variable with NO default cannot be resolved, so it cannot be classified —
    # and an unclassifiable mapping must fail rather than be skipped.
    case "$mapping" in
      *'${'*)
        echo "FAIL (H46): $f publishes '$raw', which this gate cannot resolve."
        echo "    A published port may use \${VAR:-default}; a variable with no default"
        echo "    leaves the mapping unknowable here and therefore unchecked."
        rc=1
        continue
        ;;
    esac
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
             | grep -E '^([0-9.]+:)?(\$\{[^}]*\}|[0-9]+):(\$\{[^}]*\}|[0-9]+)$' || true)
done

# ── B. DB_PASSWORD must fail loud (no empty default) in the production compose ─
if [ -f "$PROD_COMPOSE" ] && grep -qE '\$\{DB_PASSWORD\}|\$\{DB_PASSWORD:-' "$PROD_COMPOSE"; then
  echo "FAIL: $PROD_COMPOSE uses \${DB_PASSWORD} without a fail-loud guard."
  echo "    Use \${DB_PASSWORD:?DB_PASSWORD must be set} so an unset password aborts startup."
  rc=1
fi

# ── C. no :latest image tags in compose/k8s/deploy manifests ────────────────
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

# ── D. no readOnlyRootFilesystem: false in k8s/deploy manifests ─────────────
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

echo "OK: deployment artifacts publish only loopback/app ports, DB_PASSWORD fails loud,"
echo "    no :latest pins, no readOnlyRootFilesystem: false."
