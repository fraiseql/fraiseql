# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 7
# Basic operators
"eq", "neq", "in", "notin", "nin"

# Subnet/range operations
("inSubnet",)  # IP is in CIDR subnet (<<= operator)
("inRange",)  # IP is in range (>= and <=)

# Classification (RFC-based)
"isPrivate"  # RFC 1918 private addresses
"isPublic"  # Non-private addresses
"isIPv4"  # IPv4-specific (family() = 4)
"isIPv6"  # IPv6-specific (family() = 6)

# Enhanced classification (v0.6.1+)
"isLoopback"  # 127.0.0.0/8, ::1
"isLinkLocal"  # 169.254.0.0/16, fe80::/10
"isMulticast"  # 224.0.0.0/4, ff00::/8
"isDocumentation"  # RFC 3849/5737
"isCarrierGrade"  # RFC 6598 (100.64.0.0/10)
