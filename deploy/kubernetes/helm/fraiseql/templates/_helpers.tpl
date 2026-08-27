{{/*
Expand the name of the chart.
*/}}
{{- define "fraiseql.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "fraiseql.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "fraiseql.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "fraiseql.labels" -}}
helm.sh/chart: {{ include "fraiseql.chart" . }}
{{ include "fraiseql.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "fraiseql.selectorLabels" -}}
app.kubernetes.io/name: {{ include "fraiseql.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "fraiseql.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "fraiseql.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Name of the Secret holding the database URL.

Two supported shapes, and NOTHING renders unless one of them is chosen:

  database.existingSecret  — you create the Secret, the chart only references it.
  database.url             — the chart creates the Secret from this value.

⚠ Why this fails loudly instead of defaulting. Until #1129's follow-up the chart
referenced `<fullname>-db` unconditionally and shipped no Secret template at all,
so `helm install` on the defaults rendered cleanly, was accepted by the API
server, and then sat in CreateContainerConfigError forever — a chart that lints,
templates and installs, and cannot start a pod. A template-time failure with an
instruction is strictly better than a green install that never serves.
*/}}
{{- define "fraiseql.databaseSecretName" -}}
{{- if .Values.database.existingSecret -}}
{{- .Values.database.existingSecret -}}
{{- else if .Values.database.url -}}
{{- printf "%s-db" (include "fraiseql.fullname" .) -}}
{{- else -}}
{{- fail "fraiseql: no database configured. Set database.url (the chart creates the Secret) OR database.existingSecret (you create it, with the key named by database.existingSecretKey). Neither is set, and a Deployment referencing a Secret that does not exist never starts a pod." -}}
{{- end -}}
{{- end -}}

{{/*
Key within the database Secret that holds the connection URL.
*/}}
{{- define "fraiseql.databaseSecretKey" -}}
{{- if .Values.database.existingSecret -}}
{{- required "fraiseql: database.existingSecretKey must name the key inside database.existingSecret." .Values.database.existingSecretKey -}}
{{- else -}}
{{- "url" -}}
{{- end -}}
{{- end -}}

{{/*
Name of the ConfigMap holding the compiled schema.

Same two shapes as the database, and the same reason: fraiseql-server reads a
COMPILED schema at FRAISEQL_SCHEMA_PATH and exits non-zero at startup when the
file is absent (crates/fraiseql-server/src/main.rs, validate_schema_path). The
published image bakes no schema — it cannot, the schema is the deployment's, not
the engine's — so a chart that does not mount one deploys a container that exits
immediately. That is the #1071 shape.
*/}}
{{- define "fraiseql.schemaConfigMapName" -}}
{{- if .Values.schema.existingConfigMap -}}
{{- .Values.schema.existingConfigMap -}}
{{- else if .Values.schema.compiled -}}
{{- printf "%s-schema" (include "fraiseql.fullname" .) -}}
{{- else -}}
{{- fail "fraiseql: no compiled schema configured. Set schema.compiled (usually `--set-file schema.compiled=schema.compiled.json`, and the chart creates the ConfigMap) OR schema.existingConfigMap (you create it, with the key named by schema.key). fraiseql-server exits at startup when FRAISEQL_SCHEMA_PATH names no file." -}}
{{- end -}}
{{- end -}}

{{/*
Key within the schema ConfigMap, which is also the file name it is mounted as.
*/}}
{{- define "fraiseql.schemaKey" -}}
{{- required "fraiseql: schema.key must name the key inside the schema ConfigMap." .Values.schema.key -}}
{{- end -}}

{{/*
Directory the schema ConfigMap is mounted at, and the full path the server reads.
*/}}
{{- define "fraiseql.schemaMountDir" -}}
{{- "/etc/fraiseql/schema" -}}
{{- end -}}

{{- define "fraiseql.schemaPath" -}}
{{- printf "%s/%s" (include "fraiseql.schemaMountDir" .) (include "fraiseql.schemaKey" .) -}}
{{- end -}}

{{/*
Name of the ConfigMap holding fraiseql.toml, or "" when no config is supplied.

Unlike the database and the schema this is OPTIONAL — but only outside
production mode. See fraiseql.requireProductionConfig.
*/}}
{{- define "fraiseql.configConfigMapName" -}}
{{- if .Values.config.existingConfigMap -}}
{{- .Values.config.existingConfigMap -}}
{{- else if .Values.config.content -}}
{{- printf "%s-config" (include "fraiseql.fullname" .) -}}
{{- end -}}
{{- end -}}

{{- define "fraiseql.configKey" -}}
{{- required "fraiseql: config.key must name the key inside the config ConfigMap." .Values.config.key -}}
{{- end -}}

{{- define "fraiseql.configMountDir" -}}
{{- "/etc/fraiseql/config" -}}
{{- end -}}

{{- define "fraiseql.configPath" -}}
{{- printf "%s/%s" (include "fraiseql.configMountDir" .) (include "fraiseql.configKey" .) -}}
{{- end -}}

{{/*
Refuse to render a Deployment whose pod cannot start.

fraiseql-server validates its configuration before it serves, and in production
mode — which is the DEFAULT, anything but FRAISEQL_ENV=development — it refuses
to start while CORS is enabled with no origins configured
(crates/fraiseql-server/src/server_config/methods.rs). cors_origins has no
environment variable; it is set in fraiseql.toml, which reaches the server as
FRAISEQL_CONFIG. So a chart that mounts no config file produces a pod that
CrashLoopBackOffs on its first line of output, which is what this chart did.

⚠ This guard mirrors a rule that lives in the server. If that rule changes, this
message goes stale — it is quoted here rather than paraphrased so a grep for the
error text finds both.
*/}}
{{- define "fraiseql.requireProductionConfig" -}}
{{- $env := default "production" (get .Values.env "FRAISEQL_ENV") -}}
{{- if and (ne $env "development") (not (include "fraiseql.configConfigMapName" .)) -}}
{{- fail "fraiseql: no configuration file supplied, and this is a production-mode deployment. fraiseql-server exits at startup with \"cors_enabled is true but cors_origins is empty in production mode\" — cors_origins has no environment variable, it is set in fraiseql.toml. Either set config.content (usually `--set-file config.content=fraiseql.toml`) or config.existingConfigMap, or, for a non-production deployment, set env.FRAISEQL_ENV=development." -}}
{{- end -}}
{{- end -}}
