#!/usr/bin/env bash
# Runs inside the probe image: start the mock Flight server, then the positive
# probe and the negative one. Both must pass.
set -euo pipefail
PORT="${PROBE_PORT:-15051}"
export PROBE_PORT="$PORT"

python3 /probe/mock_flight_server.py "$PORT" > /tmp/server.log 2>&1 &
server=$!

for _ in $(seq 1 40); do
    python3 -c "
import socket, sys
s = socket.socket()
s.settimeout(0.5)
sys.exit(0 if s.connect_ex(('127.0.0.1', $PORT)) == 0 else 1)
" && break
    sleep 0.5
done

if ! kill -0 "$server" 2>/dev/null; then
    echo "✗ the mock Flight server exited before the probe ran:"
    sed 's/^/    /' /tmp/server.log
    exit 1
fi

echo "=== probe: the client completes the exchange ==="
Rscript /probe/probe.R
echo
echo "=== red check: a wrong session token is refused ==="
Rscript /probe/red_check.R
echo
echo "--- server log ---"
sed 's/^/    /' /tmp/server.log
kill "$server" 2>/dev/null || true
echo
echo "OK: examples/r/fraiseql_client.R completes the handshake, sends the header, and decodes the result."
