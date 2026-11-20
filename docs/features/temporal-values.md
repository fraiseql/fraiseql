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
