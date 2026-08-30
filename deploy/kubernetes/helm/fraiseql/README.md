# FraiseQL Helm Chart

Deploys `fraiseql-server` — the compiled GraphQL execution engine — against a
PostgreSQL database you provide.

## What this chart creates

Exactly these objects, and nothing else:

| Object | Always? |
|---|---|
| `Deployment` | yes |
| `Service` (ClusterIP) | yes |
| `ConfigMap` (compiled schema) | unless `schema.existingConfigMap` is set |
| `ConfigMap` (`fraiseql.toml`) | when `config.content` is set |
| `Secret` (database URL) | unless `database.existingSecret` is set |
| `HorizontalPodAutoscaler` | when `autoscaling.enabled` |
| `ServiceAccount` | when `serviceAccount.create` |

There is **no Ingress, PodDisruptionBudget, PersistentVolumeClaim or
NetworkPolicy template**, and no value pretending there is. An earlier version of
this chart shipped `ingress.enabled`, `podDisruptionBudget.enabled`,
`persistence.enabled` and `serviceAccount.create` with no templates behind them:
rendering with all four set to `true` produced output byte-identical to the
default render. If you need an Ingress, write one — it will be correct, which is
more than a value that silently does nothing was.

`tools/chart-deploy-test.sh` enforces this: for every `<path>.enabled` key in
`values.yaml` it renders the chart both ways and requires the renders to differ.

## Requirements

- Kubernetes 1.21+, Helm 3.8+ (CI pins helm 3.21.4; helm 4 is untested)
- A reachable PostgreSQL database
- A **compiled** schema — `fraiseql-cli compile schema.json` — because the
  published image bakes none

## Install

The chart refuses to render until it can produce a pod that starts. Three inputs
are required; each missing one fails at template time with an instruction rather
than installing something that never serves.

```bash
# 1. Compile your schema
fraiseql-cli compile schema.json          # -> schema.compiled.json

# 2. Minimal fraiseql.toml. cors_origins has no environment variable, and
#    fraiseql-server refuses to start in production mode without it.
cat > fraiseql.toml <<'EOF'
cors_origins = ["https://app.example.com"]
EOF

# 3. Install
helm install fraiseql ./deploy/kubernetes/helm/fraiseql \
  --set-file schema.compiled=schema.compiled.json \
  --set-file config.content=fraiseql.toml \
  --set database.existingSecret=fraiseql-db-credentials
```

with the database Secret created out of band:

```bash
kubectl create secret generic fraiseql-db-credentials \
  --from-literal=url='postgresql://user:password@postgres:5432/fraiseql'
```

For a throwaway cluster you can let the chart create the Secret instead — the URL
then lives in your values file and in Helm's release storage:

```bash
  --set-string database.url='postgresql://user:password@postgres:5432/fraiseql'
```

Outside production, `--set env.FRAISEQL_ENV=development` removes the
`config.content` requirement.

## Configuration

`values.yaml` documents every key, and every key is read by a template. Two of
them carry most of the configuration surface:

- **`env`** — a map passed to the container verbatim. This is how you set any
  `FRAISEQL_*` variable (`FRAISEQL_RATE_LIMITING_ENABLED`,
  `FRAISEQL_INTROSPECTION_ENABLED`, `RUST_LOG`, …). There are no typed value
  blocks mirroring a subset of them; the previous chart had some, they were wired
  to nothing, and they could never have covered the list.
- **`config.content`** — `fraiseql.toml`, for everything with no environment
  variable, `cors_origins` above all.

### Ports

`application.port` (default **8000**) is the single number: the container's named
port, the port the Service targets, the port every probe hits, and the address
the server is told to bind. The Deployment derives `FRAISEQL_BIND_ADDR` from it,
so changing it moves everything together. It was previously four independent
numbers and the image's own was not among them (#1216).

### Probes

- **startup** → `GET /health`. Before a pod has served, an unreachable database
  is a startup failure.
- **liveness** → **`GET /live`**, not `/health`. `/health` answers 503 whenever
  the database check fails, so a liveness probe on it restarts every pod for as
  long as PostgreSQL is unreachable — a failover becomes a restart storm on top
  of the outage. Liveness must detect a broken process, not a broken dependency.
  `/live` makes no dependency call at all. It replaces the `tcpSocket` stopgap
  this chart shipped in 2.15.0, which could not see a process deadlocked but
  still holding its listener (#1217).
- **readiness** → `GET /readiness`. 503 while the database is unreachable, so an
  affected pod leaves the Service's endpoints without being killed.

## What CI verifies

`tools/chart-deploy-test.sh`, on the `Dagger — image` leg, on every push to `dev`
and `release/*`:

1. every `image:` the default chart renders names a repository this project
   publishes;
2. every values toggle changes the render;
3. the chart refuses to render without a database, a schema, or (in production
   mode) a config;
4. every rendered object is accepted by a real Kubernetes API server;
5. the chart **installs into a throwaway k3s cluster** on the image that build
   produced, and answers a GraphQL query through its Service;
6. a row inserted into PostgreSQL *after* the pod is serving comes back out of
   the next query.

Step 6 is the one that matters. Everything before it is satisfiable by a release
serving a cached or fabricated answer.

`helm.yml` — which ran `helm lint` and rendered into `/dev/null` — was deleted
rather than repaired. A lint never resolves an image, and that is how this chart
shipped an unpullable default for several releases (#1129).

## Uninstall

```bash
helm uninstall fraiseql
```
