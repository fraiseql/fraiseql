#!/usr/bin/env bash
# Generate the throwaway CA + server certificate the local TLS Postgres uses.
#
# Mirrors `tlsCerts()` in .dagger/main.go so `make db-up` gives the same rig the
# `tls` integration suite gets in CI: a self-signed CA that is deliberately NOT in
# any platform trust store, so `verify-full` fails without the CA and succeeds with
# it. That contrast is the whole point — a TLS test that only proves "it connected"
# would have passed just as happily against the hard-coded `NoTls` this replaces
# (#801).
#
# Certificates are written to this directory and are gitignored. They are test
# fixtures with a 365-day life and no value outside a loopback container.
set -euo pipefail

out="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/certs"
mkdir -p "$out"

# Regenerate only when missing or expired, so `make db-up` stays fast.
if [ -f "$out/server.crt" ] && openssl x509 -checkend 86400 -noout -in "$out/server.crt" >/dev/null 2>&1; then
    exit 0
fi

echo "  Generating TLS test certificates in $out"
rm -f "$out"/*.crt "$out"/*.key "$out"/*.csr "$out"/*.srl

openssl req -x509 -newkey rsa:2048 -keyout "$out/ca.key" -out "$out/ca.crt" -days 365 -nodes \
    -subj '/CN=fraiseql-test-ca' \
    -addext 'basicConstraints=critical,CA:TRUE' \
    -addext 'keyUsage=critical,keyCertSign,cRLSign' 2>/dev/null

openssl req -newkey rsa:2048 -keyout "$out/server.key" -out "$out/server.csr" -days 365 -nodes \
    -subj '/CN=localhost' 2>/dev/null

openssl x509 -req -in "$out/server.csr" -CA "$out/ca.crt" -CAkey "$out/ca.key" \
    -CAcreateserial -out "$out/server.crt" -days 365 \
    -extfile <(printf 'subjectAltName=DNS:localhost,DNS:postgres-tls-test,IP:127.0.0.1\nbasicConstraints=CA:FALSE') \
    2>/dev/null

# The container copies these in and re-chmods; world-readable here so the
# unprivileged postgres user inside can read the bind mount.
chmod 644 "$out"/ca.crt "$out"/server.crt "$out"/server.key
rm -f "$out/server.csr"
