# Extracted from: docs/core/concepts-glossary.md
# Block number: 10
# Your type definition
@fraiseql.type(sql_source="v_server")
class Server:
    id: UUID
    hostname: str
    ip_address: NetworkAddress  # Special type
    port: int
    location: Coordinate  # Special type


# FraiseQL auto-generates:
class ServerWhereInput:
    id: UUIDFilter | None
    hostname: StringFilter | None
    ip_address: NetworkAddressFilter | None  # Rich operators!
    port: IntFilter | None
    location: CoordinateFilter | None  # Distance queries!
    AND: list[ServerWhereInput] | None
    OR: list[ServerWhereInput] | None
    NOT: ServerWhereInput | None
