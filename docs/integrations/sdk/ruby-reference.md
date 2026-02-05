<!-- Skip to main content -->
---
title: FraiseQL Ruby SDK Reference
description: Complete API reference for the FraiseQL Ruby SDK. This guide covers the complete Ruby authoring interface for building type-safe GraphQL APIs with Rails models,
keywords: ["framework", "directives", "types", "sdk", "schema", "scalars", "monitoring", "api"]
tags: ["documentation", "reference"]
---

# FraiseQL Ruby SDK Reference

**Status**: Production-Ready | **Ruby Version**: 3.2+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Ruby SDK. This guide covers the complete Ruby authoring interface for building type-safe GraphQL APIs with Rails models, metaprogramming, and expressive DSL patterns.

## Installation & Setup

### Gemfile Configuration

```ruby
<!-- Code example in RUBY -->
# Gemfile
source 'https://rubygems.org'

gem 'rails', '~> 7.1'  # Rails 7.1+
gem 'FraiseQL', '~> 2.0'
gem 'FraiseQL-rails', '~> 2.0'  # Rails integration helpers

# Development/Schema authoring
group :development do
  gem 'FraiseQL-cli', '~> 2.0'
end
```text
<!-- Code example in TEXT -->

Install dependencies:

```bash
<!-- Code example in BASH -->
bundle install
FraiseQL-cli --version
```text
<!-- Code example in TEXT -->

### Requirements

- **Ruby 3.2+** (Pattern matching, type annotations with RBS)
- **Rails 7.1+** (Rails 8+ recommended for best integration)
- **Bundler 2.3+**
- **Database**: PostgreSQL, MySQL, SQLite, or SQL Server

### First Schema (60 seconds)

Create `app/graphql/schema.rb`:

```ruby
<!-- Code example in RUBY -->
require 'FraiseQL'

module AppSchema
  extend Fraiseql::DSL

  # Define types
  type :User do
    field :id, :integer
    field :name, :string
    field :email, :string
    field :created_at, :datetime
  end

  # Define queries
  query :users, :root do
    field :limit, :integer, default: 10
    returns [:User]
    sql_source :v_users
  end

  # Export schema
  export_schema('schema.json')
end
```text
<!-- Code example in TEXT -->

Build and deploy:

```bash
<!-- Code example in BASH -->
cd app/graphql && FraiseQL-cli compile schema.json ../../FraiseQL.toml
FraiseQL-server --schema schema.compiled.json
```text
<!-- Code example in TEXT -->

---

## Quick Reference Table

| Feature | Method | Purpose | Returns |
|---------|--------|---------|---------|
| **Types** | `type` | Define GraphQL types | Type schema |
| **Queries** | `query` | Read operations | Single/list result |
| **Mutations** | `mutation` | Write operations | Type result |
| **Fact Tables** | `fact_table` | Analytics tables | Aggregation schema |
| **Aggregate Queries** | `aggregate_query` | Analytics queries | Aggregated results |
| **Subscriptions** | `subscription` | Real-time streams | Event stream |
| **Field Metadata** | `metadata` | Schema annotations | Metadata dict |
| **RBAC Rules** | `authorize` | Access control | Auth result |
| **Validators** | `validate` | Field validation | Validation result |

---

## Type System

### 1. Defining Types with the `type` Block

Define GraphQL object types using Ruby blocks and method calls.

**Syntax:**

```ruby
<!-- Code example in RUBY -->
type :UserProfile do
  field :id, :integer
  field :username, :string, nullable: false
  field :bio, :string, nullable: true
  field :roles, [:string]
  field :account, :Account
  description 'A user profile object'
end
```text
<!-- Code example in TEXT -->

**Key Features**:

- **Block DSL**: Each type is defined in a block
- **Field Declarations**: Use `field` to declare each GraphQL field
- **Type References**: Reference other types by symbol (`:Account`)
- **Lists**: Wrap type in array `[:string]` for list types
- **Nullability**: Use `nullable: true/false` (default: `false` for scalars)
- **Descriptions**: Add `description` for documentation
- **ActiveRecord Integration**: Automatically map Rails models

**Examples**:

```ruby
<!-- Code example in RUBY -->
# ✅ Simple type
type :Post do
  field :id, :integer, nullable: false
  field :title, :string, nullable: false
  field :content, :string, nullable: false
  field :published_at, :datetime, nullable: true
end

# ✅ Nested types
type :Author do
  field :id, :integer
  field :name, :string
  field :posts, [:Post]  # List of nested type
end

# ✅ With ActiveRecord model
type :Product, model: Product do
  field :id, :integer
  field :name, :string
  field :price, :float
  field :category, :Category
end

# ✅ Custom resolver
type :User do
  field :id, :integer
  field :email, :string
  field :display_name, :string do
    resolve { |obj| "#{obj.first_name} #{obj.last_name}" }
  end
end
```text
<!-- Code example in TEXT -->

### 2. Type Modifiers and Options

```ruby
<!-- Code example in RUBY -->
# Nullable fields (allow null values)
field :nickname, :string, nullable: true

# Lists of types
field :tags, [:string]  # Non-null list of nullable strings
field :ids, [:integer], nullable: false  # Non-null list

# Default values
field :status, :string, default: 'active'
field :limit, :integer, default: 10

# Deprecated fields
field :legacy_field, :string, deprecated: "Use newField instead"

# Field metadata for documentation
field :api_key, :string, sensitive: true  # Will be sanitized in logs
```text
<!-- Code example in TEXT -->

### 3. Ruby to GraphQL Type Mapping

```ruby
<!-- Code example in RUBY -->
# Scalar types
field :count, :integer          # Int
field :rating, :float           # Float
field :active, :boolean         # Boolean
field :name, :string            # String
field :data, :json              # JSON (custom scalar)
field :created, :datetime       # DateTime (ISO 8601)
field :modified, :date          # Date
field :expires, :time           # Time
field :amount, :decimal         # Decimal (money, precise decimals)

# Collection types
field :ids, [:integer]          # [Int!]!
field :tags, [:string]          # [String!]!

# Reference types (nested objects)
field :author, :User            # User!
field :profile, :Profile        # Profile!
```text
<!-- Code example in TEXT -->

---

## Operations: Queries, Mutations, Subscriptions

### Queries (Read Operations)

```ruby
<!-- Code example in RUBY -->
query :users, :root do
  field :limit, :integer, default: 10, description: 'Number of users'
  field :offset, :integer, default: 0
  returns [:User]
  sql_source :v_users
  description 'Get all users with pagination'
end

# Access in Rails controller
query :current_user, :root do
  returns :User
  resolve { |context:| context[:user] }
end

# Complex query with filtering
query :posts_by_author, :root do
  field :author_id, :integer, nullable: false
  field :published_only, :boolean, default: true
  returns [:Post]
  sql_source :v_posts
end
```text
<!-- Code example in TEXT -->

### Mutations (Write Operations)

```ruby
<!-- Code example in RUBY -->
mutation :create_user do
  field :name, :string, nullable: false
  field :email, :string, nullable: false
  returns :User

  resolve do |args, context:|
    User.create!(
      name: args[:name],
      email: args[:email]
    )
  end
end

mutation :update_post do
  field :id, :integer, nullable: false
  field :title, :string
  field :content, :string
  returns :Post, nullable: true  # Can fail

  resolve do |args, context:|
    post = Post.find(args[:id])
    post.update(args.except(:id))
    post
  end
end

mutation :delete_user do
  field :id, :integer, nullable: false
  returns :User  # Return the deleted user

  resolve do |args, context:|
    user = User.find(args[:id])
    user.destroy
    user
  end
end
```text
<!-- Code example in TEXT -->

### Subscriptions (Real-Time Streams)

```ruby
<!-- Code example in RUBY -->
subscription :post_created do
  returns :Post

  filter { |post| true }  # All posts
end

subscription :user_status_changed do
  field :user_id, :integer, nullable: false
  returns :User

  filter do |user, args|
    user.id == args[:user_id]
  end
end
```text
<!-- Code example in TEXT -->

---

## Advanced Features

### Fact Tables for Analytics

```ruby
<!-- Code example in RUBY -->
# Define a fact table (OLAP structure)
fact_table :sales_facts do
  # Dimensions (grouping dimensions)
  dimension :date, :date
  dimension :region, :string
  dimension :product_category, :string
  dimension :customer_segment, :string

  # Measures (aggregatable values)
  measure :revenue, :decimal
  measure :quantity, :integer
  measure :transaction_count, :integer

  sql_source :fact_sales
end

# Query aggregated data
aggregate_query :sales_by_region do
  fact_table :sales_facts

  group_by [:region]
  aggregate [:revenue, :quantity]

  where do |sql|
    sql.date >= Date.current - 30.days
  end

  returns :SalesAggregation
end
```text
<!-- Code example in TEXT -->

### Role-Based Access Control (RBAC)

```ruby
<!-- Code example in RUBY -->
type :SecretData do
  field :id, :integer
  field :value, :string

  authorize do |user, context:|
    user.admin? || user.id == context[:owner_id]
  end
end

query :admin_users, :root do
  returns [:User]

  authorize do |user|
    user.roles.include?('admin')
  end
end

mutation :delete_user do
  field :id, :integer
  returns :User

  authorize do |user, args|
    # Only admins or self
    user.admin? || user.id == args[:id]
  end
end
```text
<!-- Code example in TEXT -->

### Field Metadata and Documentation

```ruby
<!-- Code example in RUBY -->
type :Invoice do
  field :id, :integer do
    description 'Unique invoice identifier'
    metadata({ indexed: true })
  end

  field :amount, :decimal do
    description 'Invoice total in USD'
    metadata({ precision: 10, scale: 2 })
  end

  field :status, :string do
    description 'Payment status'
    metadata({
      enum: ['pending', 'paid', 'overdue'],
      default: 'pending'
    })
  end

  field :encrypted_data, :string do
    description 'Encrypted customer data'
    metadata({ sensitive: true })
  end
end
```text
<!-- Code example in TEXT -->

---

## Rails Integration Patterns

### 1. Rails Models with GraphQL Types

```ruby
<!-- Code example in RUBY -->
# app/models/user.rb
class User < ApplicationRecord
  has_many :posts
  validates :email, presence: true
end

# app/graphql/schema.rb
type :User, model: User do
  field :id, :integer
  field :email, :string
  field :first_name, :string
  field :last_name, :string
  field :posts, [:Post] do
    resolve { |user| user.posts.limit(10) }
  end
end
```text
<!-- Code example in TEXT -->

### 2. Resolvers with Context

```ruby
<!-- Code example in RUBY -->
query :current_user, :root do
  returns :User

  resolve do |context:|
    User.find(context[:user_id])
  end
end

mutation :update_profile do
  field :name, :string
  returns :User

  resolve do |args, context:|
    user = User.find(context[:user_id])
    user.update!(name: args[:name])
    user
  end
end
```text
<!-- Code example in TEXT -->

### 3. Rails Generators for Schema

```bash
<!-- Code example in BASH -->
# Generate schema scaffold
rails generate FraiseQL:schema users --model User

# Generated app/graphql/types/user_type.rb
type :User, model: User do
  field :id, :integer
  field :created_at, :datetime
  field :updated_at, :datetime
end
```text
<!-- Code example in TEXT -->

### 4. Active Record Associations

```ruby
<!-- Code example in RUBY -->
type :Author do
  field :id, :integer
  field :name, :string
  field :posts, [:Post] do
    resolve { |author| author.posts.includes(:comments) }
  end
end

type :Post do
  field :id, :integer
  field :title, :string
  field :author, :Author do
    resolve { |post| post.author }
  end
  field :comments, [:Comment] do
    resolve { |post| post.comments.where(approved: true) }
  end
end

type :Comment do
  field :id, :integer
  field :body, :string
  field :post, :Post
  field :author, :Author
end
```text
<!-- Code example in TEXT -->

---

## Error Handling

### StandardError and Custom Exceptions

```ruby
<!-- Code example in RUBY -->
# Define custom error types
class FraiseQLError < StandardError; end
class ValidationError < FraiseQLError; end
class AuthorizationError < FraiseQLError; end

# Use in mutations
mutation :create_post do
  field :title, :string
  field :content, :string
  returns :Post

  resolve do |args, context:|
    raise AuthorizationError if context[:user].guest?
    raise ValidationError, 'Title required' if args[:title].blank?

    Post.create!(args)
  rescue ActiveRecord::RecordInvalid => e
    raise ValidationError, e.message
  end
end
```text
<!-- Code example in TEXT -->

### Error Response Handling

```ruby
<!-- Code example in RUBY -->
# Schema level error handling
type :UserResult do
  field :user, :User, nullable: true
  field :error, :string, nullable: true
  field :success, :boolean
end

mutation :create_user do
  returns :UserResult

  resolve do |args, context:|
    user = User.create!(args)
    { user:, success: true }
  rescue StandardError => e
    { error: e.message, success: false }
  end
end
```text
<!-- Code example in TEXT -->

---

## Testing with RSpec

### Unit Tests

```ruby
<!-- Code example in RUBY -->
# spec/graphql/types/user_type_spec.rb
RSpec.describe 'User type' do
  subject { Fraiseql::Schema.types[:User] }

  it 'has required fields' do
    expect(subject.fields.keys).to include(:id, :email, :name)
  end

  it 'has correct field types' do
    expect(subject.fields[:id].type).to eq(:integer)
    expect(subject.fields[:email].type).to eq(:string)
  end
end

# spec/graphql/queries/users_query_spec.rb
RSpec.describe 'users query' do
  let!(:users) { create_list(:user, 3) }

  it 'returns users list' do
    result = Fraiseql::Executor.execute(
      query: 'query { users { id name } }',
      context: { user_id: 1 }
    )
    expect(result[:data][:users].length).to eq(3)
  end
end
```text
<!-- Code example in TEXT -->

---

## Schema Export Workflow

### Export to JSON

```ruby
<!-- Code example in RUBY -->
# app/graphql/schema.rb
require 'FraiseQL'

module AppSchema
  extend Fraiseql::DSL

  type :User do
    field :id, :integer
    field :email, :string
  end

  query :users, :root do
    returns [:User]
  end

  # Export schema
  export_schema('schema.json')
end

# Run export
ruby app/graphql/schema.rb
# Generates: schema.json
```text
<!-- Code example in TEXT -->

### Compilation and Deployment

```bash
<!-- Code example in BASH -->
# 1. Compile schema with configuration
FraiseQL-cli compile schema.json FraiseQL.toml

# 2. Verify compilation
FraiseQL-cli verify schema.compiled.json

# 3. Deploy to server
FraiseQL-server --schema schema.compiled.json

# 4. Or in Rails
bundle exec rails FraiseQL:deploy
```text
<!-- Code example in TEXT -->

---

## Common Patterns: CRUD Operations

### Create

```ruby
<!-- Code example in RUBY -->
mutation :create_user do
  field :email, :string, nullable: false
  field :name, :string, nullable: false
  returns :User

  resolve do |args, context:|
    User.create!(email: args[:email], name: args[:name])
  end
end
```text
<!-- Code example in TEXT -->

### Read

```ruby
<!-- Code example in RUBY -->
query :user, :root do
  field :id, :integer, nullable: false
  returns :User, nullable: true

  resolve { |args| User.find_by(id: args[:id]) }
end
```text
<!-- Code example in TEXT -->

### Update

```ruby
<!-- Code example in RUBY -->
mutation :update_user do
  field :id, :integer, nullable: false
  field :email, :string
  field :name, :string
  returns :User

  resolve do |args, context:|
    user = User.find(args[:id])
    user.update!(args.except(:id))
    user
  end
end
```text
<!-- Code example in TEXT -->

### Delete

```ruby
<!-- Code example in RUBY -->
mutation :delete_user do
  field :id, :integer, nullable: false
  returns :User

  resolve do |args, context:|
    user = User.find(args[:id])
    user.destroy
    user
  end
end
```text
<!-- Code example in TEXT -->

### Pagination

```ruby
<!-- Code example in RUBY -->
query :posts, :root do
  field :limit, :integer, default: 20
  field :offset, :integer, default: 0
  returns [:Post]

  resolve do |args|
    Post
      .order(created_at: :desc)
      .limit(args[:limit])
      .offset(args[:offset])
  end
end
```text
<!-- Code example in TEXT -->

---

## Type Mapping Reference

**Ruby ↔ GraphQL Mappings:**

```ruby
<!-- Code example in RUBY -->
:integer       # GraphQL: Int
:float         # GraphQL: Float
:boolean       # GraphQL: Boolean
:string        # GraphQL: String
:json          # GraphQL: JSON (custom scalar)
:datetime      # GraphQL: DateTime (ISO 8601)
:date          # GraphQL: Date
:decimal       # GraphQL: Decimal (big numbers)

[:integer]     # GraphQL: [Int!]!
[:User]        # GraphQL: [User!]!
:User          # GraphQL: User! (non-nullable by default)
```text
<!-- Code example in TEXT -->

---

## See Also

- [FraiseQL Compiler Documentation](../../guides/README.md)
- [Rails Integration Guide](../../integrations/authentication/README.md)
- [Security Best Practices](../../guides/)
- [GraphQL Schema Design Patterns](../../architecture/README.md)
- [RBAC Implementation Guide](../../guides/authorization-quick-start.md)
- [Performance Tuning](../../performance/)

---

**Legend**: ✅ = Supported | 🔶 = Partial | ❌ = Not Supported

---

## Troubleshooting

### Common Setup Issues

#### Gem Installation Problems

**Issue**: `Could not find gem 'FraiseQL' in any of the gem sources`

**Solution**:

```bash
<!-- Code example in BASH -->
# Update gem source
gem sources -a https://rubygems.org

# Install FraiseQL
gem install FraiseQL

# Or in Gemfile
gem 'FraiseQL', '~> 2.0.0'
bundle install
```text
<!-- Code example in TEXT -->

#### Require/Load Issues

**Issue**: `cannot load such file -- FraiseQL`

**Solution - Check load path**:

```ruby
<!-- Code example in RUBY -->
# Add to Gemfile
gem 'FraiseQL'

# Then run
bundle install

# Verify installation
ruby -e "require 'FraiseQL'; puts FraiseQL::VERSION"
```text
<!-- Code example in TEXT -->

**Manual load**:

```ruby
<!-- Code example in RUBY -->
$LOAD_PATH.unshift('/path/to/FraiseQL/lib')
require 'FraiseQL'
```text
<!-- Code example in TEXT -->

#### Version Compatibility

**Issue**: Installed version incompatible

**Check Ruby version** (2.7+ required):

```bash
<!-- Code example in BASH -->
ruby --version
```text
<!-- Code example in TEXT -->

**Check installed gem**:

```bash
<!-- Code example in BASH -->
gem list FraiseQL
gem uninstall FraiseQL -v <old_version>
gem install FraiseQL -v 2.0.0
```text
<!-- Code example in TEXT -->

#### Bundler Issues

**Issue**: `bundle exec` fails with FraiseQL

**Solution**:

```bash
<!-- Code example in BASH -->
# Update Gemfile
bundle update FraiseQL

# Clear bundle cache
bundle clean --force

# Reinstall
bundle install
```text
<!-- Code example in TEXT -->

---

### Type System Issues

#### Type Definition Errors

**Issue**: `NameError: undefined method 'type' for FraiseQL`

**Cause**: Not requiring FraiseQL correctly

**Solution**:

```ruby
<!-- Code example in RUBY -->
# ✅ Correct
require 'FraiseQL'

class User
  include FraiseQL::Type

  field :id, :Int
  field :email, :String
end

# ❌ Wrong
class User
  type(:User) do  # Wrong syntax
    field :id, :Int
  end
end
```text
<!-- Code example in TEXT -->

#### Nullability Issues

**Issue**: `TypeError: expected nil, got String`

**Solution - Use optional explicitly**:

```ruby
<!-- Code example in RUBY -->
# ❌ Can be nil but not declared
class User
  include FraiseQL::Type
  field :email, :String
end

# ✅ Explicitly optional
class User
  include FraiseQL::Type
  field :email, :String, null: true
  field :name, :String, null: false  # Required
end
```text
<!-- Code example in TEXT -->

#### Field Type Issues

**Issue**: `UnknownTypeError: unknown type :CustomType`

**Cause**: Custom type not defined

**Solution - Define all types first**:

```ruby
<!-- Code example in RUBY -->
# ✅ Define in order
class Address
  include FraiseQL::Type
  field :street, :String
  field :city, :String
end

class User
  include FraiseQL::Type
  field :id, :Int
  field :address, Address  # Now Address is defined
end
```text
<!-- Code example in TEXT -->

#### Dynamic Definition Issues

**Issue**: `RuntimeError: Type already defined`

**Solution - Define types once at startup**:

```ruby
<!-- Code example in RUBY -->
# ✅ Define in initializer
# config/initializers/FraiseQL.rb
FraiseQL.reset!  # Clear if redefining

class User
  include FraiseQL::Type
  field :id, :Int
end

# ❌ Don't define in request handlers
# app/controllers/users_controller.rb
def show
  class User  # BAD! Redefining every request
    include FraiseQL::Type
  end
end
```text
<!-- Code example in TEXT -->

---

### Runtime Errors

#### Connection Issues

**Issue**: `PG::ConnectionBad: could not connect to server`

**Check environment**:

```bash
<!-- Code example in BASH -->
echo $DATABASE_URL
psql $DATABASE_URL -c "SELECT 1"
```text
<!-- Code example in TEXT -->

**Solution - Set connection string**:

```ruby
<!-- Code example in RUBY -->
ENV['DATABASE_URL'] = 'postgresql://user:pass@localhost/db'

server = FraiseQL::Server.from_compiled('schema.compiled.json')
```text
<!-- Code example in TEXT -->

#### Thread Safety Issues

**Issue**: Race condition or `FrozenError` in multi-threaded context

**Solution - Make thread-safe**:

```ruby
<!-- Code example in RUBY -->
# ❌ Not thread-safe
$fraiseql_server = FraiseQL::Server.from_compiled('schema.json')

# ✅ Thread-safe with mutex
require 'thread'
FraiseQL::Server.instance_eval do
  def self.server
    @server ||= FraiseQL::Server.from_compiled('schema.json')
  end

  def self.server=(srv)
    @server = srv
  end
end

# Or use Singleton pattern
class FraiseQLServer
  include Singleton

  def initialize
    @server = FraiseQL::Server.from_compiled('schema.json')
  end

  def execute(query, variables = {})
    @server.execute(query, variables)
  end
end

# Usage
FraiseQLServer.instance.execute(query)
```text
<!-- Code example in TEXT -->

#### Encoding Issues

**Issue**: `Encoding::InvalidByteSequenceError`

**Solution - Force UTF-8**:

```ruby
<!-- Code example in RUBY -->
# In config/environment.rb
Encoding.default_external = Encoding::UTF_8
Encoding.default_internal = Encoding::UTF_8

# Or in schema file
# -*- encoding: utf-8 -*-
require 'FraiseQL'
```text
<!-- Code example in TEXT -->

#### Timeout Issues

**Issue**: `Timeout::Error: execution timeout`

**Solution - Increase timeout**:

```ruby
<!-- Code example in RUBY -->
server = FraiseQL::Server.from_compiled(
  'schema.compiled.json',
  timeout: 60  # seconds
)

result = server.execute(query, timeout: 30)
```text
<!-- Code example in TEXT -->

---

### Performance Issues

#### Memory Leaks

**Issue**: Memory usage grows unbounded**

**Debug with memory_profiler**:

```ruby
<!-- Code example in RUBY -->
gem 'memory_profiler'

require 'memory_profiler'

report = MemoryProfiler.report do
  # Run queries
  server.execute(query)
end
report.pretty_print
```text
<!-- Code example in TEXT -->

**Solutions**:

```ruby
<!-- Code example in RUBY -->
# Paginate large result sets
query = """
  query {
    users(limit: 20, offset: 0) { id }
  }
"""

# Cache results
server = FraiseQL::Server.from_compiled(
  'schema.compiled.json',
  cache_ttl: 300  # 5 minutes
)
```text
<!-- Code example in TEXT -->

#### Database Connection Issues

**Issue**: `ActiveRecord::ConnectionNotEstablished`

**Solution - Configure connection pool**:

```yaml
<!-- Code example in YAML -->
# config/database.yml
development:
  adapter: postgresql
  pool: 5
  timeout: 5000
```text
<!-- Code example in TEXT -->

**Or explicitly**:

```ruby
<!-- Code example in RUBY -->
ActiveRecord::Base.establish_connection(
  adapter: 'postgresql',
  host: 'localhost',
  database: 'myapp_dev',
  pool: 10,
  timeout: 5000
)
```text
<!-- Code example in TEXT -->

#### Slow Queries

**Issue**: Queries take >5 seconds**

**Enable query logging**:

```ruby
<!-- Code example in RUBY -->
FraiseQL.logger.level = Logger::DEBUG

# Or use Rails logger
Rails.logger.level = :debug
```text
<!-- Code example in TEXT -->

**Optimize**:

```ruby
<!-- Code example in RUBY -->
# Add pagination
query = """
  query($limit: Int!, $offset: Int!) {
    users(limit: $limit, offset: $offset) { id }
  }
"""
variables = { limit: 20, offset: 0 }

# Cache if appropriate
server.cache_query(query, ttl: 300)
```text
<!-- Code example in TEXT -->

#### Bundle Size Issues

**Issue**: Gemfile.lock is >100MB**

**Clean unnecessary gems**:

```bash
<!-- Code example in BASH -->
bundle clean --force
bundle install
```text
<!-- Code example in TEXT -->

**Or audit**:

```bash
<!-- Code example in BASH -->
bundler-audit check
bundle outdated
```text
<!-- Code example in TEXT -->

---

### Debugging Techniques

#### Enable Logging

**Setup logging**:

```ruby
<!-- Code example in RUBY -->
# config/initializers/FraiseQL.rb
FraiseQL.logger = Logger.new($stdout)
FraiseQL.logger.level = Logger::DEBUG

# Or with Rails
Rails.logger.level = :debug
```text
<!-- Code example in TEXT -->

**Environment variable**:

```bash
<!-- Code example in BASH -->
FRAISEQL_DEBUG=true RUST_LOG=FraiseQL=debug rails s
```text
<!-- Code example in TEXT -->

#### Use Ruby Debugger

**With byebug**:

```ruby
<!-- Code example in RUBY -->
gem 'byebug', groups: [:development, :test]

# In code
def execute_query(query)
  byebug  # Pauses here
  server.execute(query)
end

# Then run
rails s
# Send request, debugger pauses in byebug console
```text
<!-- Code example in TEXT -->

#### Inspect Schema

**Print schema**:

```ruby
<!-- Code example in RUBY -->
schema = File.read('schema.compiled.json')
require 'json'
puts JSON.pretty_generate(JSON.parse(schema))
```text
<!-- Code example in TEXT -->

**Validate**:

```ruby
<!-- Code example in RUBY -->
require 'FraiseQL'
FraiseQL.validate_schema('schema.compiled.json')
```text
<!-- Code example in TEXT -->

#### Network Debugging

**Spy on database**:

```ruby
<!-- Code example in RUBY -->
# For Rails/ActiveRecord
ActiveRecord::Base.connection.execute "SET log_statement = 'all'"

# Or with Sequel
DB.loggers << Logger.new($stdout)
```text
<!-- Code example in TEXT -->

**Monitor HTTP requests**:

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:3000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ user(id: 1) { id } }"}' \
  -v
```text
<!-- Code example in TEXT -->

---

### Getting Help

#### GitHub Issues

Provide:

1. Ruby version: `ruby --version`
2. FraiseQL version: `gem list FraiseQL`
3. Rails version (if applicable)
4. Minimal reproducible example
5. Full stack trace
6. Relevant logs

**Issue template**:

```markdown
<!-- Code example in MARKDOWN -->
**Environment**:
- Ruby: 3.2.0
- FraiseQL: 2.0.0
- Rails: 7.0.4

**Issue**:
[Describe problem]

**Reproduce**:
[Minimal code example]

**Error**:
[Full error message]
```text
<!-- Code example in TEXT -->

#### Community Channels

- **GitHub Discussions**: Ask questions
- **Stack Overflow**: Tag with `FraiseQL` and `ruby`
- **Discord**: Real-time help
- **Ruby Forum**: Ruby community discussions

#### Profiling Tools

**Use ruby-prof**:

```ruby
<!-- Code example in RUBY -->
gem 'ruby-prof'

require 'ruby-prof'

RubyProf.start

# Your code here
server.execute(query)

result = RubyProf.stop
printer = RubyProf::FlatPrinter.new(result)
printer.print($stdout)
```text
<!-- Code example in TEXT -->

**Or with stackprof**:

```ruby
<!-- Code example in RUBY -->
gem 'stackprof'

require 'stackprof'

StackProf.run(mode: :cpu, out: 'tmp/stackprof.dump') do
  server.execute(query)
end

# Analyze
StackProf.results('tmp/stackprof.dump').print_text
```text
<!-- Code example in TEXT -->

---

## See Also

- [Security Best Practices](../../guides/)
- [GraphQL Schema Design Patterns](../../architecture/README.md)
- [RBAC Implementation Guide](../../guides/authorization-quick-start.md)
- [Performance Tuning](../../performance/)

---

**Questions?** See [FAQ](../../FAQ.md) or open an issue on [GitHub](https://github.com/FraiseQL/FraiseQL).
