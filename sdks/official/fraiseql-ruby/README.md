# FraiseQL Ruby SDK

Ruby client SDK for authoring FraiseQL GraphQL schemas.

## Installation

```ruby
# Gemfile
gem 'fraiseql', '~> 2.1'
```

```bash
bundle install
```

Or install directly:

```bash
gem install fraiseql
```

## Quick Start

```ruby
require 'fraiseql'

schema = FraiseQL::Schema.new

schema.type 'User', sql_source: 'users' do |t|
  t.field :id, :int
  t.field :name, :string
  t.field :email, :string
end

schema.type 'Post', sql_source: 'posts' do |t|
  t.field :id, :int
  t.field :title, :string
  t.field :body, :string
  t.field :fk_user, :int
end

schema.export_json('schema.json')
```

## Features

- Type definitions with SQL source mapping
- Enum support
- Query and mutation registration
- Field-level metadata (description, deprecation, access control)
- Fact table and analytics support (measures, dimensions)
- Observer and webhook configuration
- Custom scalar types
- CRUD auto-generation

## Field Metadata

```ruby
schema.type 'User', sql_source: 'users' do |t|
  t.field :id, :int
  t.field :email, :string,
    requires_scope: 'admin:read',
    description: 'User email address'
  t.field :ssn, :string,
    requires_scope: 'pii:read',
    on_deny: :null_mask,
    deprecated: 'Use encrypted_ssn instead'
end
```

A String stored as a locale map (`[locale]` in `fraiseql.toml`) takes `localized: true`, on
a field (`t.field :name, :string, localized: true`, input types included) or a mutation
argument (`m.argument :display_name, :string, localized: true`). Only a String can be
localized; any other type is refused where it is declared.

A field that embeds a type whose own query is scoped by `inject` takes
`scoped_like_parent: true` (`t.field :order, "Order", nullable: false, scoped_like_parent: true`).
It declares that the type holding the field is scoped by the same columns from the same
sources; the compiler checks the claim against every query of that type, and refuses the
embedding without it. Anything but `true` or `false` is refused, and so is the option on an
input type's field.

## Observers

An observer runs actions when a row of an entity is inserted, updated or deleted:

```ruby
schema.observer :order_shipped, entity: "Order", event: "UPDATE",
                                condition: "status == 'shipped'",
                                retry_: FraiseQL::ObserverRetry.new(
                                  max_attempts: 5, backoff_strategy: "exponential",
                                  initial_delay_ms: 200, max_delay_ms: 5000
                                ) do |o|
  o.webhook url_env: "ORDER_HOOK_URL", signing_secret_env: "ORDER_HOOK_KEY"
  o.slack "#orders", "Order {{ id }} shipped"
  o.email "ops@example.com", "Order shipped", "Order {{ id }} shipped"
end
```

A webhook takes exactly one of `url:` and `url_env:`. A Slack action posts to the URL in
`SLACK_WEBHOOK_URL` unless `webhook_url:` or `webhook_url_env:` names another. The email
sender is the server's SMTP configuration. An observer with no `retry_:` runs on the
server's own policy. The keyword is `retry_:` because `retry` is a Ruby keyword, as
`function`'s is `when_:`; `ObserverRetry` is not `FraiseQL::RetryConfig`, the client's
retry of a request.

## Compile and Serve

```bash
ruby schema.rb                    # Generate schema.json
fraiseql-cli compile schema.json  # Compile to schema.compiled.json
fraiseql-server --schema schema.compiled.json
```

## Decimal Values in the Client

A `Decimal` travels as a JSON string of its digits, `"1.50"`, in both directions.
`FraiseQL::Client` sends a `BigDecimal` variable as `to_s("F")`, a string of its digits
with no exponent (`JSON.generate` alone writes `"0.15e1"`, which the server refuses). A
NaN or infinite `BigDecimal` raises `ArgumentError` before the request is sent.

A `BigDecimal` holds a value, not a scale: `BigDecimal("100.00")` is sent as `"100.0"`.
The value is exact; the scale is not kept. Pass the `String` `"100.00"` when the scale
matters. A response holds each `Decimal` as its string, scale included, and
`BigDecimal(value)` reads it exactly. Do not pass a `Float`: the server refuses a number
with a fraction for a `Decimal`.

## Requirements

- Ruby >= 3.1
- FraiseQL CLI for schema compilation

## License

MIT or Apache 2.0
