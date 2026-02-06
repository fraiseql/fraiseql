<!-- Skip to main content -->
---

title: Analytics Platform with OLAP
description: Complete guide to building a scalable analytics and business intelligence platform using FraiseQL with OLAP (Online Analytical Processing) patterns.
keywords: ["workflow", "saas", "realtime", "ecommerce", "analytics", "federation"]
tags: ["documentation", "reference"]
---

# Analytics Platform with OLAP

**Status:** ✅ Production Ready
**Complexity:** ⭐⭐⭐⭐ (Advanced)
**Audience:** Data engineers, analytics architects, BI developers
**Reading Time:** 30-35 minutes
**Last Updated:** 2026-02-05

Complete guide to building a scalable analytics and business intelligence platform using FraiseQL with OLAP (Online Analytical Processing) patterns.

---

## Architecture Overview

**Diagram: Analytics Pattern** - Denormalized fact table design with JSONB dimensions

```d2
<!-- Code example in D2 Diagram -->
direction: down

Sources: "Data Sources" {
  shape: box
  style.fill: "#e3f2fd"
  children: [
    AppDB: "Application\nDatabases"
    APIs: "APIs"
    ThirdParty: "Third-Party\nServices"
  ]
}

Warehouse: "Data Warehouse (PostgreSQL)" {
  shape: box
  style.fill: "#f3e5f5"
  children: [
    Fact: "Fact Tables\n(Events - billions of rows)"
    Dim: "Dimension Tables\n(Reference data)"
    Agg: "Aggregate Tables\n(Pre-computed)"
  ]
}

Server: "FraiseQL Server (OLAP Mode)" {
  shape: box
  style.fill: "#fff3e0"
  children: [
    Exec: "Executes analytical queries"
    Scale: "Handles aggregations at scale"
    Export: "Supports Arrow Flight for bulk exports"
  ]
}

Consumers: "Data Consumers" {
  shape: box
  style.fill: "#c8e6c9"
  children: [
    Dashboard: "📊 Dashboards\n(React)"
    Reports: "📄 Reports\n(PDF)"
    Exports: "📥 Exports\n(Excel)"
    RealTime: "🔴 Real-time\n(WebSocket)"
  ]
}

Sources -> Warehouse: "ETL/ELT"
Warehouse -> Server: "GraphQL OLAP queries"
Server -> Consumers: "Results"
```text
<!-- Code example in TEXT -->

---

## Schema Design: FraiseQL Fact Tables (Denormalized OLAP)

FraiseQL uses a **denormalized fact table pattern** optimized for fast analytics without expensive joins:

**Diagram: Analytics Pattern** - Denormalized fact table design with JSONB dimensions

```d2
<!-- Code example in D2 Diagram -->
direction: right

FactTable: "tf_events\n(Fact Table)" {
  shape: box
  style.fill: "#fff59d"
  style.border: "3px solid #f57f17"
  children: [
    Measures: "Measures (SQL columns)\nrevenue, quantity, sessions"
    Dimensions: "Dimensions (JSONB)\nuser, product, category, region"
    Filters: "Filters (Indexed)\nuser_id, occurred_at"
  ]
}

Benefits: "✅ No expensive joins\n✅ Flexible dimensions\n✅ Fast GROUP BY" {
  shape: box
  style.fill: "#c8e6c9"
}

FactTable -> Benefits
```text
<!-- Code example in TEXT -->

**Why FraiseQL's Pattern?**

| Aspect | Traditional Star Schema | FraiseQL Fact Tables |
|--------|----------------------|-------------------|
| **Structure** | Fact table + multiple dimension tables | Single denormalized fact table |
| **Joins** | Multiple expensive joins per query | No joins needed |
| **Dimensions** | Fixed schema columns | JSONB with flexible schema |
| **Query Speed** | Slower (N joins) | Faster (no joins) |
| **Schema Flexibility** | Hard to add dimensions | Easy (JSONB expansion) |

---

### Fact Tables (tf_events)

FraiseQL fact tables follow a three-part structure:

```sql
<!-- Code example in SQL -->
-- Fact table with measures, JSONB dimensions, and indexed filters
CREATE TABLE tf_events (
  -- Row identifier
  event_id BIGSERIAL PRIMARY KEY,

  -- MEASURES: Numeric columns for fast aggregation
  revenue DECIMAL(12, 2),
  quantity INT,
  cost DECIMAL(12, 2),
  sessions INT,

  -- DIMENSIONS: JSONB for flexible GROUP BY
  -- Contains: user_category, product, region, country, utm_source, etc.
  data JSONB NOT NULL,

  -- FILTERS: Indexed denormalized columns for fast WHERE
  user_id UUID NOT NULL,
  occurred_at TIMESTAMP NOT NULL,

  -- Temporal partitioning
  event_date DATE NOT NULL,  -- Partition key

  -- Audit columns
  created_at TIMESTAMP DEFAULT NOW(),

  -- Indexes for query performance
  INDEX idx_event_date (event_date),
  INDEX idx_user_id (user_id),
  INDEX idx_occurred_at (occurred_at),
  INDEX idx_data GIN (data)
) PARTITION BY RANGE (event_date);

-- Create monthly partitions
CREATE TABLE tf_events_2024_01 PARTITION OF tf_events
  FOR VALUES FROM ('2024-01-01') TO ('2024-02-01');
CREATE TABLE tf_events_2024_02 PARTITION OF tf_events
  FOR VALUES FROM ('2024-02-01') TO ('2024-03-01');
-- ... continue for each month
```text
<!-- Code example in TEXT -->

**Key components:**

1. **Measures** (numeric columns):
   - `revenue`, `quantity`, `cost`, `sessions`
   - Used in `SUM()`, `AVG()`, `COUNT()` aggregations
   - Keep these as direct SQL columns for performance

2. **Dimensions** (JSONB column):
   - Single `data` JSONB column containing flexible schema
   - Paths like `data->>'user_category'`, `data->>'product'`, `data->>'region'`
   - Easy to add new dimensions without schema migration
   - Query with operators: `->>` (text), `->` (JSON), `@>` (contains)

3. **Filters** (indexed columns):
   - `user_id`, `occurred_at` - denormalized for fast WHERE
   - Must be indexed: `INDEX idx_user_id (user_id)`
   - Avoid searching these values in JSONB (slow)

4. **Temporal structure**:
   - Partition by `event_date` (monthly or daily)
   - Improves query speed for time-range filters
   - Enables archival of old partitions

### Aggregate Tables (Pre-Computed)

```sql
<!-- Code example in SQL -->
-- Daily aggregates (updated nightly)
CREATE TABLE agg_daily_metrics (
  date DATE NOT NULL,
  product_id UUID NOT NULL,
  country VARCHAR(2) NOT NULL,
  metric_name VARCHAR(50) NOT NULL,
  metric_value DECIMAL(15, 2),
  event_count INT,
  unique_users INT,
  PRIMARY KEY (date, product_id, country, metric_name),

  INDEX idx_date (date),
  INDEX idx_product_id (product_id),
  INDEX idx_country (country)
);

-- Hourly rolling aggregates (updated every hour)
CREATE TABLE agg_hourly_metrics (
  hour_timestamp TIMESTAMP NOT NULL,
  product_id UUID NOT NULL,
  source VARCHAR(100),
  event_count INT,
  revenue DECIMAL(12, 2),
  unique_users INT,
  PRIMARY KEY (hour_timestamp, product_id, source),

  INDEX idx_timestamp (hour_timestamp),
  INDEX idx_product_id (product_id)
);

-- Cohort Analysis Table
CREATE TABLE agg_cohorts (
  cohort_date DATE NOT NULL,
  days_since_signup INT NOT NULL,
  cohort_size INT,
  retention_rate DECIMAL(5, 2),
  revenue DECIMAL(12, 2),
  PRIMARY KEY (cohort_date, days_since_signup)
);
```text
<!-- Code example in TEXT -->

---

## FraiseQL OLAP Schema

FraiseQL uses a **fact table pattern** with measures (SQL columns) and dimensions (JSONB):

```python
<!-- Code example in Python -->
# analytics_schema.py
from FraiseQL import types
from decimal import Decimal
from datetime import datetime, date

@types.fact_table(
    table_name="tf_events",
    measures=["revenue", "quantity", "sessions"],
    dimension_column="data",
    dimension_paths=[
        {"name": "product_id", "json_path": "data->>'product_id'", "data_type": "text"},
        {"name": "category", "json_path": "data->>'category'", "data_type": "text"},
        {"name": "region", "json_path": "data->>'region'", "data_type": "text"},
        {"name": "source", "json_path": "data->>'source'", "data_type": "text"},
        {"name": "device_type", "json_path": "data->>'device_type'", "data_type": "text"},
    ]
)
@types.object
class Event:
    """Event fact table - denormalized for fast analytics"""
    event_id: UUID  # UUID v4 for GraphQL ID
    event_date: date
    event_timestamp: datetime

    # MEASURES: Numeric columns for fast SUM/AVG/COUNT aggregation
    revenue: Decimal | None              # NULL for non-purchase events
    quantity: int | None                 # NULL for non-sale events
    sessions: int | None                 # Session count

    # DIMENSIONS: JSONB for flexible GROUP BY
    # Includes: product_id, category, region, source, device_type
    dimensions: dict                     # data JSONB column

    # FILTERS: Indexed denormalized columns for fast WHERE clauses
    user_id: str                         # Indexed for fast filtering
    occurred_at: datetime                # Indexed for time-range queries
```text
<!-- Code example in TEXT -->

**Schema mapping:**

- `revenue`, `quantity`, `sessions` → **Measures** (direct columns, fast aggregation)
- `dimensions` dict (JSONB column `data`) → **Dimensions** (flexible, no joins needed)
- `user_id`, `occurred_at` → **Filters** (indexed for WHERE performance)

```python
<!-- Code example in Python -->
@types.object
class MetricResult:
    """Metric aggregation result"""
    dimension_value: str
    event_count: int
    revenue: Decimal
    unique_users: int
    conversion_rate: Decimal

@types.object
class CohortResult:
    """Cohort analysis result"""
    cohort_date: date
    days_since_signup: int
    cohort_size: int
    retention_rate: Decimal
    revenue: Decimal

@types.object
class Query:
    # Time-series metrics
    def daily_revenue(
        self,
        start_date: date,
        end_date: date,
        product_id: str | None = None,
        country: str | None = None
    ) -> list[dict]:
        """Daily revenue over time period"""
        pass

    def hourly_events(
        self,
        date: date,
        event_type: str | None = None
    ) -> list[dict]:
        """Hourly event breakdown for a day"""
        pass

    # Segmentation queries
    def revenue_by_product(
        self,
        start_date: date,
        end_date: date,
        limit: int = 50
    ) -> list[MetricResult]:
        """Revenue segmented by product"""
        pass

    def conversion_by_source(
        self,
        start_date: date,
        end_date: date
    ) -> list[MetricResult]:
        """Conversion rate by traffic source"""
        pass

    def customer_lifetime_value_distribution(
        self,
        country: str | None = None
    ) -> list[dict]:
        """Distribution of customer lifetime values"""
        pass

    # Cohort analysis
    def retention_cohort(
        self,
        cohort_start: date,
        cohort_end: date,
        max_days: int = 90
    ) -> list[CohortResult]:
        """User retention by signup cohort"""
        pass

    # Funnel analysis
    def conversion_funnel(
        self,
        start_date: date,
        end_date: date,
        steps: list[str]  # ['view', 'add_to_cart', 'purchase']
    ) -> list[dict]:
        """Funnel conversion analysis"""
        pass

    # Real-time metrics
    def realtime_metrics(self) -> dict:
        """Current real-time metrics (last 1 hour)"""
        pass

    # Drill-down queries
    def product_details(
        self,
        product_id: str,
        start_date: date,
        end_date: date
    ) -> dict:
        """Deep-dive into single product performance"""
        pass
```text
<!-- Code example in TEXT -->

---

## Complex OLAP Queries

### Daily Revenue Trend

```graphql
<!-- Code example in GraphQL -->
query DailyRevenue($startDate: Date!, $endDate: Date!, $productId: ID) {
  dailyRevenue(startDate: $startDate, endDate: $endDate, productId: $productId) {
    date
    revenue
    orders
    average_order_value
    unique_customers
    returning_customer_percentage
  }
}
```text
<!-- Code example in TEXT -->

**Corresponding SQL (generated by FraiseQL):**

```sql
<!-- Code example in SQL -->
SELECT
  event_date as date,
  SUM(revenue) as revenue,
  COUNT(DISTINCT order_id) as orders,
  AVG(revenue) as average_order_value,
  COUNT(DISTINCT user_id) as unique_customers,
  ROUND(
    SUM(CASE WHEN is_returning THEN 1 ELSE 0 END)::NUMERIC / COUNT(DISTINCT user_id) * 100,
    2
  ) as returning_customer_percentage
FROM events
WHERE event_date BETWEEN $1 AND $2
  AND event_type = 'purchase'
  AND (product_id = $3 OR $3 IS NULL)
GROUP BY event_date
ORDER BY event_date DESC;
```text
<!-- Code example in TEXT -->

### Cohort Retention Analysis

```graphql
<!-- Code example in GraphQL -->
query RetentionCohort($cohortStart: Date!, $cohortEnd: Date!, $maxDays: Int!) {
  retentionCohort(cohortStart: $cohortStart, cohortEnd: $cohortEnd, maxDays: $maxDays) {
    cohortDate
    daysSinceSilgnup
    cohortSize
    retentionRate
    revenue
  }
}
```text
<!-- Code example in TEXT -->

**SQL (complex multi-CTE query):**

```sql
<!-- Code example in SQL -->
WITH cohort_users AS (
  SELECT
    DATE_TRUNC('month', signup_date)::DATE as cohort_date,
    user_id
  FROM dim_users
  WHERE signup_date BETWEEN $1 AND $2
),
user_activities AS (
  SELECT
    cu.cohort_date,
    cu.user_id,
    EXTRACT(DAY FROM e.event_date - cu.cohort_date)::INT as days_since_signup,
    SUM(e.revenue) as daily_revenue
  FROM cohort_users cu
  JOIN events e ON cu.user_id = e.user_id
  WHERE e.event_date >= cu.cohort_date
  GROUP BY cu.cohort_date, cu.user_id, EXTRACT(DAY FROM e.event_date - cu.cohort_date)
)
SELECT
  cohort_date,
  days_since_signup,
  COUNT(DISTINCT CASE WHEN days_since_signup = 0 THEN user_id END) as cohort_size,
  ROUND(
    COUNT(DISTINCT user_id)::NUMERIC /
    COUNT(DISTINCT CASE WHEN days_since_signup = 0 THEN user_id END) * 100,
    2
  ) as retention_rate,
  COALESCE(SUM(daily_revenue), 0) as revenue
FROM user_activities
WHERE days_since_signup BETWEEN 0 AND $3
GROUP BY cohort_date, days_since_signup
ORDER BY cohort_date, days_since_signup;
```text
<!-- Code example in TEXT -->

### Funnel Conversion Analysis

```graphql
<!-- Code example in GraphQL -->
query ConversionFunnel(
  $startDate: Date!
  $endDate: Date!
  $steps: [String!]!
) {
  conversionFunnel(startDate: $startDate, endDate: $endDate, steps: $steps) {
    step
    users
    dropoff
    conversionRate
  }
}
```text
<!-- Code example in TEXT -->

---

## Performance Optimization Strategies

### 1. Pre-Computed Aggregates

```sql
<!-- Code example in SQL -->
-- Refresh aggregates nightly
CREATE OR REPLACE FUNCTION refresh_daily_aggregates()
RETURNS void AS $$
BEGIN
  DELETE FROM agg_daily_metrics
  WHERE date >= CURRENT_DATE - INTERVAL '1 day';

  INSERT INTO agg_daily_metrics
  SELECT
    event_date,
    product_id,
    country,
    'revenue' as metric_name,
    SUM(revenue) as metric_value,
    COUNT(*) as event_count,
    COUNT(DISTINCT user_id) as unique_users
  FROM events
  WHERE event_date >= CURRENT_DATE - INTERVAL '30 days'
  GROUP BY event_date, product_id, country
  UNION ALL
  SELECT
    event_date,
    product_id,
    country,
    'events' as metric_name,
    COUNT(*),
    COUNT(*),
    COUNT(DISTINCT user_id)
  FROM events
  WHERE event_date >= CURRENT_DATE - INTERVAL '30 days'
  GROUP BY event_date, product_id, country;
END;
$$ LANGUAGE plpgsql;

-- Schedule with cron (pg_cron extension)
SELECT cron.schedule('refresh_daily_aggregates', '0 2 * * *', 'SELECT refresh_daily_aggregates()');
```text
<!-- Code example in TEXT -->

### 2. Partition Pruning

Queries are automatically optimized by date:

```sql
<!-- Code example in SQL -->
-- Only scans relevant partitions
SELECT * FROM events
WHERE event_date BETWEEN '2024-06-01' AND '2024-06-30';
-- ↑ Only queries events_2024_06 partition
```text
<!-- Code example in TEXT -->

### 3. Materialized Views

```sql
<!-- Code example in SQL -->
-- Fast views for common queries
CREATE MATERIALIZED VIEW mv_top_products_by_revenue AS
SELECT
  p.product_id,
  p.product_name,
  SUM(e.revenue) as total_revenue,
  COUNT(DISTINCT e.user_id) as unique_customers,
  COUNT(*) as event_count
FROM events e
JOIN dim_products p ON e.product_id = p.product_id
WHERE e.event_date >= CURRENT_DATE - INTERVAL '30 days'
GROUP BY p.product_id, p.product_name
ORDER BY total_revenue DESC
LIMIT 100;

-- Refresh hourly
SELECT cron.schedule('refresh_mv_top_products', '0 * * * *', 'REFRESH MATERIALIZED VIEW CONCURRENTLY mv_top_products_by_revenue');

CREATE INDEX idx_mv_top_products_revenue ON mv_top_products_by_revenue(total_revenue DESC);
```text
<!-- Code example in TEXT -->

### 4. Columnar Storage (Citus/Timescale Extension)

For very large fact tables:

```sql
<!-- Code example in SQL -->
-- Create hypertable (TimescaleDB)
CREATE TABLE events (
  event_timestamp TIMESTAMP NOT NULL,
  user_id UUID,
  event_type VARCHAR(50),
  revenue DECIMAL(12, 2)
) WITH (timescaledb.compress);

-- Compress old data automatically
SELECT add_compression_policy('events', INTERVAL '7 days');
```text
<!-- Code example in TEXT -->

---

## Real-Time Subscriptions

### Live Metrics Dashboard

```graphql
<!-- Code example in GraphQL -->
subscription RealtimeMetrics {
  realtimeMetrics {
    timestamp
    events_per_minute
    revenue_per_minute
    active_users
    top_event_type
    top_country
  }
}
```text
<!-- Code example in TEXT -->

**Implementation:**

```python
<!-- Code example in Python -->
@types.subscription
class Subscription:
    def realtime_metrics(self) -> dict:
        """Stream updates every 10 seconds"""
        # Queries the last 1-hour aggregates
        # Emits when new data arrives
        pass
```text
<!-- Code example in TEXT -->

---

## Export Capabilities

### CSV Export

```graphql
<!-- Code example in GraphQL -->
query ExportDailyRevenue($startDate: Date!, $endDate: Date!) {
  exportDailyRevenue(startDate: $startDate, endDate: $endDate, format: CSV) {
    url  # Pre-signed S3 URL
    size_bytes
    created_at
  }
}
```text
<!-- Code example in TEXT -->

### Arrow Flight Export (Bulk Data)

```graphql
<!-- Code example in GraphQL -->
query ExportArrowFlight($startDate: Date!, $endDate: Date!) {
  exportArrowFlight(startDate: $startDate, endDate: $endDate) {
    arrow_endpoint  # Arrow Flight server endpoint
    query_id
    row_count
  }
}
```text
<!-- Code example in TEXT -->

**Client Usage:**

```python
<!-- Code example in Python -->
import pyarrow.flight as flight
import pandas as pd

# Connect to Arrow Flight server
client = flight.connect(('localhost', 5005))

# Fetch large dataset
flight_descriptor = flight.FlightDescriptor.for_command(
    query_id.encode('utf-8')
)
reader = client.do_get(flight_descriptor)

# Read into pandas
table = reader.read_all()
df = table.to_pandas()

# Write to Parquet
df.to_parquet('export.parquet')
```text
<!-- Code example in TEXT -->

---

## Dashboard Implementation (React)

```typescript
<!-- Code example in TypeScript -->
import { useQuery, gql } from '@apollo/client';
import { LineChart, PieChart } from 'recharts';

const DAILY_REVENUE = gql`
  query DailyRevenue($startDate: Date!, $endDate: Date!) {
    dailyRevenue(startDate: $startDate, endDate: $endDate) {
      date
      revenue
      orders
    }
  }
`;

export function AnalyticsDashboard() {
  const [dateRange, setDateRange] = useState({
    start: subDays(new Date(), 30),
    end: new Date(),
  });

  const { data, loading } = useQuery(DAILY_REVENUE, {
    variables: {
      startDate: format(dateRange.start, 'yyyy-MM-dd'),
      endDate: format(dateRange.end, 'yyyy-MM-dd'),
    },
    fetchPolicy: 'cache-and-network',
  });

  if (loading) return <div>Loading...</div>;

  return (
    <div className="dashboard">
      <DateRangePicker onChange={setDateRange} />
      <LineChart data={data?.dailyRevenue}>
        <XAxis dataKey="date" />
        <YAxis />
        <Line type="monotone" dataKey="revenue" />
      </LineChart>
      <KPICards data={data} />
    </div>
  );
}
```text
<!-- Code example in TEXT -->

---

## Testing Analytical Queries

```typescript
<!-- Code example in TypeScript -->
describe('Analytical Queries', () => {
  it('should calculate daily revenue', async () => {
    const result = await client.query(DAILY_REVENUE, {
      variables: {
        startDate: '2024-01-01',
        endDate: '2024-01-31',
      },
    });

    expect(result.data.dailyRevenue).toHaveLength(31);
    expect(result.data.dailyRevenue[0]).toHaveProperty('revenue');
    expect(result.data.dailyRevenue[0].revenue).toBeGreaterThan(0);
  });

  it('should handle date filtering', async () => {
    const result = await client.query(DAILY_REVENUE, {
      variables: {
        startDate: '2024-06-15',
        endDate: '2024-06-20',
      },
    });

    expect(result.data.dailyRevenue.length).toBeLessThanOrEqual(6);
    result.data.dailyRevenue.forEach((day: any) => {
      expect(day.date).toBeGreaterThanOrEqual('2024-06-15');
      expect(day.date).toBeLessThanOrEqual('2024-06-20');
    });
  });

  it('should calculate cohort retention correctly', async () => {
    const result = await client.query(RETENTION_COHORT, {
      variables: {
        cohortStart: '2024-01-01',
        cohortEnd: '2024-02-01',
        maxDays: 30,
      },
    });

    // Retention rate should decrease over time
    const rates = result.data.retentionCohort.map(c => c.retention_rate);
    for (let i = 1; i < rates.length; i++) {
      expect(rates[i]).toBeLessThanOrEqual(rates[i - 1]);
    }
  });
});
```text
<!-- Code example in TEXT -->

---

## Monitoring Analytical Performance

```sql
<!-- Code example in SQL -->
-- Track slow analytical queries
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

SELECT
  query,
  calls,
  mean_exec_time,
  max_exec_time
FROM pg_stat_statements
WHERE query LIKE '%events%'
ORDER BY mean_exec_time DESC
LIMIT 20;
```text
<!-- Code example in TEXT -->

---

## See Also

**Related Patterns:**

- [Multi-Tenant SaaS](./saas-multi-tenant.md) - Per-tenant analytics
- [IoT Time-Series](./iot-timeseries.md) - Specialized time-series

**Performance Guides:**

- [Performance Optimization](../guides/performance-optimization.md)
- [Query Optimization](../guides/schema-design-best-practices.md)

**Deployment:**

- [Production Deployment](../guides/production-deployment.md)
- [Scaling Guidelines](../guides/monitoring.md)

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
