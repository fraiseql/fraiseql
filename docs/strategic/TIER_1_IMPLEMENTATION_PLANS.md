# Tier 1 Features - Detailed Implementation Plans

**Framework**: FraiseQL Enterprise Edition
**Methodology**: Phased TDD Approach
**Target**: Enterprise Compliance & Security Foundation

---

## ⚡ Simplification Notes

### Original Plan vs. Implementation

**Original Plan (Complex):**

- Separate `audit_events` table for crypto
- Separate `tenant.tb_audit_log` for CDC
- Python crypto modules for hashing/signing
- GraphQL interceptors in Python
- Bridge triggers to sync tables

**Actual Implementation (Simplified):**

- ✅ **Single unified `audit_events` table** (CDC + crypto together)
- ✅ **PostgreSQL handles all crypto** (triggers, not Python)
- ✅ **No GraphQL interceptors needed** (use existing `log_and_return_mutation()`)
- ✅ **No bridge triggers needed** (one table = no sync)
- ✅ **Philosophy aligned**: "In PostgreSQL Everything"

### Why Simplified?

1. **Performance**: No duplicate writes, no Python overhead
2. **Simplicity**: One table, one schema, one source of truth
3. **Maintainability**: Less code, fewer moving parts
4. **Philosophy**: PostgreSQL-native is faster and simpler

---

## Table of Contents

1. [Feature 1: Immutable Audit Logging with Cryptographic Integrity](#feature-1-immutable-audit-logging-with-cryptographic-integrity)
2. [Feature 2: Advanced RBAC (Role-Based Access Control)](#feature-2-advanced-rbac-role-based-access-control)
3. [Feature 3: GDPR Compliance Suite](#feature-3-gdpr-compliance-suite)
4. [Feature 4: Data Classification & Labeling](#feature-4-data-classification-labeling)

---

## Feature 1: Immutable Audit Logging with Cryptographic Integrity

**Complexity**: Complex | **Duration**: 5-7 weeks | **Priority**: 10/10

### Executive Summary

Implement a tamper-proof audit logging system that creates cryptographically-signed chains of events for SOX, HIPAA, and financial services compliance. Each audit event is hashed and linked to the previous event, creating an immutable chain similar to blockchain technology. The system integrates with FraiseQL's existing security infrastructure and provides APIs for compliance verification and reporting.

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                         │
│  (GraphQL Mutations, Queries, Authentication Events)         │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│              AuditLogger (Interceptor Layer)                 │
│  - Captures all mutations, queries, auth events              │
│  - Enriches with context (user, tenant, IP, timestamp)       │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│           Cryptographic Chain Builder                        │
│  - SHA-256 hashing of event data                            │
│  - Links to previous event hash                              │
│  - Signs with HMAC-SHA256                                    │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│        PostgreSQL Append-Only Audit Table                    │
│  - INSERT-only (no UPDATE/DELETE permissions)               │
│  - Row-level security policies                               │
│  - Partitioned by time for performance                       │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│          Verification & Compliance APIs                      │
│  - Chain integrity verification                              │
│  - Audit trail queries                                       │
│  - Compliance reports (SOX, HIPAA)                           │
└─────────────────────────────────────────────────────────────┘
```

### File Structure

```
src/fraiseql/enterprise/
├── audit/
│   ├── __init__.py
│   ├── chain_builder.py          # Cryptographic chain implementation
│   ├── event_logger.py            # Event capture and enrichment
│   ├── interceptors.py            # GraphQL/mutation interceptors
│   ├── verification.py            # Chain integrity verification
│   ├── types.py                   # GraphQL types for audit events
│   └── compliance_reports.py      # SOX/HIPAA report generation
├── crypto/
│   ├── __init__.py
│   ├── hashing.py                 # SHA-256 utilities
│   └── signing.py                 # HMAC-SHA256 signing
└── migrations/
    └── 001_audit_tables.sql       # Database schema

tests/integration/enterprise/audit/
├── test_chain_integrity.py
├── test_event_capture.py
├── test_verification_api.py
└── test_compliance_reports.py

docs/enterprise/
├── audit-logging.md
└── compliance-verification.md
```

---

## PHASES

### Phase 1: Database Schema & Core Data Model

**Objective**: Create append-only audit table with proper constraints and partitioning

#### TDD Cycle 1.1: Audit Event Table Schema

**RED**: Write failing test for audit table creation

```python
# tests/integration/enterprise/audit/test_audit_schema.py

async def test_audit_events_table_exists():
    """Verify audit_events table exists with correct schema."""
    result = await db.run(DatabaseQuery(
        statement="SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'audit_events'",
        params={},
        fetch_result=True
    ))

    required_columns = {
        'id': 'uuid',
        'event_type': 'character varying',
        'event_data': 'jsonb',
        'user_id': 'uuid',
        'tenant_id': 'uuid',
        'timestamp': 'timestamp with time zone',
        'ip_address': 'inet',
        'previous_hash': 'character varying',
        'event_hash': 'character varying',
        'signature': 'character varying'
    }

    assert len(result) >= len(required_columns)
    # Expected failure: table doesn't exist yet
```

**GREEN**: Implement minimal SQL migration

```sql
-- src/fraiseql/enterprise/migrations/001_audit_tables.sql

CREATE TABLE IF NOT EXISTS audit_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type VARCHAR(100) NOT NULL,
    event_data JSONB NOT NULL,
    user_id UUID,
    tenant_id UUID,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ip_address INET,
    previous_hash VARCHAR(64),
    event_hash VARCHAR(64) NOT NULL,
    signature VARCHAR(128) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Prevent updates and deletes
CREATE POLICY audit_events_insert_only ON audit_events
    FOR ALL
    USING (false)
    WITH CHECK (true);

-- Index for chain verification
CREATE INDEX idx_audit_events_hash ON audit_events(event_hash);
CREATE INDEX idx_audit_events_timestamp ON audit_events(timestamp DESC);
CREATE INDEX idx_audit_events_tenant ON audit_events(tenant_id, timestamp DESC);

-- Partition by month for performance
CREATE TABLE audit_events_y2025m01 PARTITION OF audit_events
    FOR VALUES FROM ('2025-01-01') TO ('2025-02-01');
```

**REFACTOR**: Add partitioning automation and constraints

```sql
-- Add function to auto-create partitions
CREATE OR REPLACE FUNCTION create_audit_partition()
RETURNS trigger AS $$
DECLARE
    partition_date DATE;
    partition_name TEXT;
    start_date DATE;
    end_date DATE;
BEGIN
    partition_date := DATE_TRUNC('month', NEW.timestamp);
    partition_name := 'audit_events_y' || TO_CHAR(partition_date, 'YYYY') || 'm' || TO_CHAR(partition_date, 'MM');
    start_date := partition_date;
    end_date := partition_date + INTERVAL '1 month';

    IF NOT EXISTS (
        SELECT 1 FROM pg_class WHERE relname = partition_name
    ) THEN
        EXECUTE FORMAT(
            'CREATE TABLE %I PARTITION OF audit_events FOR VALUES FROM (%L) TO (%L)',
            partition_name, start_date, end_date
        );
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER create_audit_partition_trigger
    BEFORE INSERT ON audit_events
    FOR EACH ROW EXECUTE FUNCTION create_audit_partition();
```

**QA**: Verify schema and run full test suite

```bash
uv run pytest tests/integration/enterprise/audit/test_audit_schema.py -v
uv run pytest tests/integration/enterprise/audit/ -v
```

**Success Criteria**:

- [ ] Audit table created with all required columns
- [ ] INSERT-only policy enforced (UPDATE/DELETE fail)
- [ ] Indexes created for performance
- [ ] Partitioning works automatically
- [ ] All tests pass

---

#### TDD Cycle 1.2: GraphQL Types for Audit Events

**RED**: Write failing test for GraphQL audit event type

```python
# tests/integration/enterprise/audit/test_audit_types.py

def test_audit_event_graphql_type():
    """Verify AuditEvent GraphQL type is properly defined."""
    schema = get_fraiseql_schema()

    audit_event_type = schema.type_map.get('AuditEvent')
    assert audit_event_type is not None

    fields = audit_event_type.fields
    assert 'id' in fields
    assert 'eventType' in fields
    assert 'eventData' in fields
    assert 'userId' in fields
    assert 'timestamp' in fields
    assert 'eventHash' in fields
    # Expected failure: AuditEvent type not defined yet
```

**GREEN**: Implement minimal GraphQL type

```python
# src/fraiseql/enterprise/audit/types.py

import strawberry
from datetime import datetime
from uuid import UUID
from typing import Optional

@strawberry.type
class AuditEvent:
    """Immutable audit log entry with cryptographic chain."""

    id: UUID
    event_type: str
    event_data: strawberry.scalars.JSON
    user_id: Optional[UUID]
    tenant_id: Optional[UUID]
    timestamp: datetime
    ip_address: Optional[str]
    previous_hash: Optional[str]
    event_hash: str
    signature: str

    @classmethod
    def from_db_row(cls, row: dict) -> "AuditEvent":
        """Create AuditEvent from database row."""
        return cls(
            id=row['id'],
            event_type=row['event_type'],
            event_data=row['event_data'],
            user_id=row.get('user_id'),
            tenant_id=row.get('tenant_id'),
            timestamp=row['timestamp'],
            ip_address=row.get('ip_address'),
            previous_hash=row.get('previous_hash'),
            event_hash=row['event_hash'],
            signature=row['signature']
        )
```

**REFACTOR**: Add input types and filters

```python
@strawberry.input
class AuditEventFilter:
    """Filter for querying audit events."""

    event_type: Optional[str] = None
    user_id: Optional[UUID] = None
    tenant_id: Optional[UUID] = None
    start_time: Optional[datetime] = None
    end_time: Optional[datetime] = None

@strawberry.type
class AuditEventConnection:
    """Paginated audit events with chain metadata."""

    events: list[AuditEvent]
    total_count: int
    chain_valid: bool  # Result of integrity verification
    has_more: bool
```

**QA**: Verify GraphQL schema and integration

```bash
uv run pytest tests/integration/enterprise/audit/test_audit_types.py -v
uv run pytest tests/integration/graphql/ -k audit -v
```

---

### Phase 2: Cryptographic Chain Implementation

**Objective**: Implement SHA-256 hashing and HMAC signing for tamper-proof chain

#### TDD Cycle 2.1: Event Hashing

**RED**: Write failing test for event hash generation

```python
# tests/integration/enterprise/audit/test_chain_builder.py

def test_event_hash_generation():
    """Verify event hash is deterministic and collision-resistant."""
    from fraiseql.enterprise.crypto.hashing import hash_audit_event

    event_data = {
        'event_type': 'user.login',
        'user_id': '123e4567-e89b-12d3-a456-426614174000',
        'timestamp': '2025-01-15T10:30:00Z',
        'ip_address': '192.168.1.100',
        'data': {'method': 'password'}
    }

    hash1 = hash_audit_event(event_data, previous_hash=None)
    hash2 = hash_audit_event(event_data, previous_hash=None)

    assert hash1 == hash2  # Deterministic
    assert len(hash1) == 64  # SHA-256 hex digest
    assert hash1 != hash_audit_event({**event_data, 'user_id': 'different'})
    # Expected failure: hash_audit_event not implemented
```

**GREEN**: Implement minimal hashing function

```python
# src/fraiseql/enterprise/crypto/hashing.py

import hashlib
import json
from typing import Any, Optional

def hash_audit_event(event_data: dict[str, Any], previous_hash: Optional[str]) -> str:
    """Generate SHA-256 hash of audit event linked to previous hash.

    Args:
        event_data: Event data to hash (must be JSON-serializable)
        previous_hash: Hash of previous event in chain (None for genesis event)

    Returns:
        64-character hex digest of SHA-256 hash
    """
    # Create canonical JSON representation (sorted keys for determinism)
    canonical_json = json.dumps(event_data, sort_keys=True, separators=(',', ':'))

    # Include previous hash in chain
    chain_data = f"{previous_hash or 'GENESIS'}:{canonical_json}"

    # Generate SHA-256 hash
    return hashlib.sha256(chain_data.encode('utf-8')).hexdigest()
```

**REFACTOR**: Add validation and edge case handling

```python
def hash_audit_event(
    event_data: dict[str, Any],
    previous_hash: Optional[str],
    hash_algorithm: str = 'sha256'
) -> str:
    """Generate cryptographic hash of audit event.

    Args:
        event_data: Event data (must be JSON-serializable)
        previous_hash: Previous event hash (None for first event)
        hash_algorithm: Hashing algorithm (default: sha256)

    Returns:
        Hex digest of event hash

    Raises:
        ValueError: If event_data is not JSON-serializable
    """
    if not event_data:
        raise ValueError("Event data cannot be empty")

    try:
        # Ensure deterministic ordering
        canonical_json = json.dumps(
            event_data,
            sort_keys=True,
            separators=(',', ':'),
            default=str  # Handle UUID, datetime, etc.
        )
    except (TypeError, ValueError) as e:
        raise ValueError(f"Event data must be JSON-serializable: {e}")

    # Create chain by including previous hash
    chain_data = f"{previous_hash or 'GENESIS'}:{canonical_json}"

    # Generate hash using specified algorithm
    hasher = hashlib.new(hash_algorithm)
    hasher.update(chain_data.encode('utf-8'))

    return hasher.hexdigest()
```

**QA**: Run comprehensive hash tests

```bash
uv run pytest tests/integration/enterprise/audit/test_chain_builder.py::test_event_hash_generation -v
uv run pytest tests/integration/enterprise/audit/test_chain_builder.py -v
```

---

#### TDD Cycle 2.2: HMAC Signature Generation

**RED**: Write failing test for event signing

```python
def test_event_signature():
    """Verify HMAC-SHA256 signature prevents tampering."""
    from fraiseql.enterprise.crypto.signing import sign_event

    event_hash = "abc123def456"
    secret_key = "test-secret-key-do-not-use-in-production"

    signature = sign_event(event_hash, secret_key)

    assert len(signature) > 0
    assert signature == sign_event(event_hash, secret_key)  # Deterministic
    assert signature != sign_event(event_hash, "different-key")
    # Expected failure: sign_event not implemented
```

**GREEN**: Implement HMAC signing

```python
# src/fraiseql/enterprise/crypto/signing.py

import hmac
import hashlib
import os

def sign_event(event_hash: str, secret_key: str) -> str:
    """Generate HMAC-SHA256 signature for event hash.

    Args:
        event_hash: SHA-256 hash of event
        secret_key: Secret signing key

    Returns:
        Hex digest of HMAC signature
    """
    return hmac.new(
        key=secret_key.encode('utf-8'),
        msg=event_hash.encode('utf-8'),
        digestmod=hashlib.sha256
    ).hexdigest()

def verify_signature(event_hash: str, signature: str, secret_key: str) -> bool:
    """Verify HMAC signature matches event hash.

    Args:
        event_hash: SHA-256 hash of event
        signature: Claimed HMAC signature
        secret_key: Secret signing key

    Returns:
        True if signature is valid
    """
    expected_signature = sign_event(event_hash, secret_key)
    return hmac.compare_digest(signature, expected_signature)
```

**REFACTOR**: Add key rotation and configuration

```python
# src/fraiseql/enterprise/crypto/signing.py

from typing import Optional
from datetime import datetime

class SigningKeyManager:
    """Manages signing keys with rotation support."""

    def __init__(self):
        self.current_key: Optional[str] = None
        self.previous_keys: list[tuple[str, datetime]] = []
        self._load_keys()

    def _load_keys(self):
        """Load signing keys from environment or key vault."""
        self.current_key = os.getenv('AUDIT_SIGNING_KEY')
        if not self.current_key:
            raise ValueError("AUDIT_SIGNING_KEY environment variable not set")

    def sign(self, event_hash: str) -> str:
        """Sign event hash with current key."""
        if not self.current_key:
            raise ValueError("No signing key available")
        return sign_event(event_hash, self.current_key)

    def verify(self, event_hash: str, signature: str) -> bool:
        """Verify signature with current or previous keys."""
        # Try current key first
        if self.current_key and verify_signature(event_hash, signature, self.current_key):
            return True

        # Try previous keys (for events signed before rotation)
        for key, rotated_at in self.previous_keys:
            if verify_signature(event_hash, signature, key):
                return True

        return False

# Singleton instance
_key_manager: Optional[SigningKeyManager] = None

def get_key_manager() -> SigningKeyManager:
    """Get or create signing key manager singleton."""
    global _key_manager
    if _key_manager is None:
        _key_manager = SigningKeyManager()
    return _key_manager
```

**QA**: Test signature verification and key rotation

```bash
uv run pytest tests/integration/enterprise/audit/test_signing.py -v
```

---

### Phase 3: Event Capture & Logging ✅ **COMPLETE**

**Objective**: Intercept GraphQL mutations and create audit events
**Status**: ✅ Complete (PostgreSQL-native crypto, not Python)

#### TDD Cycle 3.1: Event Logger

**RED**: Write failing test for event logging

```python
# tests/integration/enterprise/audit/test_event_logger.py

async def test_log_audit_event():
    """Verify audit event is logged to database with proper chain."""
    from fraiseql.enterprise.audit.event_logger import AuditLogger

    logger = AuditLogger(db_repo)

    event_id = await logger.log_event(
        event_type='user.created',
        event_data={'username': 'testuser', 'email': 'test@example.com'},
        user_id='123e4567-e89b-12d3-a456-426614174000',
        tenant_id='tenant-123',
        ip_address='192.168.1.100'
    )

    # Retrieve logged event
    events = await db_repo.run(DatabaseQuery(
        statement="SELECT * FROM audit_events WHERE id = %s",
        params={'id': event_id},
        fetch_result=True
    ))

    assert len(events) == 1
    event = events[0]
    assert event['event_type'] == 'user.created'
    assert event['event_hash'] is not None
    assert event['signature'] is not None
    # Expected failure: AuditLogger not implemented
```

**GREEN**: Implement minimal event logger

```python
# src/fraiseql/enterprise/audit/event_logger.py

from uuid import UUID, uuid4
from datetime import datetime
from typing import Any, Optional
from fraiseql.db import FraiseQLRepository, DatabaseQuery
from fraiseql.enterprise.crypto.hashing import hash_audit_event
from fraiseql.enterprise.crypto.signing import get_key_manager

class AuditLogger:
    """Logs audit events with cryptographic chain."""

    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo
        self.key_manager = get_key_manager()

    async def log_event(
        self,
        event_type: str,
        event_data: dict[str, Any],
        user_id: Optional[str] = None,
        tenant_id: Optional[str] = None,
        ip_address: Optional[str] = None
    ) -> UUID:
        """Log an audit event with cryptographic chain.

        Args:
            event_type: Type of event (e.g., 'user.login', 'data.modified')
            event_data: Event-specific data
            user_id: ID of user who triggered event
            tenant_id: Tenant context
            ip_address: Source IP address

        Returns:
            UUID of created audit event
        """
        # Get previous event hash for chain
        previous_hash = await self._get_latest_hash(tenant_id)

        # Create event payload
        timestamp = datetime.utcnow()
        event_payload = {
            'event_type': event_type,
            'event_data': event_data,
            'user_id': user_id,
            'tenant_id': tenant_id,
            'timestamp': timestamp.isoformat(),
            'ip_address': ip_address
        }

        # Generate hash and signature
        event_hash = hash_audit_event(event_payload, previous_hash)
        signature = self.key_manager.sign(event_hash)

        # Insert into database
        event_id = uuid4()
        await self.repo.run(DatabaseQuery(
            statement="""
                INSERT INTO audit_events (
                    id, event_type, event_data, user_id, tenant_id,
                    timestamp, ip_address, previous_hash, event_hash, signature
                ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
            """,
            params={
                'id': event_id,
                'event_type': event_type,
                'event_data': event_data,
                'user_id': user_id,
                'tenant_id': tenant_id,
                'timestamp': timestamp,
                'ip_address': ip_address,
                'previous_hash': previous_hash,
                'event_hash': event_hash,
                'signature': signature
            },
            fetch_result=False
        ))

        return event_id

    async def _get_latest_hash(self, tenant_id: Optional[str]) -> Optional[str]:
        """Get hash of most recent audit event in chain."""
        result = await self.repo.run(DatabaseQuery(
            statement="""
                SELECT event_hash FROM audit_events
                WHERE tenant_id = %s OR (tenant_id IS NULL AND %s IS NULL)
                ORDER BY timestamp DESC
                LIMIT 1
            """,
            params={'tenant_id': tenant_id},
            fetch_result=True
        ))

        return result[0]['event_hash'] if result else None
```

**REFACTOR**: Add batching and error handling

```python
class AuditLogger:
    """Logs audit events with cryptographic chain and batching support."""

    def __init__(self, repo: FraiseQLRepository, batch_size: int = 100):
        self.repo = repo
        self.key_manager = get_key_manager()
        self.batch_size = batch_size
        self._batch: list[dict] = []

    async def log_event(
        self,
        event_type: str,
        event_data: dict[str, Any],
        user_id: Optional[str] = None,
        tenant_id: Optional[str] = None,
        ip_address: Optional[str] = None,
        immediate: bool = True
    ) -> UUID:
        """Log audit event (batched or immediate).

        Args:
            event_type: Type of event
            event_data: Event data
            user_id: User ID
            tenant_id: Tenant ID
            ip_address: Source IP
            immediate: If True, write immediately; if False, batch

        Returns:
            UUID of event
        """
        event = self._prepare_event(
            event_type, event_data, user_id, tenant_id, ip_address
        )

        if immediate:
            return await self._write_event(event)
        else:
            self._batch.append(event)
            if len(self._batch) >= self.batch_size:
                await self.flush_batch()
            return event['id']

    async def flush_batch(self):
        """Write all batched events to database."""
        if not self._batch:
            return

        # Write events in transaction
        async def write_batch(conn):
            for event in self._batch:
                await self._write_event(event, conn)

        await self.repo.run_in_transaction(write_batch)
        self._batch.clear()
```

**QA**: Test event logging and batching

```bash
uv run pytest tests/integration/enterprise/audit/test_event_logger.py -v
uv run pytest tests/integration/enterprise/audit/ -v
```

---

### Phase 4: GraphQL Mutation Interceptors ✅ **COMPLETE**

**Objective**: Automatically capture all mutations for audit trail
**Status**: ✅ Complete (Unified table approach, no separate interceptors)

#### TDD Cycle 4.1: Mutation Interceptor

**RED**: Write failing test for automatic mutation logging

```python
# tests/integration/enterprise/audit/test_interceptors.py

async def test_mutation_auto_logging():
    """Verify mutations are automatically logged to audit trail."""
    # Execute a mutation
    result = await execute_graphql("""
        mutation {
            createUser(input: {
                username: "testuser"
                email: "test@example.com"
            }) {
                user { id username }
            }
        }
    """, context={'user_id': 'admin-123', 'ip': '192.168.1.100'})

    assert result['data']['createUser']['user']['username'] == 'testuser'

    # Check audit log
    events = await db_repo.run(DatabaseQuery(
        statement="SELECT * FROM audit_events WHERE event_type = 'mutation.createUser'",
        params={},
        fetch_result=True
    ))

    assert len(events) == 1
    assert events[0]['event_data']['input']['username'] == 'testuser'
    # Expected failure: interceptor not implemented
```

**GREEN**: Implement minimal mutation interceptor

```python
# src/fraiseql/enterprise/audit/interceptors.py

from typing import Any, Callable
from graphql import GraphQLResolveInfo
from fraiseql.enterprise.audit.event_logger import AuditLogger

class AuditInterceptor:
    """Intercepts GraphQL mutations for audit logging."""

    def __init__(self, audit_logger: AuditLogger):
        self.logger = audit_logger

    async def intercept_mutation(
        self,
        next_resolver: Callable,
        obj: Any,
        info: GraphQLResolveInfo,
        **kwargs
    ):
        """Intercept mutation execution and log to audit trail."""
        # Execute mutation
        result = await next_resolver(obj, info, **kwargs)

        # Log to audit trail
        context = info.context
        await self.logger.log_event(
            event_type=f"mutation.{info.field_name}",
            event_data={
                'input': kwargs,
                'result': result
            },
            user_id=context.get('user_id'),
            tenant_id=context.get('tenant_id'),
            ip_address=context.get('ip')
        )

        return result
```

**REFACTOR**: Add selective logging and PII filtering

```python
class AuditInterceptor:
    """GraphQL mutation interceptor with configurable audit logging."""

    def __init__(
        self,
        audit_logger: AuditLogger,
        exclude_fields: set[str] | None = None,
        pii_fields: set[str] | None = None
    ):
        self.logger = audit_logger
        self.exclude_fields = exclude_fields or set()
        self.pii_fields = pii_fields or {'password', 'ssn', 'credit_card'}

    async def intercept_mutation(
        self,
        next_resolver: Callable,
        obj: Any,
        info: GraphQLResolveInfo,
        **kwargs
    ):
        """Intercept and log mutation with PII filtering."""
        mutation_name = info.field_name

        # Skip excluded mutations
        if mutation_name in self.exclude_fields:
            return await next_resolver(obj, info, **kwargs)

        # Filter PII from input
        filtered_input = self._filter_pii(kwargs)

        # Execute mutation
        start_time = datetime.utcnow()
        try:
            result = await next_resolver(obj, info, **kwargs)
            success = True
            error = None
        except Exception as e:
            success = False
            error = str(e)
            raise
        finally:
            # Log audit event (even on failure)
            duration_ms = (datetime.utcnow() - start_time).total_seconds() * 1000

            context = info.context
            await self.logger.log_event(
                event_type=f"mutation.{mutation_name}",
                event_data={
                    'input': filtered_input,
                    'success': success,
                    'error': error,
                    'duration_ms': duration_ms
                },
                user_id=context.get('user_id'),
                tenant_id=context.get('tenant_id'),
                ip_address=context.get('ip')
            )

        return result

    def _filter_pii(self, data: dict[str, Any]) -> dict[str, Any]:
        """Remove PII fields from data before logging."""
        filtered = {}
        for key, value in data.items():
            if key in self.pii_fields:
                filtered[key] = '[REDACTED]'
            elif isinstance(value, dict):
                filtered[key] = self._filter_pii(value)
            else:
                filtered[key] = value
        return filtered
```

**QA**: Test interception and PII filtering

```bash
uv run pytest tests/integration/enterprise/audit/test_interceptors.py -v
```

---

### Phase 5: Chain Verification API

**Objective**: Provide APIs for verifying unified audit_events table integrity and generating compliance reports

#### TDD Cycle 5.1: Chain Integrity Verification

**RED**: Write failing test for chain verification

```python
# tests/integration/enterprise/audit/test_verification.py

async def test_verify_audit_chain():
    """Verify audit chain integrity detection."""
    from fraiseql.enterprise.audit.verification import verify_chain

    # Create valid chain of events
    logger = AuditLogger(db_repo)
    await logger.log_event('event.1', {'data': 'first'}, tenant_id='test')
    await logger.log_event('event.2', {'data': 'second'}, tenant_id='test')
    await logger.log_event('event.3', {'data': 'third'}, tenant_id='test')

    # Verify chain
    result = await verify_chain(db_repo, tenant_id='test')

    assert result['valid'] is True
    assert result['total_events'] == 3
    assert result['broken_links'] == 0
    # Expected failure: verify_chain not implemented
```

**GREEN**: Implement minimal chain verification

```python
# src/fraiseql/enterprise/audit/verification.py

from typing import Optional
from fraiseql.db import FraiseQLRepository, DatabaseQuery
from fraiseql.enterprise.crypto.hashing import hash_audit_event
from fraiseql.enterprise.crypto.signing import get_key_manager

async def verify_chain(
    repo: FraiseQLRepository,
    tenant_id: Optional[str] = None
) -> dict[str, Any]:
    """Verify integrity of audit event chain.

    Args:
        repo: Database repository
        tenant_id: Optional tenant filter

    Returns:
        Dictionary with verification results
    """
    # Retrieve all events in order
    events = await repo.run(DatabaseQuery(
        statement="""
            SELECT * FROM audit_events
            WHERE tenant_id = %s OR (tenant_id IS NULL AND %s IS NULL)
            ORDER BY timestamp ASC
        """,
        params={'tenant_id': tenant_id},
        fetch_result=True
    ))

    if not events:
        return {
            'valid': True,
            'total_events': 0,
            'broken_links': 0
        }

    key_manager = get_key_manager()
    broken_links = []
    previous_hash = None

    for event in events:
        # Verify hash links to previous event
        event_payload = {
            'event_type': event['event_type'],
            'event_data': event['event_data'],
            'user_id': str(event['user_id']) if event['user_id'] else None,
            'tenant_id': str(event['tenant_id']) if event['tenant_id'] else None,
            'timestamp': event['timestamp'].isoformat(),
            'ip_address': event['ip_address']
        }

        expected_hash = hash_audit_event(event_payload, previous_hash)

        if expected_hash != event['event_hash']:
            broken_links.append({
                'event_id': str(event['id']),
                'reason': 'hash_mismatch'
            })

        # Verify signature
        if not key_manager.verify(event['event_hash'], event['signature']):
            broken_links.append({
                'event_id': str(event['id']),
                'reason': 'invalid_signature'
            })

        previous_hash = event['event_hash']

    return {
        'valid': len(broken_links) == 0,
        'total_events': len(events),
        'broken_links': len(broken_links),
        'details': broken_links if broken_links else None
    }
```

**REFACTOR**: Add GraphQL API and batch verification

```python
# Add GraphQL query type
@strawberry.type
class AuditQuery:
    """GraphQL queries for audit system."""

    @strawberry.field
    async def verify_audit_chain(
        self,
        info: Info,
        tenant_id: Optional[UUID] = None
    ) -> AuditChainVerification:
        """Verify integrity of audit event chain."""
        repo = info.context['repo']
        result = await verify_chain(repo, tenant_id=str(tenant_id) if tenant_id else None)

        return AuditChainVerification(
            valid=result['valid'],
            total_events=result['total_events'],
            broken_links=result['broken_links'],
            verification_timestamp=datetime.utcnow()
        )

@strawberry.type
class AuditChainVerification:
    """Result of audit chain verification."""
    valid: bool
    total_events: int
    broken_links: int
    verification_timestamp: datetime
```

**QA**: Test verification with tampered events

```bash
uv run pytest tests/integration/enterprise/audit/test_verification.py -v
```

---

### Phase 6: Compliance Reports

**Objective**: Generate SOX/HIPAA compliance reports

#### TDD Cycle 6.1: SOX Compliance Report

**RED**: Write failing test for SOX report

```python
# tests/integration/enterprise/audit/test_compliance_reports.py

async def test_sox_compliance_report():
    """Verify SOX compliance report generation."""
    from fraiseql.enterprise.audit.compliance_reports import generate_sox_report

    # Create audit events for financial operations
    logger = AuditLogger(db_repo)
    await logger.log_event('financial.transaction', {'amount': 1000}, user_id='user1')
    await logger.log_event('financial.approval', {'transaction_id': '123'}, user_id='user2')

    # Generate SOX report
    report = await generate_sox_report(
        repo=db_repo,
        start_date=datetime(2025, 1, 1),
        end_date=datetime(2025, 12, 31)
    )

    assert 'total_events' in report
    assert 'chain_integrity' in report
    assert 'segregation_of_duties' in report
    # Expected failure: generate_sox_report not implemented
```

**GREEN**: Implement minimal SOX report

```python
# src/fraiseql/enterprise/audit/compliance_reports.py

from datetime import datetime
from typing import Any
from fraiseql.db import FraiseQLRepository, DatabaseQuery
from fraiseql.enterprise.audit.verification import verify_chain

async def generate_sox_report(
    repo: FraiseQLRepository,
    start_date: datetime,
    end_date: datetime,
    tenant_id: Optional[str] = None
) -> dict[str, Any]:
    """Generate SOX compliance report.

    SOX requirements:
    - Immutable audit trail
    - Access controls
    - Segregation of duties
    - Change tracking

    Args:
        repo: Database repository
        start_date: Report period start
        end_date: Report period end
        tenant_id: Optional tenant filter

    Returns:
        SOX compliance report
    """
    # Verify chain integrity
    chain_result = await verify_chain(repo, tenant_id)

    # Get event counts by type
    events = await repo.run(DatabaseQuery(
        statement="""
            SELECT event_type, COUNT(*) as count
            FROM audit_events
            WHERE timestamp >= %s AND timestamp <= %s
            AND (tenant_id = %s OR (tenant_id IS NULL AND %s IS NULL))
            GROUP BY event_type
        """,
        params={
            'start_date': start_date,
            'end_date': end_date,
            'tenant_id': tenant_id
        },
        fetch_result=True
    ))

    # Analyze segregation of duties
    # (e.g., same user shouldn't create and approve financial transactions)
    violations = await _check_segregation_violations(repo, start_date, end_date)

    return {
        'period': {
            'start': start_date.isoformat(),
            'end': end_date.isoformat()
        },
        'chain_integrity': chain_result,
        'total_events': chain_result['total_events'],
        'events_by_type': {e['event_type']: e['count'] for e in events},
        'segregation_of_duties': {
            'violations': len(violations),
            'details': violations
        },
        'compliant': chain_result['valid'] and len(violations) == 0
    }

async def _check_segregation_violations(
    repo: FraiseQLRepository,
    start_date: datetime,
    end_date: datetime
) -> list[dict]:
    """Check for segregation of duties violations."""
    # Find cases where same user created and approved
    results = await repo.run(DatabaseQuery(
        statement="""
            WITH transactions AS (
                SELECT
                    event_data->>'transaction_id' as tx_id,
                    user_id
                FROM audit_events
                WHERE event_type = 'financial.transaction'
                AND timestamp >= %s AND timestamp <= %s
            ),
            approvals AS (
                SELECT
                    event_data->>'transaction_id' as tx_id,
                    user_id
                FROM audit_events
                WHERE event_type = 'financial.approval'
                AND timestamp >= %s AND timestamp <= %s
            )
            SELECT t.tx_id, t.user_id
            FROM transactions t
            INNER JOIN approvals a ON t.tx_id = a.tx_id
            WHERE t.user_id = a.user_id
        """,
        params={
            'start_date': start_date,
            'end_date': end_date
        },
        fetch_result=True
    ))

    return [
        {
            'transaction_id': r['tx_id'],
            'user_id': str(r['user_id']),
            'violation': 'same_user_create_and_approve'
        }
        for r in results
    ]
```

**REFACTOR**: Add HIPAA and export formats

```python
async def generate_hipaa_report(
    repo: FraiseQLRepository,
    start_date: datetime,
    end_date: datetime
) -> dict[str, Any]:
    """Generate HIPAA compliance report.

    HIPAA requirements:
    - Access audit controls
    - Integrity controls
    - Transmission security
    """
    # Similar structure to SOX report
    # Focus on PHI access tracking
    pass

def export_report_pdf(report: dict[str, Any], output_path: str):
    """Export compliance report as PDF."""
    # Use reportlab or similar
    pass

def export_report_csv(report: dict[str, Any], output_path: str):
    """Export compliance report as CSV."""
    # Export event details
    pass
```

**QA**: Test report generation and exports

```bash
uv run pytest tests/integration/enterprise/audit/test_compliance_reports.py -v
uv run pytest tests/integration/enterprise/audit/ --tb=short
```

---

### Success Criteria

**Phase 1: Database & Types**

- [ ] Append-only audit table created
- [ ] Automatic partitioning working
- [ ] GraphQL types defined
- [ ] All tests pass

**Phase 2: Cryptography**

- [ ] SHA-256 hashing implemented
- [ ] HMAC signing working
- [ ] Key rotation supported
- [ ] Chain links verified

**Phase 3: Event Logging**

- [ ] Events logged with context
- [ ] Chain maintained correctly
- [ ] Batching implemented
- [ ] PII filtering working

**Phase 4: Interception**

- [ ] Mutations auto-logged
- [ ] Queries tracked (optional)
- [ ] Auth events captured
- [ ] Performance acceptable (<5ms overhead)

**Phase 5: Verification**

- [ ] Chain integrity verified
- [ ] Tampering detected
- [ ] GraphQL API functional
- [ ] Performance optimized

**Phase 6: Compliance**

- [ ] SOX reports generated
- [ ] HIPAA reports generated
- [ ] PDF/CSV exports working
- [ ] Segregation violations detected

**Overall Success Metrics**:

- [ ] 100% mutation coverage
- [ ] <10ms audit overhead
- [ ] Chain verification in <1s for 10k events
- [ ] SOX/HIPAA compliant
- [ ] Documentation complete

---

## Feature 2: Advanced RBAC (Role-Based Access Control)

**Complexity**: Complex | **Duration**: 4-6 weeks | **Priority**: 10/10

### Executive Summary

Implement a hierarchical role-based access control system that supports complex organizational structures with 10,000+ users. The system provides role inheritance, permission caching, and integrates with FraiseQL's GraphQL field-level security. It serves as the foundation for the ABAC system (Tier 2) and demonstrates enterprise-grade security architecture.

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    GraphQL Request Layer                     │
│              (Authenticated User Context)                    │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│            Permission Resolver (Cached)                      │
│  - Resolves effective permissions for user                  │
│  - Handles role hierarchy and inheritance                   │
│  - 2-layer cache: Request + Redis                           │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│              Role Hierarchy Engine                           │
│  - Computes transitive role inheritance                     │
│  - Supports multiple inheritance paths                      │
│  - Diamond problem resolution                               │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│         PostgreSQL RBAC Schema                               │
│  - roles (id, name, parent_role_id, permissions)            │
│  - user_roles (user_id, role_id, tenant_id)                 │
│  - permissions (resource, action, constraints)              │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│            Field-Level Authorization                         │
│  - Integrates with @requires_permission directive           │
│  - Row-level security (PostgreSQL RLS)                      │
│  - Column masking for PII                                   │
└─────────────────────────────────────────────────────────────┘
```

### File Structure

```
src/fraiseql/enterprise/
├── rbac/
│   ├── __init__.py
│   ├── models.py                  # Role, Permission, UserRole models
│   ├── resolver.py                # Permission resolution engine
│   ├── hierarchy.py               # Role hierarchy computation
│   ├── cache.py                   # Permission caching layer
│   ├── middleware.py              # GraphQL authorization middleware
│   ├── directives.py              # @requiresRole, @requiresPermission
│   └── types.py                   # GraphQL types for RBAC
└── migrations/
    └── 002_rbac_tables.sql        # RBAC database schema

tests/integration/enterprise/rbac/
├── test_role_hierarchy.py
├── test_permission_resolution.py
├── test_field_level_auth.py
├── test_cache_performance.py
└── test_multi_tenant_rbac.py

docs/enterprise/
├── rbac-guide.md
└── permission-patterns.md
```

---

## PHASES

### Phase 1: Database Schema & Core Models

**Objective**: Create RBAC database schema with role hierarchy support

#### TDD Cycle 1.1: RBAC Database Schema

**RED**: Write failing test for RBAC tables

```python
# tests/integration/enterprise/rbac/test_rbac_schema.py

async def test_rbac_tables_exist():
    """Verify RBAC tables exist with correct schema."""
    tables = ['roles', 'permissions', 'role_permissions', 'user_roles']

    for table in tables:
        result = await db.run(DatabaseQuery(
            statement=f"""
                SELECT column_name, data_type
                FROM information_schema.columns
                WHERE table_name = '{table}'
            """,
            params={},
            fetch_result=True
        ))
        assert len(result) > 0, f"Table {table} should exist"

    # Verify roles table structure
    roles_columns = await get_table_columns('roles')
    assert 'id' in roles_columns
    assert 'name' in roles_columns
    assert 'parent_role_id' in roles_columns  # For hierarchy
    assert 'tenant_id' in roles_columns  # Multi-tenancy
    # Expected failure: tables don't exist
```

**GREEN**: Implement RBAC schema

```sql
-- src/fraiseql/enterprise/migrations/002_rbac_tables.sql

-- Roles table with hierarchy support
CREATE TABLE IF NOT EXISTS roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    description TEXT,
    parent_role_id UUID REFERENCES roles(id) ON DELETE SET NULL,
    tenant_id UUID,  -- NULL for global roles
    is_system BOOLEAN DEFAULT FALSE,  -- System roles can't be deleted
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(name, tenant_id)  -- Unique per tenant
);

-- Permissions catalog
CREATE TABLE IF NOT EXISTS permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource VARCHAR(100) NOT NULL,  -- e.g., 'user', 'product', 'order'
    action VARCHAR(50) NOT NULL,     -- e.g., 'create', 'read', 'update', 'delete'
    description TEXT,
    constraints JSONB,  -- Optional constraints (e.g., {"own_data_only": true})
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(resource, action)
);

-- Role-Permission mapping (many-to-many)
CREATE TABLE IF NOT EXISTS role_permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    granted BOOLEAN DEFAULT TRUE,  -- TRUE = grant, FALSE = revoke (explicit deny)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(role_id, permission_id)
);

-- User-Role assignment
CREATE TABLE IF NOT EXISTS user_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,  -- References users table
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    tenant_id UUID,  -- Scoped to tenant
    granted_by UUID,  -- User who granted this role
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,  -- Optional expiration
    UNIQUE(user_id, role_id, tenant_id)
);

-- Indexes for performance
CREATE INDEX idx_roles_parent ON roles(parent_role_id);
CREATE INDEX idx_roles_tenant ON roles(tenant_id);
CREATE INDEX idx_user_roles_user ON user_roles(user_id, tenant_id);
CREATE INDEX idx_user_roles_role ON user_roles(role_id);
CREATE INDEX idx_role_permissions_role ON role_permissions(role_id);

-- Function to compute role hierarchy (recursive)
CREATE OR REPLACE FUNCTION get_inherited_roles(p_role_id UUID)
RETURNS TABLE(role_id UUID, depth INT) AS $$
    WITH RECURSIVE role_hierarchy AS (
        -- Base case: the role itself
        SELECT id as role_id, 0 as depth
        FROM roles
        WHERE id = p_role_id

        UNION ALL

        -- Recursive case: parent roles
        SELECT r.parent_role_id as role_id, rh.depth + 1 as depth
        FROM roles r
        INNER JOIN role_hierarchy rh ON r.id = rh.role_id
        WHERE r.parent_role_id IS NOT NULL
        AND rh.depth < 10  -- Prevent infinite loops
    )
    SELECT DISTINCT role_id, MIN(depth) as depth
    FROM role_hierarchy
    WHERE role_id IS NOT NULL
    GROUP BY role_id
    ORDER BY depth;
$$ LANGUAGE SQL STABLE;
```

**REFACTOR**: Add seed data for common roles

```sql
-- Seed common system roles
INSERT INTO roles (id, name, description, parent_role_id, is_system) VALUES
    ('00000000-0000-0000-0000-000000000001', 'super_admin', 'Full system access', NULL, TRUE),
    ('00000000-0000-0000-0000-000000000002', 'admin', 'Tenant administrator', NULL, TRUE),
    ('00000000-0000-0000-0000-000000000003', 'manager', 'Department manager', '00000000-0000-0000-0000-000000000002', TRUE),
    ('00000000-0000-0000-0000-000000000004', 'user', 'Standard user', '00000000-0000-0000-0000-000000000003', TRUE),
    ('00000000-0000-0000-0000-000000000005', 'viewer', 'Read-only access', '00000000-0000-0000-0000-000000000004', TRUE)
ON CONFLICT (name, tenant_id) DO NOTHING;

-- Seed common permissions
INSERT INTO permissions (resource, action, description) VALUES
    ('user', 'create', 'Create new users'),
    ('user', 'read', 'View user data'),
    ('user', 'update', 'Modify user data'),
    ('user', 'delete', 'Delete users'),
    ('role', 'assign', 'Assign roles to users'),
    ('role', 'create', 'Create new roles'),
    ('audit', 'read', 'View audit logs'),
    ('settings', 'update', 'Modify system settings')
ON CONFLICT (resource, action) DO NOTHING;
```

**QA**: Verify schema and hierarchy function

```bash
uv run pytest tests/integration/enterprise/rbac/test_rbac_schema.py -v
```

---

#### TDD Cycle 1.2: Python Models

**RED**: Write failing test for Role model

```python
# tests/integration/enterprise/rbac/test_models.py

def test_role_model_creation():
    """Verify Role model instantiation."""
    from fraiseql.enterprise.rbac.models import Role

    role = Role(
        id='123e4567-e89b-12d3-a456-426614174000',
        name='developer',
        description='Software developer',
        parent_role_id='parent-role-123',
        tenant_id='tenant-123'
    )

    assert role.name == 'developer'
    assert role.parent_role_id == 'parent-role-123'
    # Expected failure: Role model not defined
```

**GREEN**: Implement minimal models

```python
# src/fraiseql/enterprise/rbac/models.py

from dataclasses import dataclass
from datetime import datetime
from typing import Optional
from uuid import UUID

@dataclass
class Role:
    """Role with optional hierarchy."""
    id: UUID
    name: str
    description: Optional[str] = None
    parent_role_id: Optional[UUID] = None
    tenant_id: Optional[UUID] = None
    is_system: bool = False
    created_at: datetime = None
    updated_at: datetime = None

@dataclass
class Permission:
    """Permission for resource action."""
    id: UUID
    resource: str
    action: str
    description: Optional[str] = None
    constraints: Optional[dict] = None
    created_at: datetime = None

@dataclass
class UserRole:
    """User-Role assignment."""
    id: UUID
    user_id: UUID
    role_id: UUID
    tenant_id: Optional[UUID] = None
    granted_by: Optional[UUID] = None
    granted_at: datetime = None
    expires_at: Optional[datetime] = None
```

**REFACTOR**: Add GraphQL types

```python
# src/fraiseql/enterprise/rbac/types.py

import strawberry
from typing import Optional
from uuid import UUID
from datetime import datetime

@strawberry.type
class Role:
    """Role in RBAC system."""
    id: UUID
    name: str
    description: Optional[str]
    parent_role: Optional["Role"]
    permissions: list["Permission"]
    user_count: int

    @strawberry.field
    async def inherited_permissions(self, info: Info) -> list["Permission"]:
        """Get all permissions including inherited from parent roles."""
        from fraiseql.enterprise.rbac.resolver import PermissionResolver
        resolver = PermissionResolver(info.context['repo'])
        return await resolver.get_role_permissions(self.id, include_inherited=True)

@strawberry.type
class Permission:
    """Permission for resource action."""
    id: UUID
    resource: str
    action: str
    description: Optional[str]
    constraints: Optional[strawberry.scalars.JSON]

@strawberry.input
class CreateRoleInput:
    """Input for creating a role."""
    name: str
    description: Optional[str] = None
    parent_role_id: Optional[UUID] = None
    permission_ids: list[UUID] = strawberry.field(default_factory=list)

@strawberry.type
class RBACQuery:
    """GraphQL queries for RBAC."""

    @strawberry.field
    async def roles(
        self,
        info: Info,
        tenant_id: Optional[UUID] = None
    ) -> list[Role]:
        """List all roles."""
        repo = info.context['repo']
        results = await repo.run(DatabaseQuery(
            statement="""
                SELECT * FROM roles
                WHERE tenant_id = %s OR (tenant_id IS NULL AND %s IS NULL)
                ORDER BY name
            """,
            params={'tenant_id': str(tenant_id) if tenant_id else None},
            fetch_result=True
        ))
        return [Role(**row) for row in results]

    @strawberry.field
    async def permissions(self, info: Info) -> list[Permission]:
        """List all permissions."""
        repo = info.context['repo']
        results = await repo.run(DatabaseQuery(
            statement="SELECT * FROM permissions ORDER BY resource, action",
            params={},
            fetch_result=True
        ))
        return [Permission(**row) for row in results]

    @strawberry.field
    async def user_roles(
        self,
        info: Info,
        user_id: UUID
    ) -> list[Role]:
        """Get roles assigned to a user."""
        from fraiseql.enterprise.rbac.resolver import PermissionResolver
        resolver = PermissionResolver(info.context['repo'])
        return await resolver.get_user_roles(user_id)
```

**QA**: Test models and GraphQL types

```bash
uv run pytest tests/integration/enterprise/rbac/test_models.py -v
uv run pytest tests/integration/enterprise/rbac/test_graphql_types.py -v
```

---

### Phase 2: Role Hierarchy Engine

**Objective**: Implement transitive role inheritance with cycle detection

#### TDD Cycle 2.1: Hierarchy Computation

**RED**: Write failing test for role hierarchy

```python
# tests/integration/enterprise/rbac/test_role_hierarchy.py

async def test_role_inheritance_chain():
    """Verify role inherits permissions from parent roles."""
    from fraiseql.enterprise.rbac.hierarchy import RoleHierarchy

    # Create role chain: admin -> manager -> developer -> junior_dev
    # junior_dev should inherit all permissions from the chain

    hierarchy = RoleHierarchy(db_repo)
    inherited_roles = await hierarchy.get_inherited_roles('junior-dev-role-id')

    role_names = [r.name for r in inherited_roles]
    assert 'junior_dev' in role_names
    assert 'developer' in role_names
    assert 'manager' in role_names
    assert 'admin' in role_names
    assert len(role_names) == 4
    # Expected failure: get_inherited_roles not implemented
```

**GREEN**: Implement minimal hierarchy engine

```python
# src/fraiseql.enterprise/rbac/hierarchy.py

from typing import List
from uuid import UUID
from fraiseql.db import FraiseQLRepository, DatabaseQuery
from fraiseql.enterprise.rbac.models import Role

class RoleHierarchy:
    """Computes role hierarchy and inheritance."""

    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo

    async def get_inherited_roles(self, role_id: UUID) -> List[Role]:
        """Get all roles in inheritance chain (including self).

        Args:
            role_id: Starting role ID

        Returns:
            List of roles from most specific to most general
        """
        results = await self.repo.run(DatabaseQuery(
            statement="SELECT * FROM get_inherited_roles(%s)",
            params={'role_id': str(role_id)},
            fetch_result=True
        ))

        # Get full role details
        role_ids = [r['role_id'] for r in results]
        roles = await self.repo.run(DatabaseQuery(
            statement="""
                SELECT * FROM roles
                WHERE id = ANY(%s)
                ORDER BY name
            """,
            params={'ids': role_ids},
            fetch_result=True
        ))

        return [Role(**row) for row in roles]
```

**REFACTOR**: Add cycle detection and caching

```python
class RoleHierarchy:
    """Role hierarchy engine with cycle detection and caching."""

    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo
        self._hierarchy_cache: dict[UUID, List[Role]] = {}

    async def get_inherited_roles(
        self,
        role_id: UUID,
        use_cache: bool = True
    ) -> List[Role]:
        """Get inherited roles with caching.

        Args:
            role_id: Starting role
            use_cache: Whether to use cache

        Returns:
            List of roles in inheritance order

        Raises:
            ValueError: If cycle detected
        """
        if use_cache and role_id in self._hierarchy_cache:
            return self._hierarchy_cache[role_id]

        # Use PostgreSQL recursive CTE (handles cycles with depth limit)
        results = await self.repo.run(DatabaseQuery(
            statement="SELECT * FROM get_inherited_roles(%s)",
            params={'role_id': str(role_id)},
            fetch_result=True
        ))

        if not results:
            return []

        # Check if we hit cycle detection limit (depth = 10)
        if any(r['depth'] >= 10 for r in results):
            raise ValueError(f"Cycle detected in role hierarchy for role {role_id}")

        # Get full role details
        role_ids = [r['role_id'] for r in results]
        roles_data = await self.repo.run(DatabaseQuery(
            statement="""
                SELECT * FROM roles
                WHERE id = ANY(%s::uuid[])
            """,
            params={'ids': role_ids},
            fetch_result=True
        ))

        roles = [Role(**row) for row in roles_data]

        # Cache result
        self._hierarchy_cache[role_id] = roles

        return roles

    def clear_cache(self, role_id: Optional[UUID] = None):
        """Clear hierarchy cache.

        Args:
            role_id: If provided, clear only this role. Otherwise clear all.
        """
        if role_id:
            self._hierarchy_cache.pop(role_id, None)
        else:
            self._hierarchy_cache.clear()
```

**QA**: Test hierarchy with complex chains

```bash
uv run pytest tests/integration/enterprise/rbac/test_role_hierarchy.py -v
```

---

### Phase 3: Permission Resolution Engine

**Objective**: Resolve effective permissions for users with caching

#### TDD Cycle 3.1: Permission Resolution

**RED**: Write failing test for permission resolution

```python
# tests/integration/enterprise/rbac/test_permission_resolution.py

async def test_user_effective_permissions():
    """Verify user permissions are computed from all assigned roles."""
    from fraiseql.enterprise.rbac.resolver import PermissionResolver

    # User has roles: [developer, team_lead]
    # developer inherits from: user
    # team_lead inherits from: developer
    # Expected permissions: all from user + developer + team_lead

    resolver = PermissionResolver(db_repo)
    permissions = await resolver.get_user_permissions('user-123')

    permission_actions = {f"{p.resource}.{p.action}" for p in permissions}
    assert 'user.read' in permission_actions  # From 'user' role
    assert 'code.write' in permission_actions  # From 'developer' role
    assert 'team.manage' in permission_actions  # From 'team_lead' role
    # Expected failure: get_user_permissions not implemented
```

**GREEN**: Implement minimal permission resolver

```python
# src/fraiseql/enterprise/rbac/resolver.py

from typing import List, Set
from uuid import UUID
from fraiseql.db import FraiseQLRepository, DatabaseQuery
from fraiseql.enterprise.rbac.models import Permission, Role
from fraiseql.enterprise.rbac.hierarchy import RoleHierarchy

class PermissionResolver:
    """Resolves effective permissions for users."""

    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo
        self.hierarchy = RoleHierarchy(repo)

    async def get_user_permissions(
        self,
        user_id: UUID,
        tenant_id: Optional[UUID] = None
    ) -> List[Permission]:
        """Get all effective permissions for a user.

        Computes permissions from all assigned roles and their parents.

        Args:
            user_id: User ID
            tenant_id: Optional tenant scope

        Returns:
            List of effective permissions
        """
        # Get user's direct roles
        user_roles = await self._get_user_roles(user_id, tenant_id)

        # Get all inherited roles
        all_role_ids: Set[UUID] = set()
        for role in user_roles:
            inherited = await self.hierarchy.get_inherited_roles(role.id)
            all_role_ids.update(r.id for r in inherited)

        if not all_role_ids:
            return []

        # Get permissions for all roles
        permissions = await self.repo.run(DatabaseQuery(
            statement="""
                SELECT DISTINCT p.*
                FROM permissions p
                INNER JOIN role_permissions rp ON p.id = rp.permission_id
                WHERE rp.role_id = ANY(%s::uuid[])
                AND rp.granted = TRUE
            """,
            params={'role_ids': list(all_role_ids)},
            fetch_result=True
        ))

        return [Permission(**row) for row in permissions]

    async def _get_user_roles(
        self,
        user_id: UUID,
        tenant_id: Optional[UUID]
    ) -> List[Role]:
        """Get roles directly assigned to user."""
        results = await self.repo.run(DatabaseQuery(
            statement="""
                SELECT r.*
                FROM roles r
                INNER JOIN user_roles ur ON r.id = ur.role_id
                WHERE ur.user_id = %s
                AND (ur.tenant_id = %s OR (ur.tenant_id IS NULL AND %s IS NULL))
                AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
            """,
            params={
                'user_id': str(user_id),
                'tenant_id': str(tenant_id) if tenant_id else None
            },
            fetch_result=True
        ))

        return [Role(**row) for row in results]
```

**REFACTOR**: Add 2-layer caching (request + Redis)

```python
# src/fraiseql/enterprise/rbac/cache.py

import hashlib
import json
from typing import List, Optional
from uuid import UUID
from datetime import timedelta
from fraiseql.enterprise.rbac.models import Permission

class PermissionCache:
    """2-layer permission cache (request-level + Redis)."""

    def __init__(self, redis_client=None):
        self.redis = redis_client
        self._request_cache: dict[str, List[Permission]] = {}
        self._cache_ttl = timedelta(minutes=5)

    def _make_key(self, user_id: UUID, tenant_id: Optional[UUID]) -> str:
        """Generate cache key for user permissions."""
        data = f"{user_id}:{tenant_id or 'global'}"
        return f"rbac:permissions:{hashlib.md5(data.encode()).hexdigest()}"

    async def get(
        self,
        user_id: UUID,
        tenant_id: Optional[UUID]
    ) -> Optional[List[Permission]]:
        """Get cached permissions."""
        key = self._make_key(user_id, tenant_id)

        # Try request-level cache first (fastest)
        if key in self._request_cache:
            return self._request_cache[key]

        # Try Redis cache
        if self.redis:
            cached_data = await self.redis.get(key)
            if cached_data:
                permissions = [
                    Permission(**p) for p in json.loads(cached_data)
                ]
                self._request_cache[key] = permissions
                return permissions

        return None

    async def set(
        self,
        user_id: UUID,
        tenant_id: Optional[UUID],
        permissions: List[Permission]
    ):
        """Cache permissions."""
        key = self._make_key(user_id, tenant_id)

        # Store in request cache
        self._request_cache[key] = permissions

        # Store in Redis
        if self.redis:
            data = json.dumps([
                {
                    'id': str(p.id),
                    'resource': p.resource,
                    'action': p.action,
                    'constraints': p.constraints
                }
                for p in permissions
            ])
            await self.redis.setex(
                key,
                self._cache_ttl.total_seconds(),
                data
            )

    def clear_request_cache(self):
        """Clear request-level cache (called at end of request)."""
        self._request_cache.clear()

    async def invalidate_user(self, user_id: UUID, tenant_id: Optional[UUID] = None):
        """Invalidate cache for user (e.g., after role change)."""
        key = self._make_key(user_id, tenant_id)
        self._request_cache.pop(key, None)
        if self.redis:
            await self.redis.delete(key)

# Update PermissionResolver to use cache
class PermissionResolver:
    """Permission resolver with caching."""

    def __init__(self, repo: FraiseQLRepository, cache: PermissionCache = None):
        self.repo = repo
        self.hierarchy = RoleHierarchy(repo)
        self.cache = cache or PermissionCache()

    async def get_user_permissions(
        self,
        user_id: UUID,
        tenant_id: Optional[UUID] = None,
        use_cache: bool = True
    ) -> List[Permission]:
        """Get user permissions with caching."""
        if use_cache:
            cached = await self.cache.get(user_id, tenant_id)
            if cached is not None:
                return cached

        # Compute permissions (same as before)
        permissions = await self._compute_permissions(user_id, tenant_id)

        if use_cache:
            await self.cache.set(user_id, tenant_id, permissions)

        return permissions
```

**QA**: Test permission resolution and caching

```bash
uv run pytest tests/integration/enterprise/rbac/test_permission_resolution.py -v
uv run pytest tests/integration/enterprise/rbac/test_cache_performance.py -v
```

---

### Phase 4: GraphQL Integration & Directives

**Objective**: Integrate RBAC with GraphQL field-level authorization

#### TDD Cycle 4.1: Authorization Directives

**RED**: Write failing test for @requires_permission directive

```python
# tests/integration/enterprise/rbac/test_directives.py

async def test_requires_permission_directive():
    """Verify @requires_permission blocks unauthorized access."""
    # User with 'viewer' role (only has read permissions)
    result = await execute_graphql("""
        mutation {
            deleteUser(id: "user-123") {
                success
            }
        }
    """, context={'user_id': 'viewer-user', 'tenant_id': 'tenant-1'})

    # Should be blocked - viewer doesn't have 'user.delete' permission
    assert result['errors'] is not None
    assert 'permission denied' in result['errors'][0]['message'].lower()
    # Expected failure: directive not implemented
```

**GREEN**: Implement minimal authorization directive

```python
# src/fraiseql/enterprise/rbac/directives.py

import strawberry
from strawberry.types import Info
from typing import Any
from fraiseql.enterprise.rbac.resolver import PermissionResolver

@strawberry.directive(
    locations=[strawberry.directive_location.FIELD_DEFINITION],
    description="Require specific permission to access field"
)
def requires_permission(resource: str, action: str):
    """Directive to enforce permission requirements on fields."""
    def directive_resolver(resolver):
        async def wrapper(*args, **kwargs):
            info: Info = args[1]  # GraphQL Info is second arg
            context = info.context

            # Get user permissions
            resolver_instance = PermissionResolver(context['repo'])
            permissions = await resolver_instance.get_user_permissions(
                user_id=context['user_id'],
                tenant_id=context.get('tenant_id')
            )

            # Check if user has required permission
            has_permission = any(
                p.resource == resource and p.action == action
                for p in permissions
            )

            if not has_permission:
                raise PermissionError(
                    f"Permission denied: requires {resource}.{action}"
                )

            # Execute field resolver
            return await resolver(*args, **kwargs)

        return wrapper
    return directive_resolver

@strawberry.directive(
    locations=[strawberry.directive_location.FIELD_DEFINITION],
    description="Require specific role to access field"
)
def requires_role(role_name: str):
    """Directive to enforce role requirements on fields."""
    def directive_resolver(resolver):
        async def wrapper(*args, **kwargs):
            info: Info = args[1]
            context = info.context

            # Get user roles
            resolver_instance = PermissionResolver(context['repo'])
            roles = await resolver_instance.get_user_roles(
                user_id=context['user_id'],
                tenant_id=context.get('tenant_id')
            )

            # Check if user has required role
            has_role = any(r.name == role_name for r in roles)

            if not has_role:
                raise PermissionError(
                    f"Access denied: requires role '{role_name}'"
                )

            return await resolver(*args, **kwargs)

        return wrapper
    return directive_resolver
```

**REFACTOR**: Add constraint evaluation

```python
@strawberry.directive(
    locations=[strawberry.directive_location.FIELD_DEFINITION]
)
def requires_permission(resource: str, action: str, check_constraints: bool = True):
    """Permission directive with constraint evaluation."""
    def directive_resolver(resolver):
        async def wrapper(*args, **kwargs):
            info: Info = args[1]
            context = info.context

            resolver_instance = PermissionResolver(context['repo'])
            permissions = await resolver_instance.get_user_permissions(
                user_id=context['user_id'],
                tenant_id=context.get('tenant_id')
            )

            # Find matching permission
            matching_permission = None
            for p in permissions:
                if p.resource == resource and p.action == action:
                    matching_permission = p
                    break

            if not matching_permission:
                raise PermissionError(
                    f"Permission denied: requires {resource}.{action}"
                )

            # Evaluate constraints if present
            if check_constraints and matching_permission.constraints:
                constraints_met = await _evaluate_constraints(
                    matching_permission.constraints,
                    context,
                    kwargs
                )
                if not constraints_met:
                    raise PermissionError(
                        f"Permission constraints not satisfied for {resource}.{action}"
                    )

            return await resolver(*args, **kwargs)

        return wrapper
    return directive_resolver

async def _evaluate_constraints(
    constraints: dict,
    context: dict,
    field_args: dict
) -> bool:
    """Evaluate permission constraints.

    Examples:
    - {"own_data_only": true} - can only access own data
    - {"tenant_scoped": true} - must be in same tenant
    - {"max_records": 100} - can't fetch more than 100 records
    """
    if constraints.get('own_data_only'):
        # Check if accessing own data
        target_user_id = field_args.get('user_id') or field_args.get('id')
        if target_user_id != context['user_id']:
            return False

    if constraints.get('tenant_scoped'):
        # Check tenant match
        target_tenant = field_args.get('tenant_id')
        if target_tenant and target_tenant != context.get('tenant_id'):
            return False

    if 'max_records' in constraints:
        # Check record limit
        limit = field_args.get('limit', float('inf'))
        if limit > constraints['max_records']:
            return False

    return True
```

**QA**: Test directives with various scenarios

```bash
uv run pytest tests/integration/enterprise/rbac/test_directives.py -v
```

---

### Phase 5: Row-Level Security (RLS)

**Objective**: Integrate RBAC with PostgreSQL row-level security

#### TDD Cycle 5.1: RLS Policies

**RED**: Write failing test for RLS enforcement

```python
# tests/integration/enterprise/rbac/test_row_level_security.py

async def test_tenant_scoped_rls():
    """Verify users can only see data from their tenant."""
    # Create data in multiple tenants
    await create_test_data(tenant_id='tenant-1', user_id='user-1')
    await create_test_data(tenant_id='tenant-2', user_id='user-2')

    # Query as tenant-1 user
    result = await execute_graphql("""
        query {
            users {
                id
                tenantId
            }
        }
    """, context={'user_id': 'user-1', 'tenant_id': 'tenant-1'})

    users = result['data']['users']
    # Should only see tenant-1 data
    assert all(u['tenantId'] == 'tenant-1' for u in users)
    # Expected failure: RLS not configured
```

**GREEN**: Implement RLS policies

```sql
-- Enable RLS on tables
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
ALTER TABLE orders ENABLE ROW LEVEL SECURITY;
ALTER TABLE products ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see data from their tenant
CREATE POLICY tenant_isolation ON users
    FOR ALL
    USING (
        tenant_id = current_setting('app.tenant_id', TRUE)::UUID
        OR current_setting('app.is_super_admin', TRUE)::BOOLEAN
    );

CREATE POLICY tenant_isolation ON orders
    FOR ALL
    USING (
        tenant_id = current_setting('app.tenant_id', TRUE)::UUID
        OR current_setting('app.is_super_admin', TRUE)::BOOLEAN
    );

-- Policy: Users can only modify their own data (unless admin)
CREATE POLICY own_data_update ON users
    FOR UPDATE
    USING (
        id = current_setting('app.user_id', TRUE)::UUID
        OR EXISTS (
            SELECT 1 FROM user_roles ur
            INNER JOIN roles r ON ur.role_id = r.id
            WHERE ur.user_id = current_setting('app.user_id', TRUE)::UUID
            AND r.name IN ('admin', 'super_admin')
        )
    );
```

**REFACTOR**: Add session variable setup in repository

```python
# Update FraiseQLRepository to set RLS variables
# (Already in src/fraiseql/db.py - enhance it)

async def _set_session_variables(self, cursor_or_conn) -> None:
    """Set PostgreSQL session variables for RLS."""
    from psycopg.sql import SQL, Literal

    if "tenant_id" in self.context:
        await cursor_or_conn.execute(
            SQL("SET LOCAL app.tenant_id = {}").format(
                Literal(str(self.context["tenant_id"]))
            )
        )

    if "user_id" in self.context:
        await cursor_or_conn.execute(
            SQL("SET LOCAL app.user_id = {}").format(
                Literal(str(self.context["user_id"]))
            )
        )

    # Set super_admin flag based on user roles
    if "roles" in self.context:
        is_super_admin = any(r.name == 'super_admin' for r in self.context['roles'])
        await cursor_or_conn.execute(
            SQL("SET LOCAL app.is_super_admin = {}").format(Literal(is_super_admin))
        )
```

**QA**: Test RLS with multiple tenants

```bash
uv run pytest tests/integration/enterprise/rbac/test_row_level_security.py -v
```

---

### Phase 6: Management APIs & UI

**Objective**: Provide GraphQL mutations for role/permission management

#### TDD Cycle 6.1: Role Management Mutations

**RED**: Write failing test for role creation

```python
# tests/integration/enterprise/rbac/test_management_api.py

async def test_create_role_mutation():
    """Verify role creation via GraphQL."""
    result = await execute_graphql("""
        mutation {
            createRole(input: {
                name: "data_scientist"
                description: "Data science team member"
                parentRoleId: "developer-role-id"
                permissionIds: ["perm-1", "perm-2"]
            }) {
                role {
                    id
                    name
                    permissions { resource action }
                }
            }
        }
    """, context={'user_id': 'admin-user', 'tenant_id': 'tenant-1'})

    assert result['data']['createRole']['role']['name'] == 'data_scientist'
    assert len(result['data']['createRole']['role']['permissions']) == 2
    # Expected failure: createRole mutation not implemented
```

**GREEN**: Implement role management mutations

```python
# src/fraiseql/enterprise/rbac/types.py (continued)

@strawberry.type
class RBACMutation:
    """GraphQL mutations for RBAC management."""

    @strawberry.mutation
    @requires_permission(resource='role', action='create')
    async def create_role(
        self,
        info: Info,
        input: CreateRoleInput
    ) -> CreateRoleResponse:
        """Create a new role."""
        repo = info.context['repo']
        tenant_id = info.context.get('tenant_id')
        user_id = info.context['user_id']

        # Create role
        role_id = uuid4()
        await repo.run(DatabaseQuery(
            statement="""
                INSERT INTO roles (id, name, description, parent_role_id, tenant_id)
                VALUES (%s, %s, %s, %s, %s)
            """,
            params={
                'id': role_id,
                'name': input.name,
                'description': input.description,
                'parent_role_id': str(input.parent_role_id) if input.parent_role_id else None,
                'tenant_id': str(tenant_id) if tenant_id else None
            },
            fetch_result=False
        ))

        # Assign permissions to role
        if input.permission_ids:
            for perm_id in input.permission_ids:
                await repo.run(DatabaseQuery(
                    statement="""
                        INSERT INTO role_permissions (role_id, permission_id)
                        VALUES (%s, %s)
                    """,
                    params={'role_id': role_id, 'permission_id': str(perm_id)},
                    fetch_result=False
                ))

        # Log to audit trail
        audit_logger = info.context.get('audit_logger')
        if audit_logger:
            await audit_logger.log_event(
                event_type='rbac.role.created',
                event_data={'role_id': str(role_id), 'name': input.name},
                user_id=str(user_id),
                tenant_id=str(tenant_id) if tenant_id else None
            )

        # Fetch created role
        role = await repo.run(DatabaseQuery(
            statement="SELECT * FROM roles WHERE id = %s",
            params={'id': role_id},
            fetch_result=True
        ))

        return CreateRoleResponse(role=Role(**role[0]))

    @strawberry.mutation
    @requires_permission(resource='role', action='assign')
    async def assign_role_to_user(
        self,
        info: Info,
        user_id: UUID,
        role_id: UUID,
        expires_at: Optional[datetime] = None
    ) -> AssignRoleResponse:
        """Assign a role to a user."""
        repo = info.context['repo']
        tenant_id = info.context.get('tenant_id')
        granted_by = info.context['user_id']

        # Check if role exists
        role_exists = await repo.run(DatabaseQuery(
            statement="SELECT 1 FROM roles WHERE id = %s",
            params={'role_id': str(role_id)},
            fetch_result=True
        ))
        if not role_exists:
            raise ValueError(f"Role {role_id} not found")

        # Assign role
        await repo.run(DatabaseQuery(
            statement="""
                INSERT INTO user_roles (user_id, role_id, tenant_id, granted_by, expires_at)
                VALUES (%s, %s, %s, %s, %s)
                ON CONFLICT (user_id, role_id, tenant_id) DO NOTHING
            """,
            params={
                'user_id': str(user_id),
                'role_id': str(role_id),
                'tenant_id': str(tenant_id) if tenant_id else None,
                'granted_by': str(granted_by),
                'expires_at': expires_at
            },
            fetch_result=False
        ))

        # Invalidate permission cache for user
        cache = info.context.get('permission_cache')
        if cache:
            await cache.invalidate_user(user_id, tenant_id)

        # Log to audit trail
        audit_logger = info.context.get('audit_logger')
        if audit_logger:
            await audit_logger.log_event(
                event_type='rbac.role.assigned',
                event_data={
                    'user_id': str(user_id),
                    'role_id': str(role_id),
                    'granted_by': str(granted_by)
                },
                user_id=str(granted_by),
                tenant_id=str(tenant_id) if tenant_id else None
            )

        return AssignRoleResponse(success=True)

@strawberry.type
class CreateRoleResponse:
    role: Role

@strawberry.type
class AssignRoleResponse:
    success: bool
```

**REFACTOR**: Add more management operations

```python
@strawberry.mutation
@requires_permission(resource='role', action='delete')
async def delete_role(
    self,
    info: Info,
    role_id: UUID
) -> DeleteRoleResponse:
    """Delete a role (if not system role)."""
    repo = info.context['repo']

    # Check if system role
    role = await repo.run(DatabaseQuery(
        statement="SELECT is_system FROM roles WHERE id = %s",
        params={'role_id': str(role_id)},
        fetch_result=True
    ))

    if not role:
        raise ValueError(f"Role {role_id} not found")

    if role[0]['is_system']:
        raise PermissionError("Cannot delete system role")

    # Delete role (CASCADE will remove user_roles and role_permissions)
    await repo.run(DatabaseQuery(
        statement="DELETE FROM roles WHERE id = %s",
        params={'role_id': str(role_id)},
        fetch_result=False
    ))

    return DeleteRoleResponse(success=True)

@strawberry.mutation
@requires_permission(resource='role', action='update')
async def add_permission_to_role(
    self,
    info: Info,
    role_id: UUID,
    permission_id: UUID
) -> AddPermissionResponse:
    """Add permission to role."""
    repo = info.context['repo']

    await repo.run(DatabaseQuery(
        statement="""
            INSERT INTO role_permissions (role_id, permission_id, granted)
            VALUES (%s, %s, TRUE)
            ON CONFLICT (role_id, permission_id) DO UPDATE SET granted = TRUE
        """,
        params={'role_id': str(role_id), 'permission_id': str(permission_id)},
        fetch_result=False
    ))

    # Clear hierarchy cache (permissions changed)
    hierarchy = info.context.get('role_hierarchy')
    if hierarchy:
        hierarchy.clear_cache(role_id)

    return AddPermissionResponse(success=True)
```

**QA**: Test all management operations

```bash
uv run pytest tests/integration/enterprise/rbac/test_management_api.py -v
uv run pytest tests/integration/enterprise/rbac/ --tb=short
```

---

### Success Criteria

**Phase 1: Schema & Models**

- [ ] RBAC tables created with hierarchy support
- [ ] Models defined with proper types
- [ ] GraphQL types implemented
- [ ] All tests pass

**Phase 2: Hierarchy**

- [ ] Role inheritance working
- [ ] Cycle detection preventing infinite loops
- [ ] Hierarchy cache performing well
- [ ] Complex chains resolved correctly

**Phase 3: Permission Resolution**

- [ ] User permissions computed from all roles
- [ ] 2-layer caching implemented
- [ ] Cache invalidation working
- [ ] Performance <5ms for cached lookups

**Phase 4: GraphQL Integration**

- [ ] @requires_permission directive working
- [ ] @requires_role directive working
- [ ] Constraint evaluation implemented
- [ ] Error messages helpful

**Phase 5: Row-Level Security**

- [ ] RLS policies enforced
- [ ] Tenant isolation working
- [ ] Own-data-only constraints working
- [ ] Super admin bypass working

**Phase 6: Management APIs**

- [ ] Role creation/deletion working
- [ ] Role assignment working
- [ ] Permission management working
- [ ] Audit logging integrated

**Overall Success Metrics**:

- [ ] Supports 10,000+ users
- [ ] Permission check <5ms (cached)
- [ ] Hierarchy depth up to 10 levels
- [ ] Multi-tenant isolation enforced
- [ ] 100% test coverage
- [ ] Documentation complete

---

## Feature 3: GDPR Compliance Suite

**Complexity**: Complex | **Duration**: 8-10 weeks | **Priority**: 9/10

### Executive Summary

Implement a comprehensive GDPR compliance system that handles Data Subject Requests (DSRs), consent management, data portability, and the right to erasure. The system provides automated workflows for handling GDPR requests, tracks consent history with immutable audit trails, and generates compliance reports for regulatory audits.

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│              Data Subject Request Portal                     │
│  - Right to Access (export all personal data)               │
│  - Right to Erasure (delete/anonymize data)                 │
│  - Right to Rectification (update incorrect data)           │
│  - Right to Portability (machine-readable export)           │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│            DSR Workflow Engine                               │
│  - Request validation and verification                       │
│  - Multi-stage approval workflow                            │
│  - Automated data discovery                                 │
│  - Execution scheduling                                      │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│         Personal Data Discovery Engine                       │
│  - Scans database for PII/PHI fields                        │
│  - Uses data classification metadata                         │
│  - Discovers related records across tables                  │
│  - Generates complete data graph                            │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│            Consent Management System                         │
│  - Granular consent tracking                                │
│  - Consent history with audit trail                         │
│  - Consent withdrawal handling                              │
│  - Cookie consent integration                               │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│          Data Erasure Engine                                 │
│  - Anonymization strategies (hashing, randomization)        │
│  - Cascading deletion across related data                   │
│  - Retention policy enforcement                             │
│  - Backup scrubbing                                          │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│        Compliance Reporting & Auditing                       │
│  - DSR fulfillment metrics                                  │
│  - Consent statistics                                        │
│  - Data breach notification automation                       │
│  - Regulatory audit trails                                   │
└─────────────────────────────────────────────────────────────┘
```

### File Structure

```
src/fraiseql/enterprise/
├── gdpr/
│   ├── __init__.py
│   ├── dsr/
│   │   ├── __init__.py
│   │   ├── models.py              # DSR request models
│   │   ├── workflow.py            # DSR workflow engine
│   │   ├── discovery.py           # Personal data discovery
│   │   ├── export.py              # Data portability
│   │   └── erasure.py             # Right to erasure
│   ├── consent/
│   │   ├── __init__.py
│   │   ├── models.py              # Consent models
│   │   ├── manager.py             # Consent management
│   │   └── history.py             # Consent audit trail
│   ├── compliance/
│   │   ├── __init__.py
│   │   ├── reports.py             # Compliance reports
│   │   └── breach_notification.py # Data breach automation
│   └── types.py                   # GraphQL types
└── migrations/
    └── 003_gdpr_tables.sql

tests/integration/enterprise/gdpr/
├── test_dsr_workflow.py
├── test_data_discovery.py
├── test_consent_management.py
├── test_right_to_erasure.py
└── test_compliance_reports.py

docs/enterprise/
├── gdpr-guide.md
└── dsr-handbook.md
```

[Due to length constraints, Phases 1-6 for GDPR would follow the same detailed TDD structure as above, covering:

- Phase 1: Database schema for DSRs and consent
- Phase 2: Personal data discovery engine
- Phase 3: Consent management
- Phase 4: Right to erasure implementation
- Phase 5: Data portability export
- Phase 6: Compliance reporting]

---

## Feature 4: Data Classification & Labeling

**Complexity**: Complex | **Duration**: 4-5 weeks | **Priority**: 9/10

### Executive Summary

Implement an automated data classification system that scans database schemas and data to detect and label PII (Personally Identifiable Information), PHI (Protected Health Information), and PCI (Payment Card Industry) data. The system uses pattern matching, heuristics, and optional ML models to automatically classify fields, generates compliance reports, and integrates with encryption and access control systems.

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│          Schema Analysis Engine                              │
│  - Introspects database schema                              │
│  - Analyzes column names, types, constraints                │
│  - Detects common PII/PHI patterns                          │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│        Pattern Matching & Classification                     │
│  - Regex patterns for email, SSN, credit card, etc.         │
│  - Column name heuristics (e.g., "ssn", "email")            │
│  - Data sampling and analysis                               │
│  - ML-based classification (optional)                       │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│         Classification Metadata Store                        │
│  - field_classifications table                              │
│  - Stores: table, column, classification, confidence        │
│  - Manual override support                                  │
│  - Versioned classification history                         │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│         Integration with Security Features                   │
│  - Auto-configure field-level encryption                    │
│  - Generate RBAC policies for PII access                    │
│  - Enable column masking in responses                       │
│  - Configure data retention policies                        │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│        Compliance Reports & Visualization                    │
│  - Data inventory reports                                   │
│  - PII/PHI/PCI data maps                                    │
│  - Risk assessment scores                                   │
│  - Export to CSV/PDF for audits                             │
└─────────────────────────────────────────────────────────────┘
```

### File Structure

```
src/fraiseql/enterprise/
├── classification/
│   ├── __init__.py
│   ├── scanner.py                 # Schema scanning engine
│   ├── patterns.py                # PII/PHI/PCI detection patterns
│   ├── classifier.py              # Classification logic
│   ├── metadata.py                # Classification storage
│   ├── integration.py             # Security feature integration
│   └── types.py                   # GraphQL types
└── migrations/
    └── 004_classification_tables.sql

tests/integration/enterprise/classification/
├── test_pii_detection.py
├── test_phi_detection.py
├── test_pci_detection.py
├── test_auto_classification.py
└── test_compliance_reports.py

docs/enterprise/
├── data-classification.md
└── classification-patterns.md
```

[Phases 1-6 would follow same TDD structure covering:

- Phase 1: Classification schema and models
- Phase 2: Pattern matching engine
- Phase 3: Automated scanning
- Phase 4: Integration with encryption/RBAC
- Phase 5: Manual override and review workflow
- Phase 6: Compliance reporting]

---

## Implementation Timeline

### Quarter 1: Foundation (Weeks 1-13)

**Weeks 1-7: Immutable Audit Logging**

- Week 1: Database schema + GraphQL types
- Weeks 2-3: Cryptographic chain
- Weeks 4-5: Event capture + interceptors
- Week 6: Chain verification API
- Week 7: Compliance reports + QA

**Weeks 8-13: Advanced RBAC**

- Week 8: RBAC schema + models
- Weeks 9-10: Role hierarchy engine
- Week 11: Permission resolution + caching
- Week 12: GraphQL integration + directives
- Week 13: RLS + management APIs

### Quarter 2: Compliance (Weeks 14-28)

**Weeks 14-23: GDPR Compliance Suite**

- Weeks 14-16: DSR workflow engine
- Weeks 17-18: Personal data discovery
- Weeks 19-20: Consent management
- Week 21: Right to erasure
- Week 22: Data portability
- Week 23: Compliance reporting

**Weeks 24-28: Data Classification**

- Week 24: Schema scanner
- Week 25: Pattern matching + classifiers
- Week 26: Auto-classification
- Week 27: Security integration
- Week 28: Reports + QA

---

## Testing Strategy

### Unit Tests

- Individual components tested in isolation
- Mock database interactions
- Test edge cases and error handling
- Target: >90% code coverage

### Integration Tests

- End-to-end workflows
- Real PostgreSQL database
- Multi-tenant scenarios
- Performance benchmarks

### Security Tests

- Penetration testing for RBAC bypass
- Cryptographic verification
- SQL injection prevention
- Data leak prevention

### Performance Tests

- 10,000+ concurrent users
- Permission cache hit rates
- Audit log write throughput
- Query performance under load

---

## Documentation Deliverables

### Developer Documentation

- API reference for each feature
- Integration guides
- Code examples
- Migration guides

### Administrator Documentation

- Configuration guides
- Operational procedures
- Troubleshooting guides
- Best practices

### Compliance Documentation

- SOX compliance guide
- HIPAA compliance guide
- GDPR compliance guide
- Audit trail verification

---

## Success Metrics

### Tier 1 Completion Criteria

**After 3 months (end of Quarter 1)**:

- [ ] SOX/HIPAA-compliant audit trails operational
- [ ] RBAC supporting 10,000+ users with <5ms permission checks
- [ ] All features have >90% test coverage
- [ ] Documentation complete
- [ ] Performance benchmarks met

**After 6 months (end of Quarter 2)**:

- [ ] Full GDPR compliance achieved
- [ ] Automated data classification running
- [ ] EU market certification ready
- [ ] Enterprise reference customers onboarded

### Technical Metrics

- Audit log write: <10ms per event
- Permission resolution (cached): <5ms
- DSR fulfillment: <30 days automated
- Data classification accuracy: >95%
- Zero security vulnerabilities in penetration tests

### Business Metrics

- Enterprise deals closed: 3+
- Regulated industry customers: 5+
- Compliance certifications obtained: SOC 2, ISO 27001
- Revenue impact: $500K+ ARR

---

*These implementation plans provide a complete roadmap for building FraiseQL's Tier 1 enterprise features using disciplined TDD methodology and phased development approach.*
