# Kubernetes deployment

FraiseQL v2 ships **one supported Kubernetes artifact: the Helm chart** in
[`helm/fraiseql`](helm/fraiseql/README.md). It is the only one CI deploys and
queries.

## Quick start

```bash
fraiseql-cli compile schema.json          # -> schema.compiled.json

cat > fraiseql.toml <<'EOF'
cors_origins = ["https://app.example.com"]
EOF

kubectl create secret generic fraiseql-db-credentials \
  --from-literal=url='postgresql://user:password@postgres:5432/fraiseql'

helm install fraiseql ./helm/fraiseql \
  --set-file schema.compiled=schema.compiled.json \
  --set-file config.content=fraiseql.toml \
  --set database.existingSecret=fraiseql-db-credentials
```

The chart's [README](helm/fraiseql/README.md) documents every value, what the
chart does and does not create, and what CI verifies.

## Why there is only the chart

There used to be a second, hand-maintained copy of the same deployment here —
`deployment.yaml`, `service.yaml`, `configmap.yaml`, `ingress.yaml`, `hpa.yaml`,
`fraiseql-hardened.yaml` — and a third at the repository root in `k8s/`. Neither
was rendered, deployed or queried by any gate, and both drifted: they named
images this project cannot publish (`fraiseql:2.8.0`, `fraiseql/fraiseql-server:2.8.0`),
served a port the image does not bind, mounted no compiled schema, and put the
liveness probe on `/health`, which 503s when the database is unreachable and so
restart-storms a healthy process (#1217).

The root `k8s/` copy still carried every one of those on the day it was deleted,
three weeks after the same defects were fixed in the chart — which is the argument
against keeping a duplicate no gate executes (#1129, #1218).

The chart is deployed and queried by `tools/chart-deploy-test.sh` on every run of
the image leg. If you do not use Helm, `helm template ./helm/fraiseql` renders the
same manifests for `kubectl apply -f -`, from the one definition CI actually
exercises.

## Probes

`/health` is 503 whenever the database is unreachable, so it is **not** a
process-liveness endpoint — its own doc comment used to say it was, and every
manifest here followed that (#1217). Both paths use:

- **startup** → `GET /health` (before a pod has served, an unreachable database
  is a startup failure)
- **liveness** → `GET /live` (always 200, no dependency call; a liveness probe
  that fails on a dependency restarts every pod through a database outage, which
  restarting cannot fix)
- **readiness** → `GET /readiness` (503 while the database is unreachable, so the
  pod leaves the Service's endpoints without being killed)

There is no `/ready` endpoint. The path is `/readiness`, and the liveness path is
`/live` — both configurable (`readiness_path`, `liveness_path`).

## Ports

The image binds `0.0.0.0:8000` and declares `EXPOSE 8000`. Every manifest here
uses 8000. It was 8815 in all of them while the process listened on 8000 —
see #1216 and the 2.15.0 changelog.
