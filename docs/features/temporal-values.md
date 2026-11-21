# Temporal values

The four temporal scalars hold every value a client sends to the format their
`specifiedByURL` publishes, from the [chillicream family](https://scalars.graphql.org/chillicream/):

| Scalar | Written | Offset | Specified by |
|---|---|---|---|
| `DateTime` | `2026-07-01T12:00:00Z`, `2026-07-01T14:00:00.5+02:00` | required: `Z` or `±HH:MM`, never `-00:00` | [date-time](https://scalars.graphql.org/chillicream/date-time) |
| `LocalDateTime` | `2026-07-01T12:00:00` | never | [local-date-time](https://scalars.graphql.org/chillicream/local-date-time) |
| `Date` | `2026-07-01` | never | [local-date](https://scalars.graphql.org/chillicream/local-date) |
| `Time` | `09:30:00`, `09:30:00.25` | never | [local-time](https://scalars.graphql.org/chillicream/local-time) |

In each, `T` and `Z` may be lower case, seconds are always written, fractional seconds are
one to nine digits, and the date and time must exist: no 30 February, no hour `24`, no
second `60`. `-00:00` is RFC 3339's "offset unknown", which a `DateTime` does not take.

## Where a value is checked

Every value that reaches a temporal cast is checked first, on every transport:

- a root argument, written inline or as a variable;
- a REST path parameter, and a REST filter value, which are not validated as GraphQL
  arguments;
- a field of a mutation's input object, at any depth;
- a `where` operand on a temporal field, nested or not, `in` and `nin` items included,
  from GraphQL;
- a fact table's `where` operand on a `timestamptz`, `timestamp`, `date` or `time` column.

A value outside its format is a validation error that names the argument or the field and
the format it takes. The value itself is never quoted back.

## Where a served value is checked

A served value is the text the view writes into the document, and PostgreSQL writes each
column type in one shape: a `timestamp` never with an offset, a `timestamptz` always with
one. A `DateTime` key written from a `timestamp` column is therefore served without an offset,
and read by a client in whatever zone it assumes.

A response serves a temporal field only in its scalar's format, on GraphQL, in an aggregate
row and on REST alike. A value outside it, or a value that is no string, is a field error
that names the field, its scalar and why, never the value, and its position is `null`, as
a missing value's is: at a non-null field the `null` reaches the nearest nullable
position. On REST, which has no partial response, it refuses the read with `500`, naming
the field and its published type.
On gRPC, whose message has no per-field error, it refuses the read with `INTERNAL`, in
GraphQL's words (see [the row view](../architecture/grpc.md#the-row-view)).

`compile --database` and `doctor --against-db` find such a view before it serves:

- they sample each served view's documents, the documents embedded in them included, and
  fail on a temporal field holding a value outside its scalar's format. The finding names the
  field, its scalar and how many sampled documents hold such a value, never a value. A view
  holding no rows proves nothing, and draws a warning instead;
- they fail on a temporal argument that filters a native column of another type than its
  scalar's, since the value is cast to the column's type: a `DateTime` over a `timestamp`
  column has its offset dropped;
- they warn on a served view, or with gRPC enabled a row view (`vr_<sql_source>`), whose
  definition reads a document key as text cast to a temporal type: `(data->>'at')::timestamptz`,
  a `#>>` path, `CAST(… AS timestamptz)`. PostgreSQL reads a value without an offset in the
  session's time zone, drops an offset at a `timestamp` or a `time`, and cuts a date-time to
  its date at a `date`, before the server sees it; the view's output is then a valid value,
  which no sample can tell from a right one. The warning names the view and the key, never a
  value. It is not drift: the cast is exact whenever the stored text carries an offset.

Each scalar is written from one column type:

| Scalar | Column type |
|---|---|
| `DateTime` | `timestamptz` |
| `LocalDateTime` | `timestamp` |
| `Date` | `date` |
| `Time` | `time` |

PostgreSQL writes a `timestamptz` before 1900 in some zones with an offset in seconds
(`+00:09:21` in `Europe/Paris`), which the `DateTime` format, `±HH:MM`, cannot carry. Such a
value is refused like any other outside the format; a server whose sessions run in `UTC`
never writes one.

## Why

An operand is cast `::timestamptz`, `::timestamp`, `::date` or `::time`, and PostgreSQL's
input parser reads far more than these formats. Without the check, each of these was
answered, with no error, by comparing a value other than the one sent:

- an offset-less `DateTime` was read in the session's `TimeZone`;
- a date alone, as a `DateTime`, was midnight in that zone;
- a `Time` carrying an offset had the offset dropped;
- `now`, `today` or `epoch` compared against the clock;
- hour `24` was read as the next day's midnight, and `01/02/2026` by the session's
  `DateStyle`.
