#!/usr/bin/env bash
# check-examples-integrity.sh — fail when a shipped example cannot possibly run.
#
# Background (Phase 09 of the 2026-08-22 program, issues #1050-#1054, #1071-#1073, #1168):
# a nine-issue audit of `examples/` found that essentially every documented entry point
# was dead, and that NOTHING in CI would have noticed. The whole coverage of `examples/`
# before this gate was one clippy run over the single example that is a Rust crate, plus
# three greps (`check-examples-postgres-only.sh`, `check-docs-env-vars.sh`,
# `check-route-syntax.sh`). Examples are the copy a reader actually runs; they were the
# least-checked surface in the repository.
#
# This is the static tier. It runs in preflight and needs no toolchain, no database and
# no Docker. Two further tiers exist because the defects below are not the only shapes:
#
#   * `check-examples-compile.sh` — every tracked authoring artifact must compile.
#   * the examples smoke leg      — an example must actually boot and answer a query,
#     and every example's SQL must load under ON_ERROR_STOP=1.
#
# Each check below is anchored to the defect that motivated it, so a future reader can
# tell whether a failure is a real regression or a rule that has outlived its cause.
#
# ⚠ Do not use `git ls-files` here. The Dagger ShellGates leg ignores `.git` and runs
# `git init -q .` on the copied source, so the index is EMPTY in the container and every
# `git ls-files` loop silently iterates over nothing — a gate that cannot fail. `find`
# sees the same tree in both places.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

found=0

fail() {
    echo "✗ $1"
    shift
    printf '%s\n' "$@" | sed 's/^/    /'
    found=1
}

# ---------------------------------------------------------------------------
# 1. Compose bind-mount sources must exist, resolved the way Compose resolves them.
#
# #1052: `make demo-start` runs `docker compose -f docker/docker-compose.demo.yml up`.
# Compose resolves relative host paths against the PROJECT DIRECTORY, which defaults to
# the directory of the first `-f` file — `docker/` — not the current directory. So
# `./examples/basic` resolved to `docker/examples/basic`, which does not exist, and
# Docker silently created an empty directory and mounted that. Every documented Docker
# onboarding path in the repo was serving an empty schema directory. Proven with
# `docker compose -f docker/docker-compose.demo.yml config`, which printed the resolved
# source path.
# ---------------------------------------------------------------------------
echo "→ compose bind-mount sources resolve to something that exists"
while IFS= read -r compose; do
    dir="$(dirname "$compose")"
    while IFS= read -r src; do
        [ -n "$src" ] || continue
        # Generated-before-use artifacts. Each entry needs a reason: the mount is
        # legitimate only because a documented step writes the file before the
        # service that mounts it is started. `docker compose up` on its own still
        # creates an empty DIRECTORY here, so keep this list short.
        case "$dir/$src" in
            # `make supergraph` (rover) writes this before `make run` starts the router.
            examples/async-jobs-subgraph/router/supergraph.graphql) continue ;;
        esac
        if [ ! -e "$dir/$src" ]; then
            fail "$compose mounts a host path that does not exist: $src" \
                 "resolved against the compose file's directory as $dir/$src" \
                 "Compose would create an empty directory here and mount that."
        fi
    done < <(grep -oE '^[[:space:]]*-[[:space:]]*\./[^:]+:' "$compose" \
             | sed -E 's|^[[:space:]]*-[[:space:]]*\./||; s|:$||')
done < <(find docker examples -name 'docker-compose*.yml' -o -name 'compose*.yml' | sort)

# ---------------------------------------------------------------------------
# 2. A health gate must not match the string it exists to reject.
#
# #1073: `docker-compose ps | grep "$service" | grep -q "healthy"` returns success for a
# container reported `Up (unhealthy)`, because `healthy` is a substring of `unhealthy`.
# The same defect was in the sibling example as `grep -q "bank-service.*healthy"`, where
# `.*` absorbs the `(un`. This is the `gate-cannot-fail` shape that Phase 01 of this
# program was entirely about, reached through a shell script instead of a CI leg.
# ---------------------------------------------------------------------------
echo "→ health checks in example scripts cannot match \"unhealthy\""
if hits=$(grep -rn 'healthy' --include='*.sh' examples/ \
          | grep -E 'grep' \
          | grep -vE 'unhealthy|\(un\)\?healthy|\[^n\]healthy|\bhealthy\b\)"|-w ' \
          | grep -vE '^[^:]+:[0-9]+:[[:space:]]*#' || true); [ -n "$hits" ]; then
    while IFS= read -r line; do
        # A grep whose pattern excludes the unhealthy case is fine. Everything else
        # is reported: anchoring is cheap and the failure mode is silent.
        case "$line" in
            *'grep -vq'*|*'! grep'*) continue ;;
        esac
        fail "unanchored health match — \"healthy\" also matches \"(unhealthy)\":" "$line"
    done <<< "$hits"
fi

# ---------------------------------------------------------------------------
# 3. An example build step may not swallow its own failure.
#
# #1071: `RUN fraiseql compile schema.json -o schema.compiled.json || true` compiled a
# file that no step ever generates. The `|| true` turned a certain failure into a green
# build shipping an image with no compiled schema, whose server then could not boot. This
# is the `fabricated-success` shape: the only reason anyone ever saw a green build here.
# ---------------------------------------------------------------------------
echo "→ no example build step swallows its exit status"
if hits=$(grep -rn -E '^[[:space:]]*RUN .*\|\|[[:space:]]*(true|:)[[:space:]]*$' \
          --include='Dockerfile*' examples/ || true); [ -n "$hits" ]; then
    fail "an example Dockerfile RUN discards its exit status:" "$hits" \
         "If the step is genuinely optional, say so in a comment and make the" \
         "fallback produce the artifact; otherwise let it fail."
fi

# ---------------------------------------------------------------------------
# 4. Every COPY source must exist in the build context that Compose gives the image.
#
# #1050: four federation Dockerfiles begin `COPY fraiseql-cli ./fraiseql-cli` while
# docker-compose.yml sets `context: ./users-service`, a directory holding exactly
# `Dockerfile` and `schema.py`. The build cannot get past its third instruction. The
# context comes from the compose file, so the check has to read the pair, not the
# Dockerfile alone.
# ---------------------------------------------------------------------------
echo "→ example COPY sources exist in the image's build context"

# The build context comes from the compose file, and `dockerfile:` is resolved
# relative to that CONTEXT, not to the compose file — `context: ../..` with
# `dockerfile: examples/…/Dockerfile` is the working form in this repo. Reading the
# Dockerfile alone gets this wrong in both directions: it flags a repo-root build as
# missing its own sources, and it lets a per-service context off entirely. So walk the
# compose `build:` blocks first and let any unclaimed Dockerfile default to its own
# directory.
#
# gawk state machine: `build:` opens a block, and any key indented no deeper than
# `build:` itself closes it.
compose_builds() {
    gawk '
        function flush() {
            if (inb && ctx != "") { print ctx "\t" (df == "" ? "Dockerfile" : df) }
            inb = 0; ctx = ""; df = ""
        }
        /^[[:space:]]*build:[[:space:]]*$/ { flush(); inb = 1; match($0, /^[[:space:]]*/); bi = RLENGTH; next }
        inb {
            match($0, /^[[:space:]]*/)
            if ($0 ~ /^[[:space:]]*$/) next
            if (RLENGTH <= bi) { flush(); next }
            if ($0 ~ /^[[:space:]]*context:/)    { ctx = $0; sub(/^[[:space:]]*context:[[:space:]]*/, "", ctx) }
            if ($0 ~ /^[[:space:]]*dockerfile:/) { df  = $0; sub(/^[[:space:]]*dockerfile:[[:space:]]*/, "", df) }
        }
        END { flush() }
    ' "$1"
}

declare -A CLAIMED=()
while IFS= read -r compose; do
    cdir="$(dirname "$compose")"
    while IFS=$'\t' read -r ctx df; do
        [ -n "$ctx" ] || continue
        abs_ctx="$(cd "$cdir" && cd "$ctx" 2>/dev/null && pwd || true)"
        if [ -z "$abs_ctx" ]; then
            fail "$compose declares a build context that does not exist: $ctx"
            continue
        fi
        abs_df="$abs_ctx/$df"
        if [ ! -f "$abs_df" ]; then
            fail "$compose names a Dockerfile that does not exist: $df" \
                 "resolved against its context as ${abs_df#"$REPO_ROOT"/}"
            continue
        fi
        CLAIMED["$abs_df"]="$abs_ctx"
    done < <(compose_builds "$compose")
done < <(find docker examples -name 'docker-compose*.yml' -o -name 'compose*.yml' | sort)

while IFS= read -r dockerfile; do
    abs_df="$REPO_ROOT/$dockerfile"
    ctx="${CLAIMED[$abs_df]:-$REPO_ROOT/$(dirname "$dockerfile")}"
    while IFS= read -r src; do
        [ -n "$src" ] || continue
        case "$src" in --from=*|--chown=*|/*) continue ;; esac
        if [ ! -e "$ctx/$src" ]; then
            fail "$dockerfile copies a path absent from its build context: $src" \
                 "build context: ${ctx#"$REPO_ROOT"/}"
        fi
    done < <(grep -E '^COPY ' "$dockerfile" \
             | grep -v -- '--from=' \
             | sed -E 's/^COPY[[:space:]]+(--chown=[^[:space:]]+[[:space:]]+)?//' \
             | awk '{ for (i = 1; i < NF; i++) print $i }')
done < <(find examples -name 'Dockerfile*' | sort)

# ---------------------------------------------------------------------------
# 5. Exec-form CMD/ENTRYPOINT does not expand ${VAR}.
#
# #1050: `CMD ["fraiseql", "server", …, "--database", "${DATABASE_URL}", …]`. JSON-form
# CMD does not go through a shell, so the process receives the eleven literal characters
# `${DATABASE_URL}` as its connection string.
# ---------------------------------------------------------------------------
echo "→ no unexpandable \${VAR} in exec-form CMD/ENTRYPOINT"
while IFS= read -r dockerfile; do
    if hits=$(awk '/^(CMD|ENTRYPOINT) *\[/,/\]/' "$dockerfile" | grep -n '\${' || true); [ -n "$hits" ]; then
        fail "$dockerfile passes \${VAR} in exec form, where no shell expands it:" "$hits"
    fi
done < <(find examples -name 'Dockerfile*' | sort)

# ---------------------------------------------------------------------------
# 6. A healthcheck may only use a binary the image installs.
#
# #1073: three subgraphs healthcheck with `curl -f` on top of `python:3.11-slim`, whose
# only install line is `pip install flask graphene psycopg2-binary`. curl is not in that
# image, so the probe fails at exec and the container is `unhealthy` forever — which is
# how the unanchored grep in check 2 came to matter.
# ---------------------------------------------------------------------------
echo "→ healthcheck binaries are installed by the image that uses them"
while IFS= read -r compose; do
    dir="$(dirname "$compose")"
    # Compose healthchecks naming curl/wget, matched to the image or build dir that
    # would have to provide them.
    if grep -qE '^[[:space:]]*test:.*(curl|wget)' "$compose"; then
        while IFS= read -r img; do
            case "$img" in
                python:*-slim|python:*-alpine|debian:*-slim|alpine*)
                    fail "$compose healthchecks with curl/wget over a base image that ships neither: $img" \
                         "Install it in the image, or probe with the interpreter the image already has." ;;
            esac
        done < <(grep -E '^[[:space:]]*image:[[:space:]]*' "$compose" | sed -E 's/.*image:[[:space:]]*//' | sort -u)
        while IFS= read -r ctx; do
            df="$dir/${ctx#./}/Dockerfile"
            [ -f "$df" ] || continue
            base="$(grep -m1 '^FROM ' "$df" | awk '{print $2}')"
            case "$base" in
                python:*-slim|python:*-alpine|debian:*-slim|alpine*) ;;
                *) continue ;;
            esac
            grep -qE '(apt-get|apk|yum|dnf).*(curl|wget)' "$df" || \
                fail "$compose healthchecks with curl/wget, but $df never installs it (base: $base)" \
                     "The probe fails at exec, so the container is unhealthy forever."
        done < <(grep -E '^[[:space:]]*context:[[:space:]]*' "$compose" | sed -E 's/.*context:[[:space:]]*//' | sort -u)
    fi
done < <(find examples -name 'docker-compose*.yml' -o -name 'compose*.yml' | sort)

# ---------------------------------------------------------------------------
# 7. psql includes must be script-relative.
#
# #1051: `examples/mutation-patterns/schema.sql:88` is `\i sql/helpers/…`. psql resolves
# `\i` against the process's working directory — only `\ir` is relative to the including
# script. Run the way its own README documents, the include misses, and because the
# README also sets no ON_ERROR_STOP, psql prints one error, keeps going, and EXITS 0
# with four of the five functions the test script calls undefined.
# ---------------------------------------------------------------------------
echo "→ psql includes in examples are script-relative (\\ir, not \\i)"
if hits=$(grep -rn '^\\i ' --include='*.sql' examples/ || true); [ -n "$hits" ]; then
    fail "a CWD-relative psql include — use \\ir so it resolves against the script:" "$hits"
fi

# ---------------------------------------------------------------------------
# 8. A documented `cd examples/<dir>` must land somewhere.
#
# #1054: examples/README.md walks a newcomer through six directories that do not exist,
# plus a seventh for the Arrow client. The first command of the first walkthrough fails.
# ---------------------------------------------------------------------------
echo "→ every documented example directory exists"
while IFS= read -r hit; do
    dir="$(printf '%s' "$hit" | sed -E 's|.*cd[[:space:]]+(examples/[A-Za-z0-9_./-]+).*|\1|')"
    case "$dir" in *'example-name'*|*'<'*) continue ;; esac
    [ -d "$dir" ] || fail "documented directory does not exist: $dir" "$hit"
done < <(grep -rn '^[[:space:]]*cd examples/' --include='*.md' examples/ README.md docs/ 2>/dev/null || true)

if [ "$found" -ne 0 ]; then
    echo
    echo "examples/ integrity gate FAILED — see above."
    exit 1
fi
echo "OK: shipped examples pass the static integrity gate."
