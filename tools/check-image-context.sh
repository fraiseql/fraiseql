#!/usr/bin/env bash
# check-image-context.sh — every path the Dockerfile COPYs survives the image
# functions' `+ignore` filter.
#
# Background (#1215): Dagger keys `DockerBuild` on the whole context digest, so
# with `+ignore=["target", "**/target", ".git"]` any change to any other file
# rebuilt every layer — a docs-only edit cost a measured 236s. Narrowing the
# filter to what the Dockerfile actually needs fixes that, and introduces the
# hazard this gate exists for: the ignore list becomes a SECOND, invisible copy
# of "what the build needs". Add a `COPY` for a path the filter drops and the
# build uses a context missing it — a leg asserting against something the tag
# path does not build.
#
# The invariant: for every context path the Dockerfile COPYs, the filter must
# admit it. Checked in the direction that matters — a COPY with no matching
# re-inclusion is a failure. The reverse (a re-inclusion with no COPY) is merely
# wasteful and is reported, not fatal.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

IMAGE_GO="${IMAGE_CONTEXT_GO:-.dagger/image.go}"
[ -f "$IMAGE_GO" ] || { echo "✗ $IMAGE_GO does not exist; this gate would check nothing."; exit 1; }

# Every Dockerfile the image functions build, discovered from the variant table
# rather than assumed. Assuming the root Dockerfile would have missed
# `tutorial/Dockerfile`, whose four COPY sources a filter narrowed for the root
# one drops — the exact stale-context failure this gate exists to prevent.
mapfile -t DOCKERFILES < <(
    grep -oE 'dockerfile:[[:space:]]*"[^"]+"' "$IMAGE_GO" \
    | sed -E 's/.*"([^"]+)"/\1/' | sort -u
)

if [ "${#DOCKERFILES[@]}" -eq 0 ]; then
    echo "✗ parsed no dockerfile: entries out of $IMAGE_GO — the parse is wrong, not the file."
    exit 1
fi

for f in "${DOCKERFILES[@]}"; do
    [ -f "$f" ] || { echo "✗ $IMAGE_GO builds $f, which does not exist."; exit 1; }
done

# ── the Dockerfile's context requirements ───────────────────────────────────
# `COPY --from=<stage>` reads from a previous stage, not the build context, so it
# imposes no requirement here. Everything else does: all args but the last are
# sources, and only the first path component matters to a directory-level filter.
mapfile -t copy_sources < <(
    for df in "${DOCKERFILES[@]}"; do
        grep -E '^[[:space:]]*(COPY|ADD)[[:space:]]' "$df" \
        | grep -vE '(COPY|ADD)[[:space:]]+--from=' \
        | sed -E 's/^[[:space:]]*(COPY|ADD)[[:space:]]+//' \
        | sed -E 's/--[a-z-]+=[^[:space:]]+[[:space:]]+//g' \
        | awk '{ for (i = 1; i < NF; i++) print $i }' \
        | sed -E 's#^\./##; s#/.*##' \
        | grep -vE '^\.$'
    done | sort -u
)

# The Dockerfile itself has to be IN the context: DockerBuild reads it from there,
# not from the host. A filter can admit every COPY source and still drop the file
# doing the copying — which is how the first narrowing here failed, with an error
# that says only "failed to build".
for df in "${DOCKERFILES[@]}"; do
    copy_sources+=("$(printf '%s' "$df" | sed -E 's#^\./##; s#/.*##')")
done
mapfile -t copy_sources < <(printf '%s\n' "${copy_sources[@]}" | sort -u)

if [ "${#copy_sources[@]}" -eq 0 ]; then
    echo "✗ parsed no COPY sources out of ${DOCKERFILES[*]} — the parse is wrong, not the files."
    exit 1
fi

# ── the filter ──────────────────────────────────────────────────────────────
mapfile -t ignore_lines < <(grep -n '+ignore=' "$IMAGE_GO" || true)
if [ "${#ignore_lines[@]}" -eq 0 ]; then
    echo "✗ found no +ignore= in $IMAGE_GO — the parse is wrong, or the filter is gone."
    exit 1
fi

echo "→ ${#DOCKERFILES[@]} Dockerfile(s) (${DOCKERFILES[*]}) need: ${copy_sources[*]}"
echo "→ $IMAGE_GO declares ${#ignore_lines[@]} +ignore filter(s)"

rc=0
for entry in "${ignore_lines[@]}"; do
    lineno="${entry%%:*}"
    list="${entry#*+ignore=}"

    # An excluding filter has a catch-all (`**` or `*`). Without one, everything
    # is included by default and only the explicit excludes below can drop a path.
    has_catchall=0
    case "$list" in
        *'"**"'*|*'"*"'*) has_catchall=1 ;;
    esac

    for src in "${copy_sources[@]}"; do
        if [ "$has_catchall" -eq 1 ]; then
            # Must be re-included explicitly: "!src" or "!src/..." .
            if ! printf '%s' "$list" | grep -qE "\"!${src}(/[^\"]*)?\""; then
                echo "✗ $IMAGE_GO:$lineno drops \`$src\`, which a Dockerfile COPYs"
                echo "    The filter has a catch-all, so \`$src\` needs \"!${src}/**\" (or \"!${src}\")."
                rc=1
            fi
        else
            # No catch-all: fail if something excludes the path outright.
            if printf '%s' "$list" | grep -qE "\"${src}(/[^\"]*)?\""; then
                echo "✗ $IMAGE_GO:$lineno excludes \`$src\`, which a Dockerfile COPYs"
                rc=1
            fi
        fi
    done
done

if [ "$rc" -ne 0 ]; then
    echo
    echo "  The +ignore list is a second copy of what the build needs. When they"
    echo "  disagree, the build runs against a context missing a path it COPYs."
    exit 1
fi

echo "OK: every path the ${#DOCKERFILES[@]} Dockerfile(s) COPY survives all ${#ignore_lines[@]} +ignore filter(s)."
