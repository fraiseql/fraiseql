#!/usr/bin/env python3
"""Gate: every secret a publish job reads is checked before the release starts.

`release.yml` opens with `Validate Release Prerequisites`, whose `Validate required
secrets` step refuses to start a release when a token it needs is missing. Nothing tied
that list to the secrets the publish jobs actually read, and they drifted: the Rust SDK
publish (in `release.yml` and in `rust-sdk.yml`) read `secrets.CARGO_REGISTRY_TOKEN`, a
secret the repository never had, while the prerequisite step checked `CARGO_TOKEN`. The
v2.16.0 release passed every gate and failed at the upload (#1518):

    error: failed to publish fraiseql-rust v2.16.0 to registry at https://crates.io
    Caused by: please provide a non-empty token

A secret that does not exist reads as an empty string in GitHub Actions, so nothing fails
until the registry refuses the upload, after the other packages are already public.

Rule: in every workflow, each job that publishes to a package registry (`cargo publish`,
`uv publish`, `twine upload`, `npm publish`, `pypa/gh-action-pypi-publish`) may read only
secrets that `release.yml`'s `Validate required secrets` step checks, or `GITHUB_TOKEN`
(provided by the platform). A publisher that authenticates by OIDC reads none and passes.

Pure text parsing, so it runs in the toolchain-free ShellGates container. argv[1]
overrides the repository root (the self-test points it at fixtures).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

PUBLISH = re.compile(
    r"\bcargo publish\b|\buv publish\b|\btwine upload\b|\bnpm publish\b"
    r"|pypa/gh-action-pypi-publish"
)
# `secrets.NAME`, but not a step id ending in `secrets` (`steps.publish-secrets.outcome`).
SECRET = re.compile(r"(?<![\w.-])secrets\.([A-Za-z_][A-Za-z0-9_]*)")
PLATFORM = {"GITHUB_TOKEN"}
VALIDATE_STEP = "Validate required secrets"


def repo_root() -> Path:
    if len(sys.argv) > 1:
        return Path(sys.argv[1]).resolve()
    return Path(__file__).resolve().parent.parent


def uncommented(text: str) -> str:
    """Drop whole-line YAML comments, so prose naming a secret is not a reference."""
    return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("#"))


def jobs(text: str) -> list[tuple[str, str]]:
    """`(job id, job body)` for each job under the top-level `jobs:` key."""
    match = re.search(r"^jobs:\s*$", text, re.M)
    if not match:
        return []
    body = text[match.end():]
    end = re.search(r"^\S", body, re.M)
    if end:
        body = body[: end.start()]
    heads = list(re.finditer(r"^  ([A-Za-z0-9_-]+):\s*$", body, re.M))
    return [
        (head.group(1), body[head.end(): heads[i + 1].start() if i + 1 < len(heads) else None])
        for i, head in enumerate(heads)
    ]


def validated_secrets(release: str) -> set[str]:
    """The secrets the prerequisite step checks."""
    match = re.search(
        rf"^(\s*)- name:\s*{re.escape(VALIDATE_STEP)}\s*$", release, re.M
    )
    if not match:
        return set()
    indent = match.group(1)
    rest = release[match.end():]
    end = re.search(rf"^{re.escape(indent)}- ", rest, re.M)
    return set(SECRET.findall(rest[: end.start()] if end else rest))


def main() -> int:
    root = repo_root()
    workflows = root / ".github" / "workflows"
    release = workflows / "release.yml"
    if not release.is_file():
        print(f"FAIL: {release} not found", file=sys.stderr)
        return 1
    allowed = validated_secrets(uncommented(release.read_text(encoding="utf-8")))
    if not allowed:
        print(
            f"FAIL: release.yml has no `{VALIDATE_STEP}` step naming a secret; the "
            "publish jobs' secrets cannot be checked against it",
            file=sys.stderr,
        )
        return 1

    failures: list[str] = []
    publish_jobs = 0
    for path in sorted(workflows.glob("*.y*ml")):
        text = uncommented(path.read_text(encoding="utf-8"))
        for job, body in jobs(text):
            if not PUBLISH.search(body):
                continue
            publish_jobs += 1
            for secret in sorted(set(SECRET.findall(body)) - allowed - PLATFORM):
                failures.append(
                    f"{path.name}: job `{job}` publishes with `secrets.{secret}`, which "
                    f"`{VALIDATE_STEP}` in release.yml does not check (it checks: "
                    f"{', '.join(sorted(allowed))}). Use a checked secret, or check this one."
                )

    if publish_jobs == 0:
        print("FAIL: no publish job found; the gate's patterns no longer match", file=sys.stderr)
        return 1
    for failure in failures:
        print(f"FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    print(f"publish secrets: {publish_jobs} publish jobs read only checked secrets")
    return 0


if __name__ == "__main__":
    sys.exit(main())
