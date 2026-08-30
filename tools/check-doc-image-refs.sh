#!/usr/bin/env bash
# check-doc-image-refs.sh — a FraiseQL image named in documentation must be pullable.
#
# Background (#1220, #1129): `deploy/deployment-security-guide.md` showed a Compose stack
# pinning `image: fraiseql:1.8.0-hardened`. A **bare** name resolves to
# `docker.io/library/fraiseql` — Docker Hub's official-images namespace, which this
# project does not and cannot publish to — so the snippet in the document a reader
# consults specifically to get a deployment right named an image that cannot exist. #1129
# was the same defect in a real manifest; this is it in a fenced code block, where the
# file-level gates do not look: `check-deploy-versions.sh` and `check-deploy-security.sh`
# scan files, and `check-image-parity.py` compares the workflow matrices to the Dagger
# table. A code block is outside all three.
#
# The rule is deliberately narrow, because it is the half that is decidable without a
# network call: an image reference whose repository mentions `fraiseql` must carry a
# registry or a namespace. `ghcr.io/fraiseql/server:2.15.0` and `fraiseql/server:2.15.0`
# pass. `fraiseql:2.15.0` and `fraiseql-server:2.15.0` do not — Docker resolves both to
# `docker.io/library/…`.
#
# Only `image:` (Compose, Kubernetes) and `--image=` (kubectl, gcloud) are read. A
# `docker build -t fraiseql-server:local .` is a local tag, not a reference anyone pulls,
# and flagging it would push writers toward examples that are less clear.
#
# Pure bash, no toolchain, no git history → Dagger ShellGates.
#
# Overrides, for testing:
#   DOC_IMAGE_REFS_ROOT=<dir>   scan this directory instead of the repository root
set -uo pipefail

if [ -n "${DOC_IMAGE_REFS_ROOT:-}" ]; then
  cd "$DOC_IMAGE_REFS_ROOT" || exit 1
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root" || exit 1
fi

status=0
checked=0
flagged=0

shopt -s globstar nullglob
files=(deploy/**/*.md docs/**/*.md README.md)
shopt -u globstar nullglob

for file in "${files[@]}"; do
  [ -f "$file" ] || continue
  lineno=0
  while IFS= read -r line; do
    lineno=$((lineno + 1))
    # `image: X`, `"image": "X"`, `--image=X`
    # Two spellings, because there are two. `image:` / `"image":` is Compose and
    # Kubernetes; `--image=` is kubectl and gcloud. They need separate patterns: the
    # first must NOT match `my-image:`, so it excludes a preceding `-`, which is exactly
    # the character the second one starts with. Written as one pattern, the `--image=`
    # form matched nothing and the gate read 1 reference where the tree has 2.
    ref="$(printf '%s' "$line" \
      | sed -nE 's/.*(^|[^A-Za-z_-])"?image"?:[[:space:]]*"?([^"[:space:],]+).*/\2/p' \
      | head -1)"
    if [ -z "$ref" ]; then
      ref="$(printf '%s' "$line" \
        | sed -nE 's/.*--image=[[:space:]]*"?([^"[:space:],\\]+).*/\1/p' \
        | head -1)"
    fi
    [ -z "$ref" ] && continue
    case "$ref" in
      *'$'*|*'{{'*|*'<'*) continue ;;
    esac
    case "$ref" in
      *fraiseql*) ;;
      *) continue ;;
    esac
    checked=$((checked + 1))
    repo="${ref%%:*}"
    case "$repo" in
      */*) ;;  # has a namespace or a registry — pullable
      *)
        echo "ERROR: ${file}:${lineno} names a bare FraiseQL image: ${ref}"
        echo "       Docker resolves that to docker.io/library/${repo}, the official-images"
        echo "       namespace, which this project cannot publish to. Use"
        echo "       ghcr.io/fraiseql/server:<version> or fraiseql/server:<version>."
        flagged=$((flagged + 1))
        status=1
        ;;
    esac
  done < "$file"
done

if [ "$checked" -eq 0 ]; then
  # The docs always name at least one FraiseQL image. Matching none means the extraction
  # stopped working, and a gate that inspects nothing reports OK forever.
  echo "ERROR: no FraiseQL image reference found in any deploy/ or docs/ Markdown file." >&2
  echo "       The extraction stopped matching and this check went blind." >&2
  exit 1
fi

if [ "$status" -eq 0 ]; then
  echo "OK: all ${checked} FraiseQL image reference(s) in documentation are namespaced."
fi
exit "$status"
