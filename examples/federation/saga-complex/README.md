# FraiseQL Saga Example: Complex Multi-Service (Travel Booking)

This example demonstrates a **complex distributed saga** coordinating across five independent services to book flights, hotels, and cars for a trip.

## Scenario

A **travel booking saga** that shows:

- **Multi-service orchestration** (5 services, each with own DB)
- **Parallel and sequential steps** (reserve independently, then confirm sequentially)
- **Cascading failures** (if payment fails, all reservations roll back)
- **Complex compensation** (different logic for each service)
- **Performance optimization** (parallel steps reduce latency)

### The Travel Booking Saga Flow

```

1. Reserve Flight (Flight Service)
   ├─ Check availability
   ├─ Hold seat for 15 minutes
   └─ Return flightId
   ↓
2. Reserve Hotel (Hotel Service) [PARALLEL with step 1]
   ├─ Check availability for dates
   ├─ Hold room for 15 minutes
   └─ Return hotelId
   ↓
3. Reserve Car (Car Service) [PARALLEL with step 1-2]
   ├─ Check availability for dates
   ├─ Hold vehicle for 15 minutes
   └─ Return carId
   ↓
4. Process Payment (Payment Service) [SEQUENTIAL after 1-3]
   ├─ Charge credit card
   └─ Return chargeId
   ↓
5. Confirm All (each service) [SEQUENTIAL after payment]
   ├─ Confirm flight (release hold, book permanently)
   ├─ Confirm hotel
   ├─ Confirm car
   └─ Send confirmation
   ↓
✅ Travel Booking Complete

❌ If payment fails:
   Compensation (parallel):
   - Cancel flight hold
   - Cancel hotel hold
   - Cancel car hold
   - Release any partial charge
```

## Architecture

```
┌────────────────────────────────────┐
│      Apollo Router (Gateway)        │
│      localhost:4000/graphql         │
└────────────┬───────────────────────┘
             │
    ┌────────┼────────┬──────────┬──────────┐
    │        │        │          │          │
┌───▼──┐ ┌──▼───┐ ┌──▼───┐ ┌───▼──┐ ┌────▼────┐
│Flight│ │Hotel │ │Car   │ │Pay   │ │Notif    │
│Svcs  │ │Svc   │ │Svc   │ │Svc   │ │Svc      │
└───┬──┘ └──┬───┘ └──┬───┘ └───┬──┘ └────┬────┘
    │       │        │         │         │
┌───▼───┐  │        │         │         │
│SQLite  │  │  PostgreSQL Database for all services
│(flights)┘  │  - flights, hotels, cars, payments, notifications
    │        │
```

## Why This Pattern?

**Multi-service sagas are useful when:**

- Services are independently deployed
- Each service has its own database
- Operations must be coordinated across boundaries
- Failures in one service affect others
- Strong consistency guarantees are needed

**Trade-offs:**

- More complex than single-service transactions
- Higher latency (coordinating multiple services)
- But: Services can be scaled independently
- And: Services don't need shared databases

## Key Features Demonstrated

### 1. Parallel Steps (Steps 1-3)

Reserve flights, hotels, and cars simultaneously:

```
Step 1: Reserve Flight   (starts 0ms, ends ~200ms)
Step 2: Reserve Hotel    (starts 0ms, ends ~200ms)
Step 3: Reserve Car      (starts 0ms, ends ~200ms)

Total: ~200ms (not 600ms like sequential)
```

### 2. Cascading Compensation

If payment fails, compensate all previous steps:

```
Step 1: ✓ Reserved Flight
Step 2: ✓ Reserved Hotel
Step 3: ✓ Reserved Car
Step 4: ✗ Payment failed

Compensation (parallel):

- Release Flight hold
- Release Hotel hold
- Release Car hold

All compensations parallel: ~100ms
```

### 3. Service Independence

Each service:

- Has its own database
- Implements its own GraphQL schema
- Handles its own errors
- Manages its own compensation

### 4. Strong Consistency

All 5 steps succeed together or all rollback together:

```
✓ Booking confirmed AND all 5 steps succeeded
✗ No booking, AND all holds released
(No partially booked trips)
```

## Files

```
saga-complex/
├── docker-compose.yml              # 6 services + router
├── README.md                        # This file
├── fixtures/                        # Database inits
│   ├── flight-init.sql
│   ├── hotel-init.sql
│   ├── car-init.sql
│   ├── payment-init.sql
│   ├── notification-init.sql
│   ├── supergraph.graphql
│   └── router.yaml
├── flight-service/
│   ├── Dockerfile
│   ├── schema.graphql
│   └── server.py
├── hotel-service/
│   └── ...
├── car-service/
│   └── ...
├── payment-service/
│   └── ...
├── notification-service/
│   └── ...
└── test-saga.sh                    # Integration tests
```

## Quick Start

### 1. Start Example

```bash
cd examples/federation/saga-complex
docker-compose up -d
```

### 2. Run Tests

```bash
./test-saga.sh
```

Tests:

- Successful booking (all steps succeed)
- Payment failure (all steps compensate)
- Flight unavailable (failure detection)
- Partial failures (cascade effect)

### 3. Manual Testing

```bash
# Get flight availability
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { flights(from: \"NYC\", to: \"LAX\") { id available price } }"
  }'

# Book travel
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation { bookTravel(userId: \"user-1\", flightId: \"f-1\", hotelId: \"h-1\", carId: \"c-1\") { bookingId status reservations { type id status } } }"
  }'
```

## Performance Analysis

### Parallel vs Sequential

```
SEQUENTIAL (steps one at a time):
├─ Flight: 200ms
├─ Hotel: 200ms
├─ Car: 200ms
├─ Payment: 100ms
└─ Confirmations: 200ms
   TOTAL: ~900ms

PARALLEL (steps 1-3 concurrent):
├─ Flight/Hotel/Car: 200ms (parallel)
├─ Payment: 100ms
└─ Confirmations: 200ms
   TOTAL: ~500ms  ← 44% faster!
```

### Optimization Opportunities

1. **Further parallelization**: Confirmations could also run parallel
2. **Service caching**: Cache flight/hotel availability
3. **Batch operations**: Group multiple bookings per request
4. **Connection pooling**: Reuse DB connections across requests

## Error Handling

### Network Errors

If a service is temporarily unavailable:

```
Retry policy:

- Attempt 1: 100ms delay
- Attempt 2: 200ms delay
- Attempt 3: 400ms delay
- Give up after 3 attempts
```

### Business Errors

If no availability:

```
Flight unavailable for dates
→ Fail fast (don't waste time on hotel/car)
→ Compensate immediately
```

### Consistency Errors

If compensation fails:

```
Payment.refund() fails
→ Log error for manual review
→ Alert operations team
→ Status: NEEDS_MANUAL_INTERVENTION
```

## Monitoring & Observability

Key metrics for complex sagas:

```

- Booking success rate: Should be 98%+
- Booking latency: Should be <500ms p99
- Compensation rate: Should be <2%
- Service availability: Should be 99.9%+
```

Example dashboard:

```
Booking Status       Latency Distribution
  ✓ 98.5% (990)     [p50: 150ms]
  ⚠ 1.0% (10)       [p90: 300ms]
  ✗ 0.5% (5)        [p99: 450ms]

Service Health       Compensation Reasons
  Flight:  99.8%     - Unavailable: 60%
  Hotel:   99.9%     - Payment failed: 25%
  Car:     99.7%     - User cancelled: 15%
  Payment: 99.9%
  Notif:   99.2%
```

## Related Documentation

- **[saga-basic/](../saga-basic/README.md)** - Simple 3-service example
- **[saga-manual-compensation/](../saga-manual-compensation/README.md)** - Manual compensation pattern
- **[SAGA_PATTERNS.md](../../docs/SAGA_PATTERNS.md)** - Advanced patterns including parallel sagas

## Production Deployment

For production, ensure:

1. **Distributed Tracing**: Correlate requests across 5 services

   ```bash
   trace_id: abc-123-def
   │
   ├─ Flight Service: trace_id=abc-123-def span_id=f1
   ├─ Hotel Service: trace_id=abc-123-def span_id=h1
   ├─ Car Service: trace_id=abc-123-def span_id=c1
   ├─ Payment Service: trace_id=abc-123-def span_id=p1
   └─ Notification: trace_id=abc-123-def span_id=n1
   ```

2. **Circuit Breakers**: Fail fast if a service is down

   ```
   Flight Service down? → Don't try hotel/car
   → Fail fast and save 300ms
   ```

3. **Monitoring**: Alert on anomalies
   - Compensation rate > 5%
   - Latency p99 > 1000ms
   - Service unavailability

4. **Backup Plans**: Handle permanent failures
   - If payment fails, offer retry options
   - If flight unavailable, suggest alternatives

---

**Last Updated:** 2026-01-29

**Maintainer:** FraiseQL Federation Team
