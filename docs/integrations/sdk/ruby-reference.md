# FraiseQL Ruby SDK Reference

**Status**: Production-Ready | **Ruby Version**: 3.2+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Ruby SDK. This guide covers the complete Ruby authoring interface for building type-safe GraphQL APIs with Rails models, metaprogramming, and expressive DSL patterns.

## Installation & Setup

### Gemfile Configuration

```ruby
# Gemfile
source 'https://rubygems.org'

gem 'rails', '~> 7.1'  # Rails 7.1+
gem 'fraiseql', '~> 2.0'
gem 'fraiseql-rails', '~> 2.0'  # Rails integration helpers

# Development/Schema authoring
group :development do
  gem 'fraiseql-cli', '~> 2.0'
end
```

Install dependencies:

```bash
bundle install
fraiseql-cli --version
```

### Requirements

- **Ruby 3.2+** (Pattern matching, type annotations with RBS)
- **Rails 7.1+** (Rails 8+ recommended for best integration)
- **Bundler 2.3+**
- **Database**: PostgreSQL, MySQL, SQLite, or SQL Server

### First Schema (60 seconds)

Create `app/graphql/schema.rb`:

```ruby
require 'fraiseql'

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
```

Build and deploy:

```bash
cd app/graphql && fraiseql-cli compile schema.json ../../fraiseql.toml
fraiseql-server --schema schema.compiled.json
```

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
type :UserProfile do
  field :id, :integer
  field :username, :string, nullable: false
  field :bio, :string, nullable: true
  field :roles, [:string]
  field :account, :Account
  description 'A user profile object'
end
```

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
```

### 2. Type Modifiers and Options

```ruby
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
```

### 3. Ruby to GraphQL Type Mapping

```ruby
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
```

---

## Operations: Queries, Mutations, Subscriptions

### Queries (Read Operations)

```ruby
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
```

### Mutations (Write Operations)

```ruby
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
```

### Subscriptions (Real-Time Streams)

```ruby
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
```

---

## Advanced Features

### Fact Tables for Analytics

```ruby
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
```

### Role-Based Access Control (RBAC)

```ruby
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
```

### Field Metadata and Documentation

```ruby
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
```

---

## Rails Integration Patterns

### 1. Rails Models with GraphQL Types

```ruby
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
```

### 2. Resolvers with Context

```ruby
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
```

### 3. Rails Generators for Schema

```bash
# Generate schema scaffold
rails generate fraiseql:schema users --model User

# Generated app/graphql/types/user_type.rb
type :User, model: User do
  field :id, :integer
  field :created_at, :datetime
  field :updated_at, :datetime
end
```

### 4. Active Record Associations

```ruby
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
```

---

## Error Handling

### StandardError and Custom Exceptions

```ruby
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
```

### Error Response Handling

```ruby
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
```

---

## Testing with RSpec

### Unit Tests

```ruby
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
```

---

## Schema Export Workflow

### Export to JSON

```ruby
# app/graphql/schema.rb
require 'fraiseql'

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
```

### Compilation and Deployment

```bash
# 1. Compile schema with configuration
fraiseql-cli compile schema.json fraiseql.toml

# 2. Verify compilation
fraiseql-cli verify schema.compiled.json

# 3. Deploy to server
fraiseql-server --schema schema.compiled.json

# 4. Or in Rails
bundle exec rails fraiseql:deploy
```

---

## Common Patterns: CRUD Operations

### Create

```ruby
mutation :create_user do
  field :email, :string, nullable: false
  field :name, :string, nullable: false
  returns :User

  resolve do |args, context:|
    User.create!(email: args[:email], name: args[:name])
  end
end
```

### Read

```ruby
query :user, :root do
  field :id, :integer, nullable: false
  returns :User, nullable: true

  resolve { |args| User.find_by(id: args[:id]) }
end
```

### Update

```ruby
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
```

### Delete

```ruby
mutation :delete_user do
  field :id, :integer, nullable: false
  returns :User

  resolve do |args, context:|
    user = User.find(args[:id])
    user.destroy
    user
  end
end
```

### Pagination

```ruby
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
```

---

## Type Mapping Reference

**Ruby ↔ GraphQL Mappings:**

```ruby
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
```

---

## See Also

- [FraiseQL Compiler Documentation](../../compiler/)
- [Rails Integration Guide](../../authentication/rails.md)
- [Security Best Practices](../../security/)
- [GraphQL Schema Design Patterns](../schema-design.md)
- [RBAC Implementation Guide](../rbac.md)
- [Performance Tuning](../../performance/)

---

**Legend**: ✅ = Supported | 🔶 = Partial | ❌ = Not Supported

**Questions?** See [FAQ](../../faq.md) or open an issue on [GitHub](https://github.com/fraiseql/fraiseql).
