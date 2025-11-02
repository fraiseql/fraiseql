# Extracted from: docs/reference/quick-reference.md
# Block number: 10
# Basic
"eq", "neq", "in", "notin", "nin"

# Network operations
("inSubnet",)  # IP is in CIDR subnet
("inRange",)  # IP in range {"from": "...", "to": "..."}
("isPrivate",)  # RFC 1918 private
("isPublic",)  # Non-private
("isIPv4",)  # IPv4 only
("isIPv6",)  # IPv6 only

# Classification (RFC-based)
("isLoopback",)  # 127.0.0.0/8, ::1
("isLinkLocal",)  # 169.254.0.0/16, fe80::/10
("isMulticast",)  # 224.0.0.0/4, ff00::/8
("isDocumentation",)  # RFC 3849/5737
("isCarrierGrade",)  # RFC 6598 (100.64.0.0/10)
