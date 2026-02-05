# Analytics Platform with OLAP

**Status:** ✅ Production Ready
**Complexity:** ⭐⭐⭐⭐ (Advanced)
**Audience:** Data engineers, analytics architects, BI developers
**Reading Time:** 30-35 minutes
**Last Updated:** 2026-02-05

Complete guide to building a scalable analytics and business intelligence platform using FraiseQL with OLAP (Online Analytical Processing) patterns.

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│                 Data Sources                              │
│  (Application databases, APIs, third-party services)     │
└──────────────────────┬─────────────────────────────────┘
                       │
                       ↓ (ETL/ELT pipeline)
┌──────────────────────────────────────────────────────────┐
│            Data Warehouse (PostgreSQL)                    │
│  ┌──────────────┐    ┌──────────────┐  ┌──────────────┐ │
│  │ Fact Tables  │    │ Dimension    │  │ Aggregate    │ │
│  │ (Events)     │    │ Tables       │  │ Tables       │ │
│  │ (billions)   │    │ (slow-change)│  │ (pre-computed)│ │
│  └──────────────┘    └──────────────┘  └──────────────┘ │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ↓ (GraphQL OLAP queries)
┌──────────────────────────────────────────────────────────┐
│              FraiseQL Server (OLAP mode)                  │
│  - Executes analytical queries                            │
│  - Handles aggregations at scale                          │
│  - Supports arrow-flight for bulk exports                │
└──────────────────────┬──────────────────────────────────┘
                       │
            ┌──────────┼──────────┬──────────┐
            ↓          ↓          ↓          ↓
        Dashboard   Reports   Exports   Real-time
        (React)     (PDF)      (Excel)   (WebSocket)
```

---

## Schema Design: Star Schema

### Fact Tables (Events)

```sql
-- Events (fact table - billions of rows possible)
CREATE TABLE events (
  event_id BIGSERIAL PRIMARY KEY,
  event_date DATE NOT NULL,  -- Partitioned by date
  event_timestamp TIMESTAMP NOT NULL,
  user_id UUID NOT NULL,
  product_id UUID NOT NULL,
  order_id UUID,
  event_type VARCHAR(50) NOT NULL,  -- view, click, purchase, etc.
  event_properties JSONB,  -- Flexible schema for custom properties
  revenue DECIMAL(12, 2),  -- NULL for non-purchase events
  quantity INT,
  session_id VARCHAR(100),
  device_type VARCHAR(50),
  country VARCHAR(2),
  source VARCHAR(100),  -- utm_source, referrer, etc.

  INDEX idx_event_date (event_date),
  INDEX idx_event_type (event_type),
  INDEX idx_user_id (user_id),
  INDEX idx_product_id (product_id),
  INDEX idx_timestamp (event_timestamp)
) PARTITION BY RANGE (event_date);

-- Create partitions (monthly)
CREATE TABLE events_2024_01 PARTITION OF events
  FOR VALUES FROM ('2024-01-01') TO ('2024-02-01');
CREATE TABLE events_2024_02 PARTITION OF events
  FOR VALUES FROM ('2024-02-01') TO ('2024-03-01');
-- ... etc
```

### Dimension Tables (Reference Data)

```sql
-- Users Dimension (slowly changing dimension - Type 2)
CREATE TABLE dim_users (
  user_key BIGSERIAL PRIMARY KEY,
  user_id UUID NOT NULL,
  email VARCHAR(255),
  full_name VARCHAR(255),
  country VARCHAR(2),
  signup_date DATE,
  lifetime_value DECIMAL(12, 2),
  customer_segment VARCHAR(50),  -- vip, regular, at_risk
  last_activity_date DATE,
  valid_from TIMESTAMP DEFAULT NOW(),
  valid_to TIMESTAMP DEFAULT '9999-12-31'::TIMESTAMP,
  is_current BOOLEAN DEFAULT TRUE,

  INDEX idx_user_id (user_id),
  INDEX idx_is_current (is_current)
);

-- Products Dimension
CREATE TABLE dim_products (
  product_key BIGSERIAL PRIMARY KEY,
  product_id UUID NOT NULL UNIQUE,
  product_name VARCHAR(255) NOT NULL,
  category VARCHAR(100),
  subcategory VARCHAR(100),
  brand VARCHAR(100),
  price DECIMAL(12, 2),
  cost DECIMAL(12, 2),
  supplier_id UUID,
  created_date DATE,

  INDEX idx_category (category),
  INDEX idx_brand (brand)
);

-- Date Dimension (pre-computed for fast grouping)
CREATE TABLE dim_date (
  date_key INT PRIMARY KEY,
  date DATE UNIQUE,
  year INT,
  month INT,
  day INT,
  quarter INT,
  week_of_year INT,
  day_of_week INT,
  day_name VARCHAR(10),
  month_name VARCHAR(10),
  is_weekend BOOLEAN,
  is_holiday BOOLEAN,

  INDEX idx_date (date)
);

-- Time Dimension (for intraday analysis)
CREATE TABLE dim_time (
  time_key INT PRIMARY KEY,
  time_of_day TIME,
  hour INT,
  minute INT,
  second INT,
  period_of_day VARCHAR(20)  -- morning, afternoon, evening, night
);
```

### Aggregate Tables (Pre-Computed)

```sql
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
```

---

## FraiseQL OLAP Schema

```python
# analytics_schema.py
from fraiseql import types
from decimal import Decimal
from datetime import datetime, date

@types.object
class Event:
    """Raw event data (fact table)"""
    event_id: int
    event_date: date
    event_timestamp: datetime
    user_id: str
    product_id: str
    event_type: str
    revenue: Decimal | None
    quantity: int | None
    country: str
    source: str

@types.object
class DimUser:
    """User dimension (reference data)"""
    user_id: str
    email: str
    full_name: str
    country: str
    signup_date: date
    lifetime_value: Decimal
    customer_segment: str
    last_activity_date: date

@types.object
class DimProduct:
    """Product dimension"""
    product_id: str
    product_name: str
    category: str
    brand: str
    price: Decimal
    cost: Decimal

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
```

---

## Complex OLAP Queries

### Daily Revenue Trend

```graphql
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
```

**Corresponding SQL (generated by FraiseQL):**

```sql
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
```

### Cohort Retention Analysis

```graphql
query RetentionCohort($cohortStart: Date!, $cohortEnd: Date!, $maxDays: Int!) {
  retentionCohort(cohortStart: $cohortStart, cohortEnd: $cohortEnd, maxDays: $maxDays) {
    cohortDate
    daysSinceSilgnup
    cohortSize
    retentionRate
    revenue
  }
}
```

**SQL (complex multi-CTE query):**

```sql
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
```

### Funnel Conversion Analysis

```graphql
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
```

---

## Performance Optimization Strategies

### 1. Pre-Computed Aggregates

```sql
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
```

### 2. Partition Pruning

Queries are automatically optimized by date:

```sql
-- Only scans relevant partitions
SELECT * FROM events
WHERE event_date BETWEEN '2024-06-01' AND '2024-06-30';
-- ↑ Only queries events_2024_06 partition
```

### 3. Materialized Views

```sql
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
```

### 4. Columnar Storage (Citus/Timescale Extension)

For very large fact tables:

```sql
-- Create hypertable (TimescaleDB)
CREATE TABLE events (
  event_timestamp TIMESTAMP NOT NULL,
  user_id UUID,
  event_type VARCHAR(50),
  revenue DECIMAL(12, 2)
) WITH (timescaledb.compress);

-- Compress old data automatically
SELECT add_compression_policy('events', INTERVAL '7 days');
```

---

## Real-Time Subscriptions

### Live Metrics Dashboard

```graphql
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
```

**Implementation:**

```python
@types.subscription
class Subscription:
    def realtime_metrics(self) -> dict:
        """Stream updates every 10 seconds"""
        # Queries the last 1-hour aggregates
        # Emits when new data arrives
        pass
```

---

## Export Capabilities

### CSV Export

```graphql
query ExportDailyRevenue($startDate: Date!, $endDate: Date!) {
  exportDailyRevenue(startDate: $startDate, endDate: $endDate, format: CSV) {
    url  # Pre-signed S3 URL
    size_bytes
    created_at
  }
}
```

### Arrow Flight Export (Bulk Data)

```graphql
query ExportArrowFlight($startDate: Date!, $endDate: Date!) {
  exportArrowFlight(startDate: $startDate, endDate: $endDate) {
    arrow_endpoint  # Arrow Flight server endpoint
    query_id
    row_count
  }
}
```

**Client Usage:**

```python
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
```

---

## Dashboard Implementation (React)

```typescript
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
```

---

## Testing Analytical Queries

```typescript
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
```

---

## Monitoring Analytical Performance

```sql
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
```

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
