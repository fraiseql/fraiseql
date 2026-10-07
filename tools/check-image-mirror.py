#!/usr/bin/env python3
"""Every base image the Dagger legs pull is mirrored, from a pinned source.

The legs pull `ghcr.io/fraiseql/*` only; `.github/workflows/mirror-base-images.yml`
copies each one there from upstream. Two rules held only in prose:

  A. every `ghcr.io/fraiseql/<name>:<tag>` the Dagger module names is a
     destination of that mirror (`.dagger/main.go` says so in a comment), so a
     deleted or expired package can be restored from upstream;
  B. no source and no destination rides `:latest`, and a source is a version
     tag or a `@sha256:` digest.

B is #1453. `docker.io/minio/minio:latest` stopped resolving when MinIO stopped
publishing community images, and the weekly mirror was red on every run from
2026-09-14 on. Nothing else noticed, because the legs pulled the last copy left
in ghcr. A floating source makes "which bytes are we testing against" a function
of the week, and when the upstream goes away there is no record of what the
copy was.

Exit codes: 0 = clean, 1 = findings, 2 = FATAL (an input the gate cannot read,
or a scan that found no image at all).

Overrides, for testing:
  IMAGE_MIRROR_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
GHCR_RE = re.compile(r'"(ghcr\.io/fraiseql/[^"\s]+)"')
IMAGES_RE = re.compile(r'IMAGES="\n(.*?)\n\s*"', re.DOTALL)
DIGEST_RE = re.compile(r"@sha256:[0-9a-f]{64}$")
TAG_RE = re.compile(r":([A-Za-z0-9_][A-Za-z0-9_.-]{0,127})$")


def die(message: str) -> None:
    print(f"FATAL: {message}", file=sys.stderr)
    raise SystemExit(2)


def root() -> Path:
    env = os.environ.get("IMAGE_MIRROR_ROOT")
    return Path(env) if env else REPO


def pinned(ref: str) -> str | None:
    """Why `ref` is not pinned, or None when it is."""
    if DIGEST_RE.search(ref):
        return None
    tag = TAG_RE.search(ref.rsplit("/", 1)[-1])
    if tag is None:
        return "has no tag, so it means `:latest`"
    if tag.group(1) == "latest":
        return "rides `:latest`"
    return None


def main() -> int:
    base = root()
    workflow = base / ".github" / "workflows" / "mirror-base-images.yml"
    if not workflow.is_file():
        die(f"no mirror workflow at {workflow}")
    block = IMAGES_RE.search(workflow.read_text())
    if block is None:
        die(f"{workflow.name}: no IMAGES=\"…\" block")

    mirror: dict[str, str] = {}
    for line in block.group(1).splitlines():
        line = line.strip()
        if not line:
            continue
        if line.count("|") != 1:
            die(f"{workflow.name}: unreadable mirror row {line!r}")
        src, dst = line.split("|")
        mirror[dst] = src

    used: dict[str, str] = {}
    for go in sorted((base / ".dagger").glob("*.go")):
        for n, text in enumerate(go.read_text().splitlines(), 1):
            if text.lstrip().startswith("//"):
                continue
            for ref in GHCR_RE.findall(text):
                used.setdefault(ref, f".dagger/{go.name}:{n}")
    if not used or not mirror:
        die("found no ghcr.io/fraiseql image in .dagger/ or no mirror rows")

    findings: list[str] = []
    for ref, where in sorted(used.items()):
        if ref not in mirror:
            findings.append(f"{where}: {ref} is not a destination of {workflow.name}")
    for dst, src in sorted(mirror.items()):
        for side, ref in (("source", src), ("destination", dst)):
            why = pinned(ref)
            if why:
                findings.append(f"{workflow.name}: mirror {side} {ref} {why}")

    if findings:
        for line in findings:
            print(f"FAIL: {line}")
        return 1
    print(f"image mirror: OK ({len(used)} Dagger images, {len(mirror)} pinned mirror rows)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
