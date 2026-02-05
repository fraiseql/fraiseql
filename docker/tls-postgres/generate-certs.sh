#!/usr/bin/env bash
#
# Generate self-signed certificates for PostgreSQL TLS testing
#
# Usage: ./generate-certs.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CERT_DIR="$SCRIPT_DIR/certs"

echo "Generating TLS certificates in $CERT_DIR..."

mkdir -p "$CERT_DIR"
cd "$CERT_DIR"

# Generate CA key and certificate
echo "Generating CA certificate..."
openssl genrsa -out ca.key 4096 2>/dev/null
openssl req -new -x509 -days 365 -key ca.key -out ca.crt \
    -subj "/CN=FraiseQL Test CA/O=FraiseQL/C=US" 2>/dev/null

# Generate server key and CSR
echo "Generating server certificate..."
openssl genrsa -out server.key 2048 2>/dev/null
openssl req -new -key server.key -out server.csr \
    -subj "/CN=localhost/O=FraiseQL/C=US" 2>/dev/null

# Create extensions file for SAN (Subject Alternative Names)
cat > server.ext << EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
DNS.2 = postgres-tls
DNS.3 = fraiseql-postgres-tls
IP.1 = 127.0.0.1
IP.2 = ::1
EOF

# Sign server certificate with CA
openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
    -out server.crt -days 365 -extfile server.ext 2>/dev/null

# Clean up temporary files
rm -f server.csr server.ext ca.srl

# Set permissions (PostgreSQL requires key to be readable only by owner)
chmod 600 server.key
chmod 644 server.crt ca.crt ca.key

echo ""
echo "Certificates generated successfully:"
ls -la "$CERT_DIR"
echo ""
echo "Files:"
echo "  CA certificate:     $CERT_DIR/ca.crt"
echo "  Server certificate: $CERT_DIR/server.crt"
echo "  Server key:         $CERT_DIR/server.key"
