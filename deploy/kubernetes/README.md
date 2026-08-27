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

## The plain manifests

`deployment.yaml`, `service.yaml`, `configmap.yaml`, `ingress.yaml`, `hpa.yaml`,
`secrets.yaml.example` and `fraiseql-hardened.yaml` are a hand-maintained copy of
the same deployment for operators who do not use Helm.

⚠ **Nothing executes them.** They are not rendered, not deployed and not queried
by any gate, and they have historically drifted from the chart and from the
image: until 2026-08-27 they named `fraiseql:2.8.0` (an image this project cannot
publish), served port 8815 (the image binds 8000), and mounted no compiled schema
(so the container exited at startup). Those are fixed, but nothing stops them
recurring. Prefer the chart; if you use these, read them first.

Both paths require the same three inputs, for the same reasons:

| Input | Why |
|---|---|
| `DATABASE_URL`, from a Secret | the server has no usable default |
| A compiled schema at `FRAISEQL_SCHEMA_PATH` | the published image bakes none, and the server validates the path before serving |
| `fraiseql.toml` at `FRAISEQL_CONFIG` | `cors_origins` has no environment variable, and production mode refuses to start without it |

Create the schema ConfigMap the plain manifests expect with:

```bash
kubectl create configmap fraiseql-schema \
  --from-file=schema.compiled.json=schema.compiled.json
```

## Probes

`/health` is 503 whenever the database is unreachable, so it is **not** a
process-liveness endpoint despite its doc comment saying so. Both paths use:

- **startup** → `GET /health` (before a pod has served, an unreachable database
  is a startup failure)
- **liveness** → `tcpSocket` (a liveness probe that fails on a dependency
  restarts every pod through a database outage)
- **readiness** → `GET /readiness` (503 while the database is unreachable, so the
  pod leaves the Service's endpoints without being killed)

There is no `/ready` endpoint. The path is `/readiness`.

## Ports

The image binds `0.0.0.0:8000` and declares `EXPOSE 8000`. Every manifest here
uses 8000. It was 8815 in all of them while the process listened on 8000 — see
#1216 and the 2.15.0 changelog.
