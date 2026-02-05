# IoT Platform with Time-Series Data

**Status:** ✅ Production Ready
**Complexity:** ⭐⭐⭐⭐ (Advanced)
**Audience:** IoT architects, DevOps engineers, data engineers
**Reading Time:** 25-30 minutes
**Last Updated:** 2026-02-05

Complete guide to building a scalable IoT platform for collecting and querying sensor data efficiently.

---

## Architecture Overview

```
┌──────────────┬──────────────┬──────────────┐
│   Devices    │   Devices    │   Devices    │
│  (millions)  │  (millions)  │  (millions)  │
└──────────┬───┴──────┬───────┴──────┬───────┘
           │          │               │
           └──────────┼───────────────┘
                      ↓ (MQTT/HTTP)
         ┌────────────────────────┐
         │  Message Broker        │
         │  (Kafka/MQTT/Redis)    │
         └────────────┬───────────┘
                      ↓
         ┌────────────────────────┐
         │  Stream Processor      │
         │  (Validation,          │
         │   Aggregation)         │
         └────────────┬───────────┘
                      ↓
         ┌────────────────────────┐
         │  Time-Series Database  │
         │  (PostgreSQL +         │
         │   TimescaleDB)         │
         └────────────┬───────────┘
                      ↓
         ┌────────────────────────┐
         │  FraiseQL GraphQL      │
         │  (Query Layer)         │
         └────────────────────────┘
```

---

## Schema Design

### Devices & Metadata

```sql
-- Device registry
CREATE TABLE devices (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  device_id VARCHAR(100) UNIQUE NOT NULL,  -- External ID (MAC, serial)
  name VARCHAR(255) NOT NULL,
  device_type VARCHAR(50) NOT NULL,  -- temperature_sensor, humidity_sensor, etc.
  location VARCHAR(255),  -- Building/Room
  latitude NUMERIC(10, 8),
  longitude NUMERIC(11, 8),
  owner_id UUID NOT NULL,
  status VARCHAR(50) NOT NULL,  -- active, inactive, error
  last_heartbeat TIMESTAMP,
  created_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_device_id (device_id),
  INDEX idx_owner_id (owner_id),
  INDEX idx_status (status),
  INDEX idx_device_type (device_type)
);

-- Device configuration
CREATE TABLE device_config (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  device_id UUID NOT NULL UNIQUE REFERENCES devices(id) ON DELETE CASCADE,
  read_interval INT NOT NULL,  -- Seconds between readings
  alert_threshold JSONB,  -- { temperature: { min: 0, max: 100 } }
  data_retention_days INT DEFAULT 365,
  custom_fields JSONB,  -- Any device-specific config
  updated_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_device_id (device_id)
);

-- Sensor metadata (what each device measures)
CREATE TABLE sensors (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  sensor_name VARCHAR(100) NOT NULL,  -- temperature, humidity, pressure
  unit VARCHAR(20),  -- Celsius, %, hPa
  sensor_type VARCHAR(50),  -- analog, digital, counter
  accuracy DECIMAL(5, 2),
  created_at TIMESTAMP DEFAULT NOW(),

  UNIQUE(device_id, sensor_name),
  INDEX idx_device_id (device_id)
);
```

### Time-Series Data

```sql
-- Raw sensor readings (hypertable with TimescaleDB)
CREATE TABLE sensor_readings (
  time TIMESTAMP NOT NULL,
  device_id UUID NOT NULL,
  sensor_name VARCHAR(100) NOT NULL,
  value NUMERIC(10, 4) NOT NULL,
  unit VARCHAR(20),
  quality VARCHAR(50),  -- good, poor, unknown
  FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
);

-- Create hypertable (TimescaleDB extension)
SELECT create_hypertable('sensor_readings', 'time');

-- Automatic compression of old data
SELECT add_compression_policy('sensor_readings', INTERVAL '7 days');

-- Automatic retention (delete data older than 1 year)
SELECT add_retention_policy('sensor_readings', INTERVAL '1 year');

-- Indexes for common queries
CREATE INDEX idx_device_time ON sensor_readings (device_id, time DESC);
CREATE INDEX idx_sensor_time ON sensor_readings (sensor_name, time DESC);
```

### Aggregated Data (Pre-Computed)

```sql
-- Hourly aggregates (faster for dashboards)
CREATE TABLE sensor_readings_1h (
  time TIMESTAMP NOT NULL,
  device_id UUID NOT NULL,
  sensor_name VARCHAR(100) NOT NULL,
  avg_value NUMERIC(10, 4),
  min_value NUMERIC(10, 4),
  max_value NUMERIC(10, 4),
  reading_count INT,
  FOREIGN KEY (device_id) REFERENCES devices(id)
);

-- Create hypertable
SELECT create_hypertable('sensor_readings_1h', 'time');

-- Continuous aggregate (automatically updated)
CREATE MATERIALIZED VIEW sensor_readings_1h AS
SELECT
  time_bucket('1 hour', time) as time,
  device_id,
  sensor_name,
  AVG(value) as avg_value,
  MIN(value) as min_value,
  MAX(value) as max_value,
  COUNT(*) as reading_count
FROM sensor_readings
GROUP BY time_bucket('1 hour', time), device_id, sensor_name;

-- Same for daily
CREATE MATERIALIZED VIEW sensor_readings_1d AS
SELECT
  time_bucket('1 day', time) as time,
  device_id,
  sensor_name,
  AVG(value) as avg_value,
  MIN(value) as min_value,
  MAX(value) as max_value,
  COUNT(*) as reading_count
FROM sensor_readings
WHERE time >= NOW() - INTERVAL '1 year'
GROUP BY time_bucket('1 day', time), device_id, sensor_name;
```

### Alerts & Events

```sql
CREATE TABLE device_alerts (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  device_id UUID NOT NULL REFERENCES devices(id),
  alert_type VARCHAR(50) NOT NULL,  -- threshold_exceeded, device_offline, low_battery
  severity VARCHAR(50) NOT NULL,  -- info, warning, critical
  value NUMERIC(10, 4),
  threshold NUMERIC(10, 4),
  acknowledged BOOLEAN DEFAULT FALSE,
  acknowledged_by UUID,
  acknowledged_at TIMESTAMP,
  created_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_device_id (device_id),
  INDEX idx_severity (severity),
  INDEX idx_acknowledged (acknowledged),
  INDEX idx_created_at (created_at)
);
```

---

## FraiseQL Schema

```python
# iot_schema.py
from fraiseql import types
from datetime import datetime
from decimal import Decimal

@types.object
class Device:
    id: str
    device_id: str
    name: str
    device_type: str
    location: str | None
    latitude: Decimal | None
    longitude: Decimal | None
    status: str  # active, inactive, error
    last_heartbeat: datetime | None
    sensors: list['Sensor']
    current_readings: list['SensorReading']
    alerts: list['Alert']

@types.object
class Sensor:
    id: str
    device: Device
    sensor_name: str
    unit: str
    accuracy: Decimal

@types.object
class SensorReading:
    time: datetime
    device: Device
    sensor_name: str
    value: Decimal
    unit: str
    quality: str

@types.object
class SensorMetric:
    """Aggregated metric"""
    time: datetime
    avg_value: Decimal
    min_value: Decimal
    max_value: Decimal
    reading_count: int

@types.object
class Alert:
    id: str
    device: Device
    alert_type: str
    severity: str  # info, warning, critical
    value: Decimal | None
    threshold: Decimal | None
    acknowledged: bool
    created_at: datetime

@types.object
class Query:
    def device(self, id: str) -> Device:
        """Get device details"""
        pass

    def devices(self, device_type: str | None = None) -> list[Device]:
        """List all devices"""
        pass

    def sensor_readings(
        self,
        device_id: str,
        sensor_name: str,
        start_time: str,
        end_time: str,
        limit: int = 1000
    ) -> list[SensorReading]:
        """Get raw sensor readings"""
        pass

    def sensor_metrics(
        self,
        device_id: str,
        sensor_name: str,
        start_time: str,
        end_time: str,
        granularity: str = '1h'  # 1h, 1d, 1w
    ) -> list[SensorMetric]:
        """Get aggregated metrics (faster)"""
        pass

    def active_alerts(self) -> list[Alert]:
        """Unacknowledged alerts"""
        pass

    def device_status_summary(self) -> dict:
        """Status summary (active, offline, error counts)"""
        pass

@types.object
class Mutation:
    def create_device(
        self,
        device_id: str,
        name: str,
        device_type: str
    ) -> Device:
        """Register new device"""
        pass

    def record_reading(
        self,
        device_id: str,
        sensor_name: str,
        value: Decimal,
        timestamp: str
    ) -> SensorReading:
        """Record sensor reading"""
        pass

    def acknowledge_alert(self, alert_id: str) -> Alert:
        """Mark alert as acknowledged"""
        pass

@types.subscription
class Subscription:
    def device_status(self, device_id: str) -> dict:
        """Real-time device status updates"""
        pass

    def alerts(self) -> Alert:
        """Real-time alert stream"""
        pass

    def metrics(self, device_id: str) -> SensorMetric:
        """Real-time metric updates"""
        pass
```

---

## MQTT Ingestion

### Stream Processor

```python
import asyncio
import aiomqtt
from datetime import datetime

class MQTTIngestionService:
    def __init__(self, db_client, kafka_producer):
        self.db = db_client
        self.kafka = kafka_producer

    async def start(self):
        async with aiomqtt.Client('mqtt.broker.local') as client:
            async with client.messages() as messages:
                async for message in messages:
                    await self.process_message(message)

    async def process_message(self, message):
        """
        Expected MQTT topic: sensors/{device_id}/{sensor_name}
        Expected payload: { value: 23.5, timestamp: ISO8601 }
        """
        try:
            # Parse topic
            parts = message.topic.split('/')
            if len(parts) < 3:
                return

            device_id = parts[1]
            sensor_name = parts[2]
            payload = json.loads(message.payload.decode())

            # Validate device exists
            device = await self.db.fetchrow(
                'SELECT id FROM devices WHERE device_id = $1',
                device_id
            )
            if not device:
                print(f'Unknown device: {device_id}')
                return

            # Insert reading
            await self.db.execute("""
                INSERT INTO sensor_readings
                (time, device_id, sensor_name, value)
                VALUES ($1, $2, $3, $4)
            """, (
                datetime.fromisoformat(payload.get('timestamp', datetime.now().isoformat())),
                device['id'],
                sensor_name,
                payload['value']
            ))

            # Check for alerts
            await self.check_alerts(device['id'], sensor_name, payload['value'])

            # Publish to Kafka for other consumers
            self.kafka.send('sensor-readings', {
                'device_id': device_id,
                'sensor_name': sensor_name,
                'value': payload['value'],
                'timestamp': datetime.now().isoformat()
            })

        except Exception as e:
            print(f'Error processing message: {e}')

    async def check_alerts(self, device_id, sensor_name, value):
        """Check if value exceeds threshold"""
        config = await self.db.fetchrow("""
            SELECT alert_threshold FROM device_config WHERE device_id = $1
        """, device_id)

        if not config:
            return

        thresholds = config.get('alert_threshold', {})
        sensor_threshold = thresholds.get(sensor_name)

        if not sensor_threshold:
            return

        # Check if value exceeds
        if value > sensor_threshold.get('max'):
            await self.db.execute("""
                INSERT INTO device_alerts
                (device_id, alert_type, severity, value, threshold)
                VALUES ($1, 'threshold_exceeded', 'warning', $2, $3)
            """, (device_id, value, sensor_threshold['max']))
```

---

## Query Examples

### Real-Time Dashboard

```graphql
query DeviceDashboard($deviceId: ID!) {
  device(id: $deviceId) {
    id
    name
    status
    current_readings {
      sensor_name
      value
      unit
      time
    }
    alerts {
      id
      alert_type
      severity
      value
    }
  }
}
```

### Time-Series Analysis

```graphql
query TemperatureTrend(
  $deviceId: ID!
  $startTime: String!
  $endTime: String!
) {
  sensorMetrics(
    deviceId: $deviceId
    sensorName: "temperature"
    startTime: $startTime
    endTime: $endTime
    granularity: "1h"
  ) {
    time
    avg_value
    min_value
    max_value
  }
}
```

---

## Scaling Strategies

### Time-Based Partitioning

```sql
-- Automatic partitioning by date with TimescaleDB
SELECT set_integer_now_func('sensor_readings', 'pg_catalog.extract_epoch(now())::bigint'::regprocedure);

-- Chunks are automatically created
SELECT show_chunks('sensor_readings');
```

### Data Retention

```sql
-- Automatically delete old data
SELECT add_retention_policy('sensor_readings', INTERVAL '1 year');

-- Or manually archive to cold storage
INSERT INTO sensor_readings_archive
SELECT * FROM sensor_readings
WHERE time < NOW() - INTERVAL '1 year';

DELETE FROM sensor_readings
WHERE time < NOW() - INTERVAL '1 year';
```

### Downsampling

```sql
-- For very long-term storage, downsample to daily
CREATE MATERIALIZED VIEW sensor_summary_long_term AS
SELECT
  DATE(time) as date,
  device_id,
  sensor_name,
  AVG(value) as avg_value,
  MIN(value) as min_value,
  MAX(value) as max_value
FROM sensor_readings
WHERE time < NOW() - INTERVAL '90 days'
GROUP BY DATE(time), device_id, sensor_name;
```

---

## Alerting

### Rule Engine

```python
# Define alert rules
ALERT_RULES = [
    {
        'id': 'high_temp',
        'condition': 'sensor_name == "temperature" AND value > 100',
        'severity': 'critical',
        'message': 'High temperature detected'
    },
    {
        'id': 'device_offline',
        'condition': 'last_heartbeat < NOW() - INTERVAL 5 minutes',
        'severity': 'warning',
        'message': 'Device offline for 5+ minutes'
    },
    {
        'id': 'low_battery',
        'condition': 'sensor_name == "battery_level" AND value < 10',
        'severity': 'warning',
        'message': 'Battery level low'
    }
]

# Evaluate rules periodically
async def evaluate_alert_rules():
    for rule in ALERT_RULES:
        results = await db.fetch(f"""
            SELECT * FROM sensor_readings
            WHERE {rule['condition']}
            AND time > NOW() - INTERVAL '1 minute'
        """)

        for reading in results:
            await create_alert(
                device_id=reading['device_id'],
                alert_type=rule['id'],
                severity=rule['severity'],
                message=rule['message']
            )
```

---

## Testing

```typescript
describe('IoT Platform', () => {
  it('should ingest sensor readings', async () => {
    const deviceId = 'device_123';
    const reading = {
      sensorName: 'temperature',
      value: 23.5,
      timestamp: new Date().toISOString(),
    };

    await recordReading(deviceId, reading);

    const result = await client.query(GET_READINGS, {
      variables: {
        deviceId,
        sensorName: 'temperature',
        startTime: readingTime,
        endTime: new Date().toISOString(),
      },
    });

    expect(result.data.sensorReadings).toHaveLength(1);
    expect(result.data.sensorReadings[0].value).toBe(23.5);
  });

  it('should trigger alert on threshold', async () => {
    const device = await createDevice({
      alertThreshold: { temperature: { max: 30 } }
    });

    await recordReading(device.id, {
      sensorName: 'temperature',
      value: 35,  // Exceeds 30
    });

    const alerts = await getAlerts(device.id);
    expect(alerts).toHaveLength(1);
    expect(alerts[0].severity).toBe('warning');
  });

  it('should aggregate data correctly', async () => {
    const times = [1, 2, 3, 4, 5].map(i => new Date(Date.now() - i * 60000));
    const values = [20, 22, 21, 23, 24];

    for (let i = 0; i < 5; i++) {
      await recordReading(deviceId, {
        sensorName: 'temperature',
        value: values[i],
        timestamp: times[i],
      });
    }

    const metrics = await getMetrics(deviceId, 'temperature');

    expect(metrics[0].avg_value).toBe(22);  // (20+22+21+23+24)/5
    expect(metrics[0].min_value).toBe(20);
    expect(metrics[0].max_value).toBe(24);
  });
});
```

---

## Monitoring

```graphql
query ClusterHealth {
  deviceStatusSummary {
    total_devices
    active_devices
    offline_devices
    error_devices
    average_response_time_ms
    critical_alerts_count
    readings_per_second
  }
}
```

---

## See Also

**Related Patterns:**

- [Analytics Platform](./analytics-olap-platform.md) - OLAP for metrics
- [Real-Time Collaboration](./realtime-collaboration.md) - Real-time updates

**Deployment:**

- [Production Deployment](../guides/production-deployment.md)
- [Kubernetes Scaling](../guides/production-deployment.md)

**Monitoring:**

- [Observability & Monitoring](../guides/observability.md)

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
