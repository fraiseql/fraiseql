#!/bin/bash
# Check DNS propagation for fraiseql.dev

echo "🔍 Checking DNS for fraiseql.dev..."
echo "===================================="

# Check A record
echo -n "A record:    "
dig +short fraiseql.dev A

# Check AAAA record
echo -n "AAAA record: "
dig +short fraiseql.dev AAAA

# Check www
echo -n "www A:       "
dig +short www.fraiseql.dev A

# Check from different DNS servers
echo ""
echo "Checking from different DNS servers:"
echo -n "Google DNS:  "
dig +short @8.8.8.8 fraiseql.dev A

echo -n "Quad9 DNS:   "
dig +short @9.9.9.9 fraiseql.dev A

# Check if pointing to our server
if dig +short fraiseql.dev A | grep -q "82.66.42.150"; then
    echo ""
    echo "✅ DNS is correctly pointing to RNSWEB01p!"
else
    echo ""
    echo "⏳ DNS not propagated yet. This can take 5-30 minutes."
fi
