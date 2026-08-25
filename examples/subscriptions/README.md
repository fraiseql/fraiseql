# Subscriptions

A GraphQL subscription in FraiseQL is not a long-running query. The
`SubscriptionManager` holds the open subscriptions, a change source publishes
events into it, and each event is matched against every open subscription and
projected to that subscriber's selection. Whatever moves the bytes to the client —
a WebSocket, an SSE stream, a webhook — sits on top of the broadcast channel the
manager hands out.

This example plays the whole cycle in one process: subscribe two clients, publish
three events, show which client got what, then unsubscribe. No database, no server.

## Run it

```bash
./run.sh
```

## What to read

Three things that are easy to get wrong:

- **Take the receiver before subscribing.** It is a broadcast channel; an event
  published while nobody holds a receiver is dropped, not queued.
- **`entity_type` is what routes an event.** It must equal the subscription's
  GraphQL return type — `Message` for `onMessage`. An event for an entity nobody
  watches matches zero subscriptions and costs nothing, silently.
- **Closing a connection unsubscribes everything on it.** `unsubscribe_connection`
  is what the transport calls when a socket goes away, so a dropped client cannot
  leak subscriptions.

## Uses

`examples/streaming` — the event/message/presence/metrics schema and its four
subscriptions.
