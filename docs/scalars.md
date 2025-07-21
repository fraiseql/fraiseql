# FraiseQL Scalar Types

FraiseQL provides a comprehensive set of scalar types for common data validation needs. These scalars ensure type safety at the GraphQL layer, providing clear error messages when validation fails.

## Built-in Scalars

### Date and Time Scalars

#### DateTime
- **Description**: ISO 8601 datetime with timezone awareness
- **Example**: `2024-01-15T10:30:00Z`, `2024-01-15T10:30:00+02:00`
- **Python Type**: `datetime.datetime`

#### Date
- **Description**: ISO 8601 calendar date
- **Example**: `2024-01-15`
- **Python Type**: `datetime.date`

#### DateRange
- **Description**: PostgreSQL daterange values
- **Example**: `[2024-01-01,2024-12-31)`
- **Python Type**: String representation of date range

### Network Scalars

#### IpAddress
- **Description**: IPv4 and IPv6 addresses
- **Examples**: `192.168.1.1`, `2001:db8::1`
- **Validation**: Uses Python's `ipaddress` module

#### Port
- **Description**: Network port number
- **Range**: 1-65535 (port 0 is reserved)
- **Examples**: `80`, `443`, `8080`

#### CIDR
- **Description**: CIDR notation for IP network ranges
- **Examples**: `192.168.1.0/24`, `10.0.0.0/8`, `2001:db8::/32`
- **Validation**: Supports both IPv4 and IPv6 CIDR notation

#### Hostname
- **Description**: DNS hostname (RFC 1123 compliant)
- **Rules**:
  - Total length: 1-253 characters
  - Each label: 1-63 characters
  - Valid characters: a-z, A-Z, 0-9, hyphen (-), dot (.)
  - Cannot start/end with hyphens
  - Case-insensitive (normalized to lowercase)
- **Examples**: `example.com`, `my-server.local`

#### MacAddress
- **Description**: Hardware MAC address
- **Accepted Formats**:
  - Colon-separated: `00:11:22:33:44:55`
  - Hyphen-separated: `00-11-22-33-44-55`
  - Dot-separated (Cisco): `0011.2233.4455`
  - No separators: `001122334455`
- **Normalization**: All formats normalized to uppercase colon-separated

#### SubnetMask
- **Description**: Subnet mask for IPv4 networks
- **Examples**: `255.255.255.0`, `255.255.0.0`

### Communication Scalars

#### EmailAddress
- **Description**: Validated email address
- **Validation**: Basic pattern matching `^[^@]+@[^@]+\.[^@]+$`
- **Examples**: `user@example.com`, `admin@company.org`

### Data Structure Scalars

#### JSON
- **Description**: Arbitrary JSON-serializable values
- **Examples**: `{"key": "value"}`, `[1, 2, 3]`, `"string"`, `123`
- **Python Type**: `dict`, `list`, or any JSON-serializable type

#### UUID
- **Description**: RFC 4122 UUID values
- **Format**: Mapped to GraphQL ID type
- **Examples**: `550e8400-e29b-41d4-a716-446655440000`
- **Python Type**: `uuid.UUID`

#### LTree
- **Description**: PostgreSQL ltree path type
- **Examples**: `Top.Science.Astronomy`, `Top.Hobbies.Amateurs_Astronomy`
- **Use Case**: Hierarchical data structures

## Usage Examples

### Basic Usage

```python
from fraiseql.types import Port, IpAddress, EmailAddress, UUID
from datetime import datetime
import fraiseql

@fraiseql.type
class Server:
    id: UUID
    hostname: str
    ip_address: IpAddress
    ssh_port: Port
    admin_email: EmailAddress
    created_at: datetime
```

### Network Device Management

```python
from fraiseql.types import Port, IpAddress, CIDR, Hostname, MacAddress
import fraiseql

@fraiseql.input
class NetworkDeviceInput:
    hostname: Hostname
    ip_address: IpAddress
    mac_address: MacAddress
    subnet: CIDR
    management_port: Port = 22
```

### Optional Fields with UNSET

```python
import fraiseql
from fraiseql.types import EmailAddress, IpAddress

@fraiseql.input
class UpdateUserInput:
    email: EmailAddress | None = fraiseql.UNSET
    backup_ip: IpAddress | None = fraiseql.UNSET
```

## Error Handling

Scalar validation provides clear GraphQL errors:

```json
{
  "errors": [{
    "message": "Port must be between 1 and 65535, got 0",
    "path": ["createServer", "input", "port"]
  }]
}
```

## Creating Custom Scalars

While FraiseQL doesn't currently support user-defined custom scalars via decorators, you can:

1. Request new scalars be added to FraiseQL core
2. Use field validation in your mutation handlers
3. Create wrapper types with validation logic

Example of mutation-level validation:

```python
from graphql import GraphQLError

@fraiseql.mutation
async def create_server(info, input: ServerInput) -> Server:
    # Custom validation
    if input.port == 22 and not input.ssh_enabled:
        raise GraphQLError("Port 22 requires SSH to be enabled")
    
    # Continue with mutation logic...
```

## Best Practices

1. **Use specific scalars**: Prefer `IpAddress` over generic `str` for IP addresses
2. **Provide defaults**: Use sensible defaults like `port: Port = 22` for SSH
3. **Combine scalars**: Use multiple scalars together for comprehensive validation
4. **Handle None vs UNSET**: Use `fraiseql.UNSET` for optional update fields
5. **Document formats**: Include examples in your schema documentation

## Type Mapping

| Python Type | GraphQL Scalar | Notes |
|------------|----------------|-------|
| `str` | `String` | |
| `int` | `Int` | |
| `float` | `Float` | |
| `bool` | `Boolean` | |
| `uuid.UUID` | `ID` | |
| `datetime.datetime` | `DateTime` | ISO 8601 with timezone |
| `datetime.date` | `Date` | ISO 8601 date |
| `dict` | `JSON` | Any JSON-serializable |
| `IpAddress` | `IpAddress` | IPv4/IPv6 validation |
| `Port` | `Port` | 1-65535 range |
| `EmailAddress` | `EmailAddress` | Email validation |
| And more... | | |