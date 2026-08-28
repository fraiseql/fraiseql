# Storage Architecture

The `fraiseql-storage` crate provides object storage for FraiseQL: an HTTP
object API with per-object ownership, backed by S3-compatible services, Google
Cloud Storage, Azure Blob Storage, or the local filesystem.

## Overview

```
HTTP request → Storage Router → key validation → ownership check (RLS)
                                                        ↓
                                   object-metadata table (PostgreSQL)
                                                        ↓
                                 Backend Adapter → S3 / GCS / Azure / Local
```

Two things are worth understanding up front, because most of the behaviour
follows from them:

- **The metadata table is the system of record for ownership.** Every object has a row in
  `_fraiseql_storage_objects`, keyed on `(bucket, key)`, carrying `owner_id`. Access decisions are
  made against that row, not against the object store. Bytes in the backing store with no row are
  not reachable through the API.
- **Object keys are validated, never normalised.** Two keys that differ as strings must never name
  one stored object, or ownership recorded against one spelling would not apply to the other.

## Backends

| Backend | Feature flag | Object CRUD | Presigned URLs | List |
|---|---|---|---|---|
| Amazon S3 / MinIO / Hetzner / Scaleway / OVH / Exoscale / Backblaze / R2 | `aws-s3` | yes | yes | yes |
| Google Cloud Storage | `gcs` | yes | **not implemented** | **not implemented** |
| Azure Blob Storage | `azure-blob` | yes | **not implemented** | **not implemented** |
| Local filesystem | (always available) | yes | no (not a service) | yes |

`GcsBackend::presigned_url` / `list` and `AzureBackend::presigned_url` / `list`
return `FileError::NotImplemented`. Presigned uploads and downloads therefore
work only on the S3-compatible backends.

The local backend is the only one available in a default build; every other
backend needs its Cargo feature enabled.

## Configuration

Storage sections are keyed by name. **The section name is the logical bucket
name that appears in the URL path**, and v1 supports exactly one section —
configuring more than one is a startup error.

```toml
[storage.uploads]
backend = "s3"                 # local | s3 | hetzner | scaleway | ovh | exoscale
                               # | backblaze | r2 | gcs | azure
bucket = "my-app-uploads"      # physical bucket / container
region = "us-east-1"
endpoint = "https://s3.us-east-1.amazonaws.com"   # optional; required for R2
access = "private"             # private (default) | public_read
max_object_bytes = 10485760    # optional
allowed_mime_types = ["image/*", "application/pdf"]  # optional
serve_inline = false           # optional; see "Serving" below
```

Objects in this example are addressed as
`/storage/v1/object/uploads/<key>`.

For the local backend:

```toml
[storage.uploads]
backend = "local"
path = "/var/lib/fraiseql/uploads"
access = "private"
```

Storage requires PostgreSQL: the object-metadata repository needs a
`sqlx::PgPool`, so under any other database backend the routes are not mounted
and the server says so at startup.

### Authentication is required to mount the routes

The storage routes are mounted only when a caller can be authenticated —
`storage_token` is set, or an OIDC validator is configured. With neither, the
server logs a `SECURITY:` error and **does not mount** the routes rather than
expose an anonymous-only object API.

`storage_token` acts as a storage-admin bearer (it grants the
`fraiseql:storage:admin` role). Per-user OIDC tokens populate the request
identity that ownership is evaluated against. A request with no token is
anonymous, which under the access policy below can only read a `public_read`
bucket.

## API

| Method | Path | Operation |
|---|---|---|
| `PUT` | `/storage/v1/object/{bucket}/{key}` | Upload |
| `GET` | `/storage/v1/object/{bucket}/{key}` | Download |
| `DELETE` | `/storage/v1/object/{bucket}/{key}` | Delete |
| `GET` | `/storage/v1/list/{bucket}` | List (`prefix`, `limit`, `offset`) |
| `POST` | `/storage/v1/presign/{bucket}/{key}` | Presigned upload or download URL |
| `GET` | `/storage/v1/render/{bucket}/{key}` | Rendered image (`transforms` feature) |
| `POST` | `/storage/v1/uploads/{bucket}/{key}` | Create a resumable (Tus) upload |
| `PATCH` | `/storage/v1/uploads/{id}` | Append a chunk at the declared offset |
| `HEAD` | `/storage/v1/uploads/{id}` | Current offset (resume point) |
| `DELETE` | `/storage/v1/uploads/{id}` | Cancel an upload |

`{key}` is a wildcard, so it may contain `/`.

### Resumable uploads (Tus 1.0.0)

`POST` with `Upload-Length` (and optionally `Upload-Metadata: filetype <base64>`)
creates a session and returns its `Location`. Each `PATCH` carries
`Content-Type: application/offset+octet-stream` and the `Upload-Offset` the
client believes the server is at; a mismatch is `409`, so a resumed client
must `HEAD` first. The final chunk completes the upload atomically. `DELETE`
cancels.

Sessions are rows in `_fraiseql_storage_uploads`, so an upload survives a
server restart. They obey the same access control as every other upload path:
creation passes the overwrite gate and reserves the metadata row (the key
carries an owner from the first byte), and only the creator can append to,
probe, or cancel a session — anyone else gets the same `404` a nonexistent
session gets.

Per-backend behaviour: local stages the bytes and renames on completion; S3
maps chunks to multipart parts, so **every non-final chunk must be at least
5 MiB** (undersized chunks are refused with `400` rather than failing at
completion). GCS and Azure refuse resumable uploads outright (`501`) until
their resumable APIs are implemented (#972). Sessions expire after
`upload_ttl_secs` (default 24 h): an expired session answers `410` and is
reaped, releasing the key.

## Access control

Access is evaluated by `StorageRlsEvaluator` against the object's metadata row.
**It is scoped by object owner, not by tenant** — the model is:

| Operation | `private` bucket | `public_read` bucket |
|---|---|---|
| Read | owner, or `fraiseql:storage:admin` | anyone, including anonymous |
| Create (no existing object) | any authenticated caller | any authenticated caller |
| Overwrite (object exists) | owner, or `fraiseql:storage:admin` | owner, or `fraiseql:storage:admin` |
| Delete | owner, or `fraiseql:storage:admin` | owner, or `fraiseql:storage:admin` |
| List | filtered to owned objects (all, for an admin) | all objects in the bucket |

The owner is the `sub` claim of the token that created the object. There is no
tenant column and no tenant scoping: two users of the same tenant do not see
each other's objects in a `private` bucket, and a `public_read` bucket is
readable by unauthenticated callers regardless of tenant. If you need
tenant-level rather than user-level sharing, put it in the key space and treat
this document's model as the floor, not the ceiling.

The admin role is the explicit `fraiseql:storage:admin`, never a generic
`admin` scope, so an unrelated application scope of that name cannot
accidentally grant full storage access.

### Presigned uploads own their object

A presigned `PUT` sends the bytes straight to the object store, so the server
never sees them. Signing the URL therefore **claims the object first**: the
metadata row is written with the caller as owner and marked `pending`, which is
what keeps the overwrite rules above applicable to an object the server did not
upload. The first successful read settles the row (recording the real size and
etag) and clears `pending`; `list` reports the flag so an unsettled claim is
distinguishable from a stored object.

Two consequences worth planning for:

- A signed-but-never-used upload leaves a `pending` row holding the key for its owner. It is
  released automatically only if signing itself failed.
- Bucket policy (`max_object_bytes`, `allowed_mime_types`) **cannot** be enforced through a vanilla
  S3 presigned `PUT` — the URL grants the holder the same authority the server has for that
  bucket+key. Restrict presigning to trusted callers, re-validate afterwards, or route uploads
  through `PUT /storage/v1/object/...` instead.

## Bucket policies

A bucket's `access` mode covers two coarse shapes. For anything else, attach a
policy — a list of **permit** rules that *replaces* the access mode for that
bucket:

```toml
[storage.docs]
backend = "local"
path = "/var/lib/fraiseql/docs"

[[storage.docs.policies]]
methods = ["read"]
principal = "role:auditor"
key_prefix = "reports/"

[[storage.docs.policies]]
methods = ["read", "write", "overwrite", "delete", "list"]
principal = "owner"
```

- `methods`: `read` | `write` | `overwrite` | `delete` | `list`.
- `principal`: `owner` | `authenticated` | `anonymous` | `role:<name>`.
- `key_prefix` (optional) narrows a rule to keys under that prefix.

Three properties are deliberate:

**Denial is structural.** There is no `effect = "deny"`. A request is permitted
only when some rule matches it; every other path falls through to denied. An
empty policy denies everything, including to an object's own owner.

**`write` is create-only.** Replacing an existing object requires `overwrite`.
Without the split, the natural rule *"authenticated callers may write"* would
let any authenticated caller clobber any other user's object by writing to its
key — the overwrite IDOR the object-level checks exist to prevent.

A `key_prefix` narrows both halves, against the key each one is about: a `write`
rule is decided against the key being created, an `overwrite` rule against the
key that already exists. So `methods = ["write"], key_prefix = "uploads/"` is
the grant *"may create anything under `uploads/`, and replace nothing"*.

**An unparseable policy does not boot.** An unknown method or principal
spelling, an empty `methods` list, or a misspelled field is a startup error, so
a typo can never become a rule that silently denies (or one that silently
disappears from a policy whose remaining rules permit).

`list` is its own permission: under a policy it is not implied by write access,
and a prefix-scoped `list` grant does not answer the whole-bucket listing
question. Row filtering still applies on top, so a permitted listing returns
only the keys the policy permits reading.

The storage-admin role (`fraiseql:storage:admin`) bypasses policies exactly as
it bypasses the access mode.

### Managing policies at runtime

A policy can also be pushed over the Admin API, without a config deploy:

```
GET    /api/v1/admin/storage/{bucket}/policies    # read token
PUT    /api/v1/admin/storage/{bucket}/policies    # write token
DELETE /api/v1/admin/storage/{bucket}/policies    # write token
```

```bash
curl -X PUT https://api.example.com/api/v1/admin/storage/docs/policies \
  -H "Authorization: Bearer $FRAISEQL_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"rules":[{"methods":["read"],"principal":"role:auditor","key_prefix":"reports/"}]}'
```

Pushed policies live in `_fraiseql_storage_policies`, one row per bucket, and
are loaded at boot. Four rules govern the two sources:

**A stored policy replaces the configured one, wholesale.** They are never
merged. "What can this caller do" stays answerable by reading one list, and
which list is never a guess: `GET` reports a `source` of `store`,
`config_file`, or `access_mode`, and the boot log states it per bucket.

**`DELETE` reverts to the configured policy** — or, for a bucket that declares
none, to the coarse `access` mode. That can *widen* access, which is why it
takes the write token and why the response says which source now governs.

**A pushed policy cannot refuse to boot, so it refuses at the request.** The
rules are validated before anything is written or applied, through the same
parser the config file goes through. A rejection answers `400` naming the
offending `rule_index`, and the policy already in force keeps serving,
untouched. An empty `rules` list is *not* a rejection: it is a valid lock-down
that permits nothing, distinct from having no policy at all.

**Reads and writes are separately tokenable.** `GET` takes
`admin_readonly_token`, `PUT`/`DELETE` take `admin_token`. An operator can hand
out the ability to inspect what governs a bucket without handing out the
ability to change it. (This is why the endpoint lives on `/api/v1/admin/*` and
not beside the Studio storage browser at `/admin/v1/storage/*`, which is a
single-token surface.)

A push applies immediately on the replica that served it; other replicas pick
it up within 30 seconds. A stored row that this server cannot parse — one
hand-edited in SQL, or written by a different version — refuses the boot, and
on a running server is logged and skipped, leaving the policy in force
untouched rather than silently reverting the bucket.

## Key validation

`validate_key` rejects — rather than rewrites — any key that could alias onto
another. Canonicalising would be worse than rejecting: it would silently merge
two keys the client believes are distinct.

A key must be a non-empty, `/`-separated relative path where every segment:

- is non-empty (so no leading `/`, trailing `/`, or `//`),
- is not `.` or `..`,
- has no leading or trailing whitespace and no trailing `.`,

and the key as a whole contains no backslash, no control byte (including NUL),
and no percent-escape that decodes to path syntax (`%2e`, `%2f`, `%5c`, `%25`).

The local backend additionally resolves each path before any I/O and refuses
anything that leaves the storage root, or whose final component is a symlink —
key validation is lexical and cannot see a symlink planted inside the root.

## Serving

- **`Cache-Control`** depends on the bucket's access mode. A `private` bucket serves `private,
  no-store`: its read decision is per-object, so a shared cache keyed on the URL cannot represent
  the boundary. A `public_read` bucket serves `public, max-age=3600`.
- **`X-Content-Type-Options: nosniff`** is always set.
- **`Content-Disposition`** is `attachment` by default. A bucket may set `serve_inline = true`, but
  content types a browser can execute as active content (`text/html`, `image/svg+xml`,
  `application/xml`, …) stay `attachment` regardless.
- **Not-found and not-yours are the same answer**, so the status code cannot be used to enumerate
  the keys in a private bucket.

## MIME allow-list

`allowed_mime_types` matches the media type only: parameters are ignored and
comparison is case-insensitive, so an entry of `text/plain` accepts
`text/plain;charset=UTF-8`. Entries may be exact (`application/pdf`), a subtype
wildcard (`image/*`), or `*/*`. Omitting the key means no restriction; an
**empty list allows nothing**.

## Transforms

Build the server with `--features storage-transforms` to mount

```
GET /storage/v1/render/{bucket}/{key}?w=&h=&mode=&gravity=&background=&crop=
                                     &blur=&sharpen=&watermark=&format=&quality=&preset=
```

which reads the stored object through exactly the gates the download route
uses and returns a transformed image. The pipeline is **crop → resize →
blur/sharpen → watermark → encode**.

### Resize modes

`mode` chooses how the `w`×`h` box is filled. Only `contain` returns something
other than exactly that box.

| `mode` | Output | Behaviour |
|---|---|---|
| `contain` (default) | scaled size | Fit inside the box, aspect preserved, no padding or cropping. |
| `stretch` | exactly `w`×`h` | Scale to the box, ignoring the source's aspect ratio. |
| `fit` | exactly `w`×`h` | Fit inside, then letterbox with `background` (`#rrggbb` or `#rrggbbaa`). |
| `fill` | exactly `w`×`h` | Scale to cover, then crop the overflow at `gravity`. |
| `cover-blur` | exactly `w`×`h` | Letterbox, bars filled with a blurred, box-covering copy of the image. |
| `cover-mirror` | exactly `w`×`h` | Letterbox, bars filled by mirroring the image's own edges. |
| `retarget` | exactly `w`×`h` | Seam carving (`transforms-retarget` feature); see below. |

Every mode but `contain` needs **both** `w` and `h` — they are defined by the
box they fill, and inventing the missing side would render something the caller
did not ask for. A bucket may set `default_resize_mode`; an explicit `mode=`
still wins. An unknown mode is a `400`, never a fallback to the default.

`gravity` is a compass point (`north`, `south-east`, …), `center`, or `smart`.
`smart` is resolved from the pixels: the window carrying the most edge energy,
searched on a fixed 128 px grid so it costs the same for any source size.

### Crop, effects and watermarks

- `crop=x,y,w,h` takes an exact rectangle; `crop=w:h` takes the largest rectangle of that aspect
  ratio, positioned by `gravity`. A rectangle outside the source is refused, never clamped.
- `blur=` and `sharpen=` are Gaussian radii.
- `watermark=<key>` composites another object **from the same bucket**, through the same read gate
  — a watermark the caller cannot read answers exactly like a missing object.
- `watermark_text=` rasterises text using the bucket's `watermark_font`. FraiseQL vendors no
  typeface: point `watermark_font` at a font file and it is read and parsed at boot, so a missing
  or unreadable font refuses to start. A bucket without one refuses `watermark_text` by name.
- `watermark_scale` (fraction of canvas width, default `0.25`), `watermark_color`,
  `watermark_size`, `watermark_inset` tune the mark.

### Quality

`quality` (1–100) reaches the encoder for **`jpeg` and `avif`**. PNG is lossless
by definition and this server's WebP encoder writes lossless WebP, so a quality
on either could never take effect: it is refused by name at request time, and a
preset that pairs them refuses at boot. Ask for `jpeg` or `avif`, or drop the
parameter.

### Presets

```toml
[storage.media]
backend = "s3"
default_resize_mode = "fill"
transform_presets = [
    { name = "thumb", width = 200, height = 200, resize_mode = "fill", gravity = "smart", format = "jpeg", quality = 80 },
]
```

Explicit query parameters override a named preset's fields. Every preset
spelling — mode, gravity, the quality/format pairing — is validated at boot.
Declaring `transform_presets`, `default_resize_mode` or `watermark_font` in a
binary built *without* `storage-transforms` is a startup error, not a silently
absent endpoint.

**Resource bounds.** Source and requested dimensions are both capped at 12 000
pixels per side, and the decoder runs under matching hard limits. A
decompression bomb — a small file whose header declares an enormous image — is
refused by the dimension guard before any allocation, and returns `400`, as do
non-image objects, malformed bytes, and absurd target sizes.

Each operation carries its own bound, because a size cap does not bound them
all:

- **Blur and sharpen** cost `pixels × radius`, so the budget is on that product, not on the radius
  alone. A 500×500 render may ask for a large radius; a 4000×4000 one may not ask for the same
  one, and is told the maximum for its size.
- **Watermarks** cannot be scaled past the canvas they are drawn on.
- **Text** is capped in both type size and character count, since the raster is allocated from
  them.
- **`retarget`** is the one operation whose cost is not bounded by its output — seam carving is
  `O(pixels × seams)`. It is bounded three ways: an aspect-delta threshold past which the request
  renders as `fill` instead, a working-resolution cap so the seam count is a constant, and a
  wall-clock budget. Falling back is a different but valid rendering, never an error and never a
  half-carved image.

A hostile upload, or a hostile query string, cannot exhaust the server through
this endpoint.

**Caching.** A render is a pure function of the source bytes and the resolved
transform, so results are cached under a content-addressed key covering both.
Invalidation is therefore structural: a re-uploaded source hashes differently
and reads a different key, so a stale entry is unreachable rather than merely
marked. Entries live under the reserved `.fraiseql-transforms/` prefix, a name
no bucket may take. Responses still carry an `ETag` and a bucket-appropriate
`Cache-Control` (`private, no-store` for private buckets) so HTTP caches do
their share.

Every render logs the resolved transform and whether it was a cache hit —
the same canonical string the cache key is derived from, so the audit trail and
the cache cannot disagree about what was served.

Note that **stored objects are still served as uploaded**: rendering
re-encodes through the `image` crate and so drops EXIF (including GPS) as a
side effect, but a plain `GET /storage/v1/object/...` returns the original
bytes with metadata intact. If you accept user photographs and need their GPS
tags removed at rest, strip them before upload or in your own pipeline.

## Local development with emulators

All three cloud backends honour the `endpoint` field, so the standard emulators
can be used for local development and CI:

| Backend | Emulator | Example `endpoint` |
|---------|----------|--------------------|
| S3 | [MinIO](https://min.io/) | `http://localhost:9000` |
| Azure Blob | [Azurite](https://github.com/Azure/Azurite) | `http://127.0.0.1:10000/devstoreaccount1` |
| GCS | [fake-gcs-server](https://github.com/fsouza/fake-gcs-server) | `http://localhost:4443` |

For Azure Blob the endpoint is the account-level base (Azurite serves the
account as a path segment, e.g. `devstoreaccount1`):

```toml
[storage.uploads]
backend = "azure"
account_name = "devstoreaccount1"
bucket = "uploads"           # container name
endpoint = "http://127.0.0.1:10000/devstoreaccount1"
```

When `endpoint` is omitted the backends target the production hostnames
(`*.blob.core.windows.net`, `storage.googleapis.com`), so real-cloud
deployments need no change.

## Testing

The object-safety properties above are gated by
`crates/fraiseql-server/tests/storage_minio_integration_test.rs`, which drives
the router over MinIO plus a real metadata table. In CI it is the
`server-storage` integration suite (`dagger call test-integration
--suite=server-storage`), which binds both services. Locally:

```bash
docker run -d -p 9000:9000 -e MINIO_ROOT_USER=minioadmin \
    -e MINIO_ROOT_PASSWORD=minioadmin minio/minio server /data
export MINIO_ENDPOINT=http://127.0.0.1:9000 DATABASE_URL=postgres://…
export AWS_ACCESS_KEY_ID=minioadmin AWS_SECRET_ACCESS_KEY=minioadmin \
       AWS_DEFAULT_REGION=us-east-1
cargo test -p fraiseql-server --features aws-s3 \
    --test storage_minio_integration_test -- --test-threads=1
```

The crate's own suites (`metadata`, `migrations`, `routes`, and the Azurite /
fake-gcs round-trips) run in the `storage` integration suite and need
`DATABASE_URL` plus `--test-threads=1`.

## Crate dependencies

```
fraiseql-storage
├── fraiseql-error
├── sqlx (object metadata, PostgreSQL)
├── aws-sdk-s3 / aws-config (optional, aws-s3 feature)
├── reqwest + jsonwebtoken (optional, gcs feature)
├── reqwest + hmac + base64 + urlencoding (optional, azure-blob feature)
└── image + kamadak-exif (optional, transforms feature)
```

## See Also

- [Functions Architecture](functions.md) -- Serverless functions runtime
- [Architecture Overview](overview.md) -- System-wide architecture
