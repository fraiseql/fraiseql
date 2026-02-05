# NATS Integration Troubleshooting Guide

## Connection Issues

### Issue: ConnectionError - Failed to connect to NATS

**Error Message:**
```
ConnectionError: Failed to connect to NATS: [connection refused]
```

**Causes:**
1. NATS server is not running
2. Wrong NATS_SERVERS configuration
3. Network connectivity issues
4. Authentication credentials incorrect

**Solutions:**

1. **Verify NATS is running:**
```bash
docker-compose ps nats
docker-compose logs nats
```

2. **Check NATS_SERVERS configuration:**
```bash
echo $NATS_SERVERS
# Should output: nats://nats:4222 (or your configured value)
```

3. **Test connectivity:**
```bash
# From Fraisier container
docker-compose exec fraisier nc -zv nats 4222

# Or from host (if exposed)
nc -zv localhost 4222
```

4. **Verify credentials:**
```bash
echo $NATS_USERNAME
echo $NATS_PASSWORD
```

5. **Check network connectivity:**
```bash
docker-compose exec fraisier ping nats
```

---

### Issue: TimeoutError - Connection timeout

**Error Message:**
```
TimeoutError: Failed to connect: timeout exceeded
```

**Causes:**
1. NATS server is slow to start
2. NATS_TIMEOUT too short
3. Network latency

**Solutions:**

1. **Increase timeout:**
```bash
NATS_TIMEOUT=15  # Increase from default 5
```

2. **Wait for NATS to be ready:**
```bash
docker-compose up -d nats
sleep 5
docker-compose up -d fraisier
```

3. **Check NATS startup logs:**
```bash
docker-compose logs nats | tail -20
```

---

### Issue: Authentication failed

**Error Message:**
```
Error: Authentication failed
```

**Causes:**
1. Wrong username/password
2. NATS credentials not configured
3. NATS using different auth mechanism

**Solutions:**

1. **Verify credentials match:**
```bash
# In .env
NATS_USERNAME=fraisier
NATS_PASSWORD=fraisier_password

# In nats.conf
authorization {
  user: "fraisier"
  password: "fraisier_password"
}
```

2. **Reset credentials:**
```bash
docker-compose down
docker volume rm fraisier_nats-data
docker-compose up
```

3. **Check NATS auth configuration:**
```bash
docker-compose exec nats cat /etc/nats/nats.conf | grep -A 5 authorization
```

---

## Event Publishing Issues

### Issue: Events not being published

**Symptoms:**
- No events appear in NATS
- No errors in logs
- Event publishing code seems to run

**Causes:**
1. Event bus not connected
2. Event publishing silent failures
3. Configuration issues

**Solutions:**

1. **Verify event bus is initialized:**
```python
from fraisier.nats import is_nats_enabled, get_nats_config

if is_nats_enabled():
    config = get_nats_config()
    print(f"NATS enabled: {config.connection.servers}")
else:
    print("NATS not configured")
```

2. **Check event bus connection:**
```python
client = NatsClient(**config.connection.to_nats_client_kwargs())
await client.connect()
print("Connected:", client.connection is not None)
```

3. **Enable debug logging:**
```bash
FRAISIER_LOG_LEVEL=DEBUG
```

4. **Manually publish test event:**
```python
import asyncio
from fraisier.nats import NatsClient, NatsEventBus
from fraisier.nats.config import get_nats_config

async def test_publish():
    config = get_nats_config()
    client = NatsClient(**config.connection.to_nats_client_kwargs())
    event_bus = NatsEventBus(client)

    await client.connect()

    try:
        await event_bus.publish_deployment_event(
            event_type="deployment.test",
            deployment_id="test_001",
            data={"test": "data"},
        )
        print("✓ Test event published")
    except Exception as e:
        print(f"✗ Error: {e}")
    finally:
        await client.disconnect()

asyncio.run(test_publish())
```

---

### Issue: Events published but not stored

**Symptoms:**
- Events published successfully
- But not persisted in JetStream
- Lost after server restart

**Causes:**
1. JetStream not enabled
2. Stream not created
3. Storage location permissions

**Solutions:**

1. **Verify JetStream is enabled:**
```bash
docker-compose exec nats nats server info
# Should show JetStream status: enabled
```

2. **Check streams exist:**
```bash
docker-compose exec nats nats stream list
```

3. **View stream info:**
```bash
docker-compose exec nats nats stream info DEPLOYMENT_EVENTS
```

4. **Check storage permissions:**
```bash
docker-compose exec nats ls -la /data/nats/
```

5. **Enable JetStream in config:**
```bash
# In docker-compose.yml
nats:
  command:
    - -js    # Enable JetStream
    - -c
    - /etc/nats/nats.conf
```

---

## Event Subscription Issues

### Issue: Subscribers not receiving events

**Symptoms:**
- Handler registered but never called
- No events reaching subscriber
- Filter conditions might be wrong

**Causes:**
1. Handler filter too restrictive
2. Handler not properly registered
3. Event type mismatch
4. Subscriber not connected

**Solutions:**

1. **Check handler registration:**
```python
registry = get_subscriber_registry()
print(f"Registered handlers: {registry.get_subscription_count()}")

# Get subscriptions for specific event type
subs = registry.get_subscriptions_for_event_type("deployment.started")
print(f"Subscriptions for deployment.started: {len(subs)}")
```

2. **Verify event filter:**
```python
# Too restrictive - only matches specific service
registry.register(handler, EventFilter(service="api"))

# Better - matches all services
registry.register(handler, EventFilter())
```

3. **Check event type names match:**
```python
# ✓ Correct
EventFilter(event_type="deployment.started")

# ✗ Wrong - uses constant
EventFilter(event_type=DeploymentEvents.STARTED)
# (Use the string value, not the constant name)
```

4. **Enable logging in handler:**
```python
async def my_handler(event):
    print(f"Handler called with event: {event.event_type}")
    # ... rest of handler ...
```

5. **Test handler directly:**
```python
from fraisier.nats.events import NatsEvent

event = NatsEvent(
    event_type="deployment.started",
    data={"service": "api"},
)

# Call handler directly
await my_handler(event)
print("Handler executed successfully")
```

---

### Issue: Handler errors causing pipeline failures

**Symptoms:**
- One handler fails, others don't execute
- Silent failures in logs
- Events not processed by any handler

**Causes:**
1. Unhandled exceptions in handler
2. Async/sync mismatch
3. Handler dependency on missing service

**Solutions:**

1. **Wrap handler in try/except:**
```python
async def safe_handler(event):
    try:
        # Handler logic
        pass
    except Exception as e:
        logger.error(f"Handler error: {e}", exc_info=True)
        # Re-raise if critical, otherwise continue
```

2. **Verify handler type (sync vs async):**
```python
# ✓ Async handler
async def async_handler(event):
    await some_async_operation()

registry.register(async_handler, is_async=True)

# ✗ Wrong - async handler not marked
registry.register(async_handler, is_async=False)
```

3. **Configure retry logic:**
```python
registry.register(
    handler,
    retry_on_failure=True,
    retry_count=3,
    retry_delay=2.0,
)
```

4. **Monitor handler execution:**
```python
def monitored_handler(event):
    logger.info(f"Before: {event.event_type}")
    try:
        # Handler logic
        logger.info(f"Success: {event.event_type}")
    except Exception as e:
        logger.error(f"Error: {e}", exc_info=True)
```

---

## Storage and Retention Issues

### Issue: JetStream storage full

**Error Message:**
```
stream limit exceeded
```

**Causes:**
1. Too many events stored
2. Retention period too long
3. Disk space exhausted

**Solutions:**

1. **Check disk usage:**
```bash
docker-compose exec nats df -h /data/nats/
```

2. **Check stream storage:**
```bash
docker-compose exec nats nats stream info DEPLOYMENT_EVENTS
# Look for "Store Size"
```

3. **Reduce retention periods:**
```bash
# In .env
NATS_DEPLOYMENT_EVENTS_RETENTION=360    # Instead of 720
NATS_HEALTH_EVENTS_RETENTION=72         # Instead of 168
```

4. **Purge old events:**
```bash
docker-compose exec nats nats stream purge DEPLOYMENT_EVENTS
```

5. **Reduce max stream size:**
```bash
NATS_STREAM_MAX_SIZE=536870912  # 512MB instead of 1GB
```

6. **Expand storage:**
```yaml
# docker-compose.yml
nats:
  volumes:
    - nats-data:/data/nats
    # Use larger volume or external storage
```

---

### Issue: Events not retained

**Symptoms:**
- Events disappear after restart
- Old events not in stream
- Can't replay events

**Causes:**
1. JetStream not using persistent storage
2. Stream configured with memory-only storage
3. Retention set to 0

**Solutions:**

1. **Verify persistent storage:**
```bash
# In nats.conf
jetstream {
  store_dir: "/data/nats"  # Must have mount point
}
```

2. **Check stream storage type:**
```bash
docker-compose exec nats nats stream info DEPLOYMENT_EVENTS
# Look for "Storage: File" (not "Memory")
```

3. **Set retention:**
```bash
NATS_DEPLOYMENT_EVENTS_RETENTION=720  # Days
# NOT 0 (which means no retention)
```

---

## Multi-Region Issues

### Issue: Regional events not isolated

**Symptoms:**
- Events from other regions received
- Regional filtering not working
- Cross-region interference

**Causes:**
1. Event region not being set
2. Filter not configured for region
3. Provider not passing region

**Solutions:**

1. **Verify event region is set:**
```python
event = NatsEvent(
    event_type="deployment.started",
    data={"service": "api"},
    region="us-east-1",  # Must be set
)
```

2. **Check provider region configuration:**
```python
provider = BareMetalProvider(
    config={...},
    event_bus=event_bus,
    region=config.regional.region,  # Must pass region
)
```

3. **Verify filter by region:**
```python
registry.register(
    handler,
    EventFilter(region="us-east-1"),  # Must filter
)
```

4. **Check environment configuration:**
```bash
NATS_REGION=us-east-1
DEPLOYMENT_REGIONS=us-east-1,us-west-2,eu-west-1
```

---

### Issue: Inter-region communication timeout

**Error Message:**
```
Timeout waiting for response from region
```

**Causes:**
1. Inter-region timeout too short
2. Network latency between regions
3. Remote region not responding

**Solutions:**

1. **Increase inter-region timeout:**
```bash
INTER_REGION_TIMEOUT=60  # Increase from 30
```

2. **Check network connectivity:**
```bash
ping other-region-host
traceroute other-region-host
```

3. **Verify all regions connected to same NATS:**
```bash
# All regions should use same NATS cluster
NATS_SERVERS=nats://central-nats:4222
```

---

## Configuration Issues

### Issue: Configuration not loading from environment

**Symptoms:**
- Default values used instead of env vars
- Changes to .env not reflected
- Wrong configuration loaded

**Causes:**
1. .env file not loaded
2. Environment variables not set
3. Configuration cache

**Solutions:**

1. **Verify .env is loaded:**
```bash
# Load environment manually
set -a
source .env
set +a
python app.py
```

2. **Check environment variables:**
```bash
env | grep NATS
env | grep DEPLOYMENT
```

3. **Verify configuration loading:**
```python
from fraisier.nats.config import get_nats_config
import os

print("NATS_SERVERS:", os.getenv("NATS_SERVERS"))
config = get_nats_config()
print("Config servers:", config.connection.servers)
```

4. **Clear configuration cache:**
```python
from fraisier.nats import reset_subscriber_registry
reset_subscriber_registry()
```

---

## Monitoring and Debugging

### Enable Debug Logging

```bash
# Fraisier debug logging
FRAISIER_LOG_LEVEL=DEBUG

# NATS server debug logging
# In nats.conf
logging {
  trace: true
  debug: true
}
```

### Monitor NATS Server

```bash
# Real-time server stats
docker-compose exec nats nats server info --watch

# Stream statistics
docker-compose exec nats nats stream report

# View stream messages
docker-compose exec nats nats stream view DEPLOYMENT_EVENTS
```

### Common NATS Commands

```bash
# List all streams
docker-compose exec nats nats stream list

# Detailed stream info
docker-compose exec nats nats stream info DEPLOYMENT_EVENTS

# Purge stream
docker-compose exec nats nats stream purge DEPLOYMENT_EVENTS

# List subscribers
docker-compose exec nats nats account info

# Check memory usage
docker-compose exec nats nats server info | grep -i memory
```

---

## Getting Help

If you encounter issues not covered here:

1. **Check logs:**
   - Fraisier: `docker-compose logs -f fraisier`
   - NATS: `docker-compose logs -f nats`

2. **Enable debug mode:**
   ```bash
   FRAISIER_LOG_LEVEL=DEBUG docker-compose up
   ```

3. **Review test cases:**
   - Configuration: `tests/test_nats_config.py`
   - Integration: `tests/test_nats_integration.py`
   - Subscribers: `tests/test_subscribers.py`

4. **Check documentation:**
   - Main guide: `NATS_INTEGRATION_GUIDE.md`
   - Examples: `NATS_EXAMPLES.md`

5. **Verify NATS installation:**
   ```bash
   docker-compose exec nats nats --version
   ```

6. **Test NATS directly:**
   ```bash
   # Connect and test
   docker-compose exec nats nats sub "test.>" &
   docker-compose exec nats nats pub "test.hello" "world"
   ```
