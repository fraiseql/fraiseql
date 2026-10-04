#!/usr/bin/env bash
#
# Crypto policy gate: M-dual-crypto, and the scope of RSA private-key use (#1110).
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

# RSA private-key operations stay confined to GCS service-account signing (#1110).
#
# RUSTSEC-2023-0071 (rsa, Marvin) is accepted because the `rsa` crate is reached only
# through jsonwebtoken's `rust_crypto` backend for PUBLIC-key verification — which the
# advisory does not affect — plus one private-key site: the opt-in `gcs` storage backend
# signing its own service-account assertion, which Google requires to be RS256 and which
# no caller can trigger or time. A new RSA private key anywhere else (an RS256 token
# signer, a decryptor) would make that acceptance false while every other gate stayed
# green, so it fails here. Test files are exempt: they sign tokens as an external issuer.
# Scanned with grep, not git: the container this leg runs in has no `.git`.
allowed_rsa_signers="crates/fraiseql-storage/src/backend/gcs.rs"
rsa_private="$(grep -rlE 'EncodingKey::from_rsa_(pem|der|raw_components)|RsaPrivateKey' \
  --include='*.rs' crates 2>/dev/null \
  | grep -vE '(^|/)tests/|(^|/)tests?\.rs$|_tests?\.rs$|/fuzz/|/benches/' \
  | grep -vxF "$allowed_rsa_signers" || true)"
if [ -n "$rsa_private" ]; then
  echo "FAIL (#1110): RSA private-key operations outside ${allowed_rsa_signers}:"
  printf '    %s\n' $rsa_private
  echo "    RUSTSEC-2023-0071 is accepted only while rsa does public-key work (plus GCS"
  echo "    signing). Sign with HS256/ES256/EdDSA instead, or re-argue the acceptance in"
  echo "    docs/dependency-risk-policy.md before allowing a new site."
  rc=1
fi

if [ "$rc" -ne 0 ]; then
  exit 1
fi

echo "OK: default fraiseql-server build links one crypto provider (${providers}) and one rustls major (${rustls_majors}); RSA private keys only in ${allowed_rsa_signers}."
