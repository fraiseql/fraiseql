<!-- Skip to main content -->
---

title: Database Federation: Multi-Database Queries
description: Complete guide to federating queries across multiple databases (PostgreSQL, MySQL, SQLite) as a unified GraphQL API.
keywords: ["workflow", "saas", "realtime", "ecommerce", "analytics", "federation"]
tags: ["documentation", "reference"]
---

# Database Federation: Multi-Database Queries

**Status:** ✅ Production Ready
**Complexity:** ⭐⭐⭐⭐⭐ (Expert)
**Audience:** Database architects, DevOps engineers, migration specialists
**Reading Time:** 25-30 minutes
**Last Updated:** 2026-02-05

Complete guide to federating queries across multiple databases (PostgreSQL, MySQL, SQLite) as a unified GraphQL API.

---

## Architecture Overview

**Diagram: Federation** - Multi-database query coordination

```d2
<!-- Code example in D2 Diagram -->
direction: down

GraphQL: "FraiseQL Server\n(Single GraphQL endpoint)" {
  shape: box
  style.fill: "#e3f2fd"
  style.border: "3px solid #1976d2"
}

Postgres: "PostgreSQL\n(Primary)" {
  shape: box
  style.fill: "#c8e6c9"
  style.border: "2px solid #388e3c"
}

MySQL: "MySQL\n(Historical Data)" {
  shape: box
  style.fill: "#bbdefb"
  style.border: "2px solid #1976d2"
}

SQLite: "SQLite\n(Cache/Sync)" {
  shape: box
  style.fill: "#f8bbd0"
  style.border: "2px solid #c2185b"
}

Details: "All queries federated to appropriate database\nCross-database joins in FraiseQL Rust layer\nResults merged and returned as single GraphQL response" {
  shape: box
  style.fill: "#fffde7"
}

GraphQL -> Postgres
GraphQL -> MySQL
GraphQL -> SQLite
Postgres -> Details
MySQL -> Details
SQLite -> Details
```text
<!-- Code example in TEXT -->

---

## Configuration

### FraiseQL TOML

```toml
<!-- Code example in TOML -->
# FraiseQL.toml
[[FraiseQL.databases]]
name = "postgres_primary"
engine = "postgresql"
host = "${DB_POSTGRES_HOST}"
port = 5432
database = "production"
username = "${DB_POSTGRES_USER}"
password = "${DB_POSTGRES_PASSWORD}"
pool_size = 10
timeout_secs = 30

[[FraiseQL.databases]]
name = "mysql_historical"
engine = "mysql"
host = "${DB_MYSQL_HOST}"
port = 3306
database = "historical"
username = "${DB_MYSQL_USER}"
password = "${DB_MYSQL_PASSWORD}"
pool_size = 5
timeout_secs = 30
read_only = true  # Historical data, no writes

[[FraiseQL.databases]]
name = "sqlite_cache"
engine = "sqlite"
path = "/var/cache/FraiseQL.db"
pool_size = 1
read_write = false  # Read-only cache
```text
<!-- Code example in TEXT -->

---

## Schema Definition

### Federated Schema

```python
<!-- Code example in Python -->
# federation_schema.py
from FraiseQL import types, database

@types.object
class Customer:
    """Primary customer data from PostgreSQL"""
    id: UUID  # UUID v4 for GraphQL ID
    email: str
    name: str
    created_at: str

    @database('postgres_primary')
    def details(self) -> 'CustomerDetails':
        """Detailed info from PostgreSQL"""
        pass

    @database('mysql_historical')
    def historical_orders(self, year: int) -> list['Order']:
        """Orders from historical MySQL database"""
        pass

@types.object
class CustomerDetails:
    customer_id: UUID  # UUID v4 for GraphQL ID
    phone: str
    address: str
    preferred_timezone: str

@types.object
class Order:
    id: UUID  # UUID v4 for GraphQL ID
    customer_id: UUID  # UUID v4 for GraphQL ID
    order_date: str
    total_amount: float

    # Can load items from different database
    @database('mysql_historical')
    def items(self) -> list['OrderItem']:
        """Order line items"""
        pass

@types.object
class OrderItem:
    order_id: UUID  # UUID v4 for GraphQL ID
    product_id: UUID  # UUID v4 for GraphQL ID
    quantity: int
    price: float

@types.object
class Query:
    @database('postgres_primary')
    def customer(self, id: str) -> Customer:
        """Get customer from primary database"""
        pass

    def customer_with_history(self, id: str) -> 'CustomerWithHistory':
        """Composite type from multiple databases"""
        pass

    @database('postgres_primary', fallback='sqlite_cache')
    def customers(self, limit: int = 50) -> list[Customer]:
        """Query primary, fallback to cache"""
        pass
```text
<!-- Code example in TEXT -->

---

## Cross-Database Queries

### Pattern 1: Fetch-Then-Join (Application Layer)

```typescript
<!-- Code example in TypeScript -->
// Join data from multiple databases at application level
const GET_CUSTOMER_WITH_HISTORY = gql`
  query GetCustomerWithHistory($customerId: ID!) {
    # From PostgreSQL
    customer(id: $customerId) {
      id
      email
      name
    }

    # From MySQL
    historicalOrders(customerId: $customerId, year: 2024) {
      id
      date
      total
    }
  }
`;

export async function getCustomerWithHistory(customerId: string) {
  const result = await client.query(GET_CUSTOMER_WITH_HISTORY, {
    variables: { customerId },
  });

  // FraiseQL automatically routes to correct databases
  const customer = result.data.customer;
  const orders = result.data.historicalOrders;

  return {
    customer,
    orders,
  };
}
```text
<!-- Code example in TEXT -->

### Pattern 2: Virtual Foreign Keys

```python
<!-- Code example in Python -->
# Define relationship across databases
@types.object
class Customer:
    id: UUID  # UUID v4 for GraphQL ID
    email: str

    @database('mysql_historical')
    def all_orders(self) -> list['Order']:
        """
        Automatically joins on customer_id
        Query: mysql_historical.orders WHERE customer_id = $1
        """
        pass

# Usage
query = """
  query {
    customer(id: "123") {
      id
      email
      allOrders {  # Automatic cross-database join
        id
        total
      }
    }
  }
"""
```text
<!-- Code example in TEXT -->

---

## Synchronization Patterns

### Pattern 1: Read-Through Cache

```python
<!-- Code example in Python -->
@database('postgres_primary', fallback='sqlite_cache')
def get_customer(self, id: str) -> Customer:
    """
    1. Try PostgreSQL first
    2. If fails, fallback to SQLite cache
    3. Return whichever succeeds
    """
    pass
```text
<!-- Code example in TEXT -->

### Pattern 2: Write-Through Cache

```sql
<!-- Code example in SQL -->
-- When writing to primary, also write to cache
CREATE OR REPLACE FUNCTION sync_to_cache() RETURNS TRIGGER AS $$
BEGIN
  -- Insert/update to SQLite cache
  INSERT INTO cache.customers (id, email, name, updated_at)
  VALUES (NEW.id, NEW.email, NEW.name, NOW())
  ON CONFLICT(id) DO UPDATE SET
    email = NEW.email,
    name = NEW.name,
    updated_at = NOW();

  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER postgres_to_cache
AFTER INSERT OR UPDATE ON customers
FOR EACH ROW
EXECUTE FUNCTION sync_to_cache();
```text
<!-- Code example in TEXT -->

### Pattern 3: Background Sync

```python
<!-- Code example in Python -->
# Periodic sync from PostgreSQL to MySQL
async def sync_orders_to_historical():
    """Run daily to move completed orders to historical database"""

    # 1. Query PostgreSQL for completed orders
    pg_orders = await pg_client.query("""
        SELECT * FROM orders
        WHERE created_at < CURRENT_DATE - INTERVAL '30 days'
        AND status = 'completed'
    """)

    # 2. Insert into MySQL
    for order in pg_orders:
        await mysql_client.query("""
            INSERT INTO orders (id, customer_id, total, date)
            VALUES (?, ?, ?, ?)
        """, (order.id, order.customer_id, order.total, order.created_at))

    # 3. Optionally delete from PostgreSQL (archive strategy)
    # OR keep both for redundancy

# Schedule with APScheduler
scheduler.add_job(
    sync_orders_to_historical,
    'cron',
    hour=2,  # 2 AM daily
    minute=0
)
```text
<!-- Code example in TEXT -->

---

## Transaction Coordination

### Two-Phase Commit Pattern

```python
<!-- Code example in Python -->
async def transfer_customer_data(customer_id: str):
    """Transfer customer from PostgreSQL to MySQL"""

    # Phase 1: Prepare
    pg_tx = await pg_pool.acquire()
    mysql_tx = await mysql_pool.acquire()

    try:
        # Start transactions
        await pg_tx.execute('BEGIN')
        await mysql_tx.execute('BEGIN')

        # Read from PostgreSQL
        customer = await pg_tx.fetchrow(
            'SELECT * FROM customers WHERE id = $1',
            customer_id
        )

        # Write to MySQL
        await mysql_tx.execute("""
            INSERT INTO customers (id, email, name, created_at)
            VALUES (%s, %s, %s, %s)
        """, (customer['id'], customer['email'], customer['name'], customer['created_at']))

        # Phase 2: Commit both
        await pg_tx.execute('COMMIT')
        await mysql_tx.execute('COMMIT')

        return {'status': 'success'}

    except Exception as e:
        # Rollback both on any error
        await pg_tx.execute('ROLLBACK')
        await mysql_tx.execute('ROLLBACK')
        raise e
    finally:
        await pg_pool.release(pg_tx)
        await mysql_pool.release(mysql_tx)
```text
<!-- Code example in TEXT -->

---

## Handling Inconsistencies

### Eventual Consistency

```python
<!-- Code example in Python -->
# Mark data with source and timestamp
@types.object
class Customer:
    id: UUID  # UUID v4 for GraphQL ID
    email: str
    source_database: str  # postgres_primary, mysql_historical
    last_sync: datetime
    is_stale: bool  # True if > 1 hour since sync
```text
<!-- Code example in TEXT -->

### Reconciliation Queries

```sql
<!-- Code example in SQL -->
-- Find customers in PostgreSQL but not in MySQL
SELECT p.id
FROM postgres_primary.customers p
LEFT JOIN mysql_historical.customers m ON p.id = m.id
WHERE m.id IS NULL;

-- Find customers with different data
SELECT
  p.id,
  p.email as pg_email,
  m.email as mysql_email
FROM postgres_primary.customers p
JOIN mysql_historical.customers m ON p.id = m.id
WHERE p.email != m.email;
```text
<!-- Code example in TEXT -->

---

## Performance Optimization

### Database Routing Hints

```python
<!-- Code example in Python -->
@database('postgres_primary', affinity='read_primary')
def get_current_user(self, id: str) -> User:
    """Always hit primary for current data"""
    pass

@database('mysql_historical', affinity='read_replica')
def get_historical_orders(self, id: str) -> list[Order]:
    """Can use read replicas"""
    pass

@database('sqlite_cache', affinity='cache')
def search_products(self, term: str) -> list[Product]:
    """Precomputed cache, fastest"""
    pass
```text
<!-- Code example in TEXT -->

### Query Optimization

```python
<!-- Code example in Python -->
# Avoid N+1 queries across databases
@database.batch
def get_customers_with_orders(self, customer_ids: list[str]):
    """
    Instead of:
      - Query PostgreSQL for each customer
      - Query MySQL for each customer's orders

    This does:
      - Single query: SELECT * FROM postgres.customers WHERE id IN (...)
      - Single query: SELECT * FROM mysql.orders WHERE customer_id IN (...)
      - Join in application layer
    """
    pass
```text
<!-- Code example in TEXT -->

---

## Testing Federation

```typescript
<!-- Code example in TypeScript -->
describe('Database Federation', () => {
  it('should query across databases', async () => {
    // Create customer in PostgreSQL
    await createCustomer({ id: '1', email: 'test@example.com' });

    // Create order in MySQL
    await createOrder({ id: '100', customer_id: '1', total: 99.99 });

    // Federated query
    const result = await client.query(GET_CUSTOMER_WITH_HISTORY, {
      variables: { customerId: '1' }
    });

    expect(result.data.customer.id).toBe('1');
    expect(result.data.historicalOrders[0].id).toBe('100');
  });

  it('should fallback to cache on primary failure', async () => {
    // Cache has data
    await cache.insert('customers', { id: '1', name: 'Cached' });

    // Primary down
    await shutdownDatabase('postgres_primary');

    const result = await client.query(GET_CUSTOMER, {
      variables: { customerId: '1' }
    });

    // Should return from cache
    expect(result.data.customer.name).toBe('Cached');
  });

  it('should keep databases in sync', async () => {
    const customer = { id: '1', email: 'test@example.com' };
    await updateCustomer(customer);

    // Give sync time
    await sleep(100);

    // Verify both databases updated
    const pgData = await pg.query('SELECT * FROM customers WHERE id = $1', ['1']);
    const cacheData = await sqlite.query('SELECT * FROM customers WHERE id = ?', ['1']);

    expect(pgData[0].email).toBe('test@example.com');
    expect(cacheData[0].email).toBe('test@example.com');
  });
});
```text
<!-- Code example in TEXT -->

---

## Migration Scenario: Legacy System Integration

### Setup

- **Old System**: MySQL database with 10 years of data
- **New System**: PostgreSQL for current operations
- **Goal**: Single GraphQL API querying both

### Implementation

```python
<!-- Code example in Python -->
# 1. Define federation
config = {
    'postgres_new': {
        'engine': 'postgresql',
        'host': 'new-db.internal',
        'databases': ['current_data'],
    },
    'mysql_legacy': {
        'engine': 'mysql',
        'host': 'legacy-db.internal',
        'databases': ['old_data'],
        'read_only': True,
    }
}

# 2. Create unified schema
@types.object
class Order:
    id: UUID  # UUID v4 for GraphQL ID

    @database('postgres_new')
    def customer(self) -> 'Customer':
        """New customer data"""
        pass

    @database('mysql_legacy')
    def legacy_details(self) -> dict:
        """Legacy order details"""
        pass

# 3. Gradual migration
@database('postgres_new', fallback='mysql_legacy')
def get_order(self, id: str) -> Order:
    """
    Try new database first (faster)
    Fall back to legacy if not found
    Allows gradual migration of data
    """
    pass

# 4. Monitor discrepancies
SELECT
  id,
  created_at,
  CASE
    WHEN EXISTS (SELECT 1 FROM postgres_new.orders WHERE id = id) THEN 'migrated'
    WHEN EXISTS (SELECT 1 FROM mysql_legacy.orders WHERE id = id) THEN 'legacy_only'
    ELSE 'missing'
  END as status
FROM mysql_legacy.orders;
```text
<!-- Code example in TEXT -->

---

## Monitoring & Debugging

### Health Check

```graphql
<!-- Code example in GraphQL -->
query FederationHealth {
  databaseStatus {
    name
    connected
    latency_ms
    last_check
  }
}
```text
<!-- Code example in TEXT -->

### Query Tracing

```python
<!-- Code example in Python -->
# Enable query tracing to see which database each query goes to
result = await client.query(
    query,
    trace=True  # Includes database routing info
)

print(result.meta.trace)
# Output: {
#   'query': 'customer(id: "1")',
#   'routes': [
#     {'query': 'SELECT * FROM customers WHERE id = $1', 'database': 'postgres_primary', 'time_ms': 15},
#     {'query': 'SELECT * FROM orders WHERE customer_id = $1', 'database': 'mysql_historical', 'time_ms': 120}
#   ]
# }
```text
<!-- Code example in TEXT -->

---

## See Also

**Related Patterns:**

- [Multi-Tenant SaaS](./saas-multi-tenant.md) - Single database per tenant federation
- [Analytics Platform](./analytics-olap-platform.md) - Read-only historical data

**Migration:**

- [Database Migration Guide](../guides/database-migration-guide.md)
- [Legacy System Integration](../guides/database-migration-guide.md)

**Deployment:**

- [Production Deployment](../guides/production-deployment.md)
- [Connection Pooling](../guides/production-deployment.md)

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
