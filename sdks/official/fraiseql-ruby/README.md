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
- Subscription definitions
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

## Compile and Serve

```bash
ruby schema.rb                    # Generate schema.json
fraiseql-cli compile schema.json  # Compile to schema.compiled.json
fraiseql-server --schema schema.compiled.json
```

## Requirements

- Ruby >= 3.1
- FraiseQL CLI for schema compilation

## License

MIT or Apache 2.0
