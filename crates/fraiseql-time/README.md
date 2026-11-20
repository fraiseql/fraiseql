# fraiseql-time

The instant type FraiseQL crates share.

`Timestamp` is a point in time that writes fixed bytes in every form in which it leaves
the process:

| form | example |
|---|---|
| serde, `Debug` | `2025-11-20T12:00:00.120Z` |
| `to_rfc3339` | `2025-11-20T12:00:00.120+00:00` |
| `Display` | `2025-11-20 12:00:00.120 UTC` |

A fraction of a second is padded to 3, 6 or 9 digits and left out at a whole second:
the bytes chrono's `DateTime<Utc>` wrote for the same instant. Text is read with the
relaxed RFC 3339 grammar chrono's parser accepted.

With the `sqlx` or `postgres` feature, a `Timestamp` is a PostgreSQL `timestamptz`:
whole microseconds from 2000-01-01, truncated toward it when written. Reading a value
outside the range a `Timestamp` holds (`infinity`, `-infinity`, a year past 9999) is a
decode error naming the value.
