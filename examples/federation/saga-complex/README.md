# Saga: complex multi-service travel booking

A distributed saga across five services, with a compensating action for every
step. Booking a trip reserves a flight, a hotel and a car, charges a card, and
sends a confirmation; if the charge is refused, everything already reserved is
released — in reverse order.

## What this example is, and is not

**It is** a demonstration of saga control flow: ordered steps, a failure that can
happen part-way, and compensation that undoes exactly the work that was done.

**It is not** a FraiseQL feature demo. The five services are standard-library
Python stubs and the router is Apollo Router. Nothing here compiles a FraiseQL
schema. It sits in `examples/federation/` because the read side is federated and
the saga is what a caller sees through that federation.

**Federation does not orchestrate the saga.** The router federates the *read*
side — `Query.flight`, `Query.hotel`, `Query.car` each resolve at their own
subgraph. `Mutation.bookTravel` resolves at one subgraph, `flight-service`, which
then calls the other four over HTTP. That distinction is the point: a saga is
service-to-service orchestration, not something a supergraph gives you.

## The saga

| # | Step | Service | Compensation |
|---|---|---|---|
| 1 | reserve flight | flight-service (local) | cancel the reservation |
| 2 | reserve hotel | hotel-service | `POST /internal/cancel` |
| 3 | reserve car | car-service | `POST /internal/cancel` |
| 4 | charge card | payment-service | `POST /internal/refund` |
| 5 | send confirmation | notification-service | none — nothing follows it |

Compensation runs in **reverse** order. The car is released before the hotel,
because the car was booked against the hotel stay.

`payment-service` refuses any `userId` beginning `decline`, deterministically.
That is what makes the failure branch reachable from a test rather than a
paragraph in a README.

## Running it

```bash
docker compose up -d --build
./test-saga.sh
```

`test-saga.sh` asserts both paths and exits non-zero on any failure, including
the router never coming up.

### The happy path

```graphql
mutation($userId: ID!, $flightId: ID!, $hotelId: ID!, $carId: ID!) {
  bookTravel(userId: $userId, flightId: $flightId, hotelId: $hotelId, carId: $carId) {
    bookingId status reason
    reservations { type id status }
    compensations
  }
}
```

with `userId: "u-42"`:

```json
{"status": "confirmed", "reason": null,
 "reservations": [{"type":"flight"}, {"type":"hotel"}, {"type":"car"}, {"type":"payment"}],
 "compensations": []}
```

### The failure path

The same mutation with `userId: "decline-7"`:

```json
{"status": "rolled_back",
 "reason": "card declined for decline-7",
 "reservations": [{"type":"flight"}, {"type":"hotel"}, {"type":"car"}],
 "compensations": ["car: compensated", "hotel: compensated", "flight: compensated"]}
```

A service being *unreachable* is handled the same way — stop `car-service` and
book, and the flight and hotel are released with `reason: "car step failed (0)"`.
The coordinator distinguishes the two: a refusal comes back as HTTP 402 with a
reason, an unreachable service as status 0.

## Layout

```
flight-service/        the coordinator, and the Flight subgraph
hotel-service/         Hotel subgraph  + /internal/reserve,/cancel
car-service/           Car subgraph    + /internal/reserve,/cancel
payment-service/       /internal/charge,/refund — declines `decline*` users
notification-service/  /internal/reserve — the last step
fixtures/supergraph.graphql   hand-maintained; see the note below
fixtures/router.yaml
```

Each service is a directory with a Dockerfile and a `server.py` on the Python
standard library — no framework, so the image needs no package index at build
and no dependency resolution at start.

> The supergraph here is hand-maintained. `examples/federation/basic` instead
> ships a `router/supergraph.yaml` and generates the artifact with
> `rover supergraph compose`, which is the more durable arrangement: this file
> drifted into being invalid and nothing noticed, because no CI leg starts this
> stack.
