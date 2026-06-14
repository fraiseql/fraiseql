#!/usr/bin/env bash
#
# Phase 12 — M-dual-crypto gate.
#
# Asserts the *default-feature* fraiseql-server build (what ships) links exactly
# one rustls crypto provider and one rustls major. The workspace standardised on
# `ring`; aws-lc-rs must not appear in a default build.
#
# Why a bespoke gate: cargo-deny already bans multiple *versions* of a single
# crate across all features, but it cannot express "one crypto provider in the
# default build" — `ring` and `aws-lc-rs` are distinct crates, so deny is blind to
# the fact that both being compiled means two providers linked into one rustls.
#
# Out of scope (intentional, opt-in — NOT default builds, so this gate ignores
# them): the `metrics` feature pulls metrics-exporter-prometheus -> hyper-rustls
# (aws-lc-rs by default), the `aws-s3` feature pulls the legacy aws rustls 0.21
# stack (tracked separately in deny.toml, deadline 2026-09-01), and dev-deps pull
# aws-lc-rs via metrics-exporter-prometheus.
set -euo pipefail

# Normal (non-dev, non-build) dependency closure of the server binary at default
# features, flattened to one "name vX.Y.Z" per line.
tree="$(cargo tree -p fraiseql-server -e normal --prefix none 2>/dev/null)"
if [ -z "$tree" ]; then
  echo "FAIL: 'cargo tree -p fraiseql-server' produced no output (dependency resolution error?)." >&2
  exit 1
fi

providers="$(printf '%s\n' "$tree" | grep -oE '^(ring|aws-lc-rs) v[0-9][0-9.]*' | awk '{print $1}' | sort -u)"
rustls_majors="$(printf '%s\n' "$tree" | grep -oE '^rustls v[0-9]+\.[0-9]+' | sort -u)"

rc=0

# Exactly one provider, and it must be ring. This single check rejects aws-lc-rs,
# a second provider, and the degenerate "no provider at all" case.
if [ "$providers" != "ring" ]; then
  printed="$(printf '%s' "$providers" | tr '\n' ',' | sed 's/,$//; s/,/, /g')"
  echo "FAIL (M-dual-crypto): default fraiseql-server build crypto providers = '${printed:-<none>}' (expected exactly 'ring')."
  rc=1
fi

major_count="$(printf '%s\n' "$rustls_majors" | grep -c . || true)"
if [ "$major_count" -gt 1 ]; then
  echo "FAIL (M-dual-crypto): default fraiseql-server build links more than one rustls major:"
  printf '    %s\n' $rustls_majors
  rc=1
fi

if [ "$rc" -ne 0 ]; then
  exit 1
fi

echo "OK: default fraiseql-server build links one crypto provider (${providers}) and one rustls major (${rustls_majors})."
