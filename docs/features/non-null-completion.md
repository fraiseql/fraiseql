# Non-null completion

A field the schema publishes as non-null (`name: String!`) is never answered with `null`.
When the stored value is missing, or JSON `null`, the response follows GraphQL § 6.4.4: the
field is a **field error**, and the `null` moves to the nearest position that allows it.

```graphql
{ product(id: "3") { id maker { title } } }   # Maker.title: String!, row 3's maker has none
```

```json
{
  "data": { "product": { "id": "3", "maker": null } },
  "errors": [
    { "message": "Cannot return null for non-nullable field Maker.title.",
      "path": ["product", "maker", "title"] }
  ]
}
```

The response stays HTTP `200`, with `data` and `errors` together. Each violation is one
entry in `errors`; its `path` uses the response keys the client wrote (aliases) and list
indices.

## Where the null goes

The types are the ones introspection and the SDL publish:

| Position | Published as | An incomplete value there |
|---|---|---|
| A field | `T!` unless declared nullable | nulls its parent object |
| A list item | `[T!]` | nulls the list |
| A root list query | `[T!]!` | nulls `data` |
| A single query | `T` when declared nullable | is `null` |
| A mutation | `T` | is `null` |
| A relay edge's `node` | `T` | is `null` |
| A subscription event | `T` | is `null` |

So one incomplete row of a non-null list makes the whole field `null`, and, under a
non-null root, `data`. Declare a field nullable when its value may legitimately be absent.

A field gated with `on_deny = "mask"` is published nullable: a caller the gate refuses reads
it as `null`, which is a value it may take, not an error.

## Other transports

- **REST.** A representation has no partial form, so a read with an incomplete row is
  refused with `500`, naming the field and where it was.
- **MCP.** The tool result is the GraphQL response, `errors` included, and is marked as an
  error result.
- **gRPC.** Protobuf 3 has no `null`, so an unset field would read as `""` or `0`. A row
  with `NULL` in a non-null column refuses the RPC (`INTERNAL`), or ends a server stream
  with that status, naming the field.
- **Subscriptions.** On `/ws`, an event that cannot be completed is a `next` message with
  `data` `null` and its `errors`; webhook and Kafka deliveries carry the same `errors`. A
  REST stream sends an `error` frame instead of the entity.
