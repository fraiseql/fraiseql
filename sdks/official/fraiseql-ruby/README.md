# fraiseql-ruby

> **Status: Not yet implemented.**

The Ruby authoring SDK for FraiseQL is planned but not yet built.

## What it will provide

A Ruby-native way to define FraiseQL schemas that compile to `schema.json`:

```ruby
# Planned API (subject to change)
module MyApp
  extend FraiseQL::Schema

  fraiseql_type :User do
    field :id,    :Int,    null: false
    field :name,  :String, null: false
    field :email, :String
  end

  fraiseql_query :users do
    returns list_of(:User)
    arg :limit, :Int, default: 10
    sql_source "v_users"
  end
end

FraiseQL.export_schema("schema.json")
```

## Alternatives

The following SDKs are production-ready today:

- [fraiseql-python](../fraiseql-python) — reference implementation
- [fraiseql-typescript](../fraiseql-typescript)
- [fraiseql-java](../fraiseql-java)
- [fraiseql-php](../fraiseql-php)
- [fraiseql-go](../fraiseql-go)

## Contributing

Contributions welcome. See the Python SDK for the reference authoring API and
the expected `schema.json` output format.
