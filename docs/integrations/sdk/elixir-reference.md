---
title: FraiseQL Elixir SDK Reference
description: Complete API reference for the FraiseQL Elixir SDK. This guide covers the Elixir authoring interface for building type-safe GraphQL APIs with functional pattern
keywords: ["framework", "directives", "types", "sdk", "schema", "scalars", "monitoring", "api"]
tags: ["documentation", "reference"]
---

# FraiseQL Elixir SDK Reference

**Status**: Production-Ready | **Elixir Version**: 1.14+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Elixir SDK. This guide covers the Elixir authoring interface for building type-safe GraphQL APIs with functional patterns, pattern matching, and OTP integration. Emphasizes idiomatic Elixir design using pipes, atoms, maps, and supervisor patterns.

## Quick Start

```bash
# mix.exs
def deps do
  [{:FraiseQL, "~> 2.0"}]
end
```

**Requirements**: Elixir 1.14+, OTP 25+, optional: Ecto, Phoenix

**First Schema**:

```elixir
defmodule MyApp.Schema.User do
  use FraiseQL.Schema

  defschema User do
    field :id, :integer
    field :name, :string
    field :email, :string
  end
end

defmodule MyApp.Schema.Queries do
  use FraiseQL.Schema

  defquery :users do
    field :limit, :integer, default: 10
    returns list_of(User)
    config sql_source: "v_users"
  end
end

FraiseQL.export_schema("schema.json")
```

Deploy: `FraiseQL-cli compile schema.json` → `FraiseQL-server --schema schema.compiled.json`

---

## Quick Reference Table

| Feature | Macro | Purpose | Returns |
|---------|-------|---------|---------|
| **Types** | `defschema` | GraphQL object types | Schema module |
| **Queries** | `defquery` | Read operations (SELECT) | Single or list |
| **Mutations** | `defmutation` | Write operations (INSERT/UPDATE/DELETE) | Type result |
| **Fact Tables** | `deffacttable` | Analytics tables (OLAP) | Aggregation schema |
| **Aggregate Queries** | `defaggregate` | Analytics queries | Aggregated results |
| **Observers** | `defobserver` | Event webhooks (async) | Event response |
| **Security** | `defsecurity` | RBAC and access control | Auth metadata |
| **Subscriptions** | `defsubscription` | Real-time pub/sub | Event stream |
| **Validators** | `defvalidator` | Field validation | Validation result |

---

## Type System

### The `defschema` Macro

Define GraphQL object types using Elixir modules:

```elixir
defmodule MyApp.Schema.User do
  use FraiseQL.Schema

  defschema User do
    field :id, :integer, description: "Unique identifier"
    field :name, :string
    field :email, :string, nullable: true
    field :is_active, :boolean, default: true
  end

  @type t :: %__MODULE__{
    id: integer(),
    name: String.t(),
    email: String.t() | nil
  }
end
```

**Features**: Type annotations, nullability, nested types, lists, descriptions, defaults

---

## Operations

### 1. Query Operations

Define read-only operations that map to database views:

```elixir
defmodule MyApp.Schema.Queries do
  use FraiseQL.Schema

  defquery :users do
    field :limit, :integer, default: 10
    field :offset, :integer, default: 0
    returns list_of(User)
    config sql_source: "v_users"
  end

  defquery :user_by_id do
    field :id, :integer, required: true
    returns User
    config sql_source: "v_users_by_id"
  end
end

# Use with pipe operator
def list_users(limit \\ 10) do
  %{limit: limit}
  |> execute_query(:users)
  |> case do
    {:ok, users} -> users
    {:error, reason} -> handle_error(reason)
  end
end
```

### 2. Mutation Operations

Define write operations that call stored procedures or functions:

```elixir
defmodule MyApp.Schema.Mutations do
  use FraiseQL.Schema

  defmutation :create_user do
    field :name, :string, required: true
    field :email, :string, required: true
    returns User
    config sql_function: "create_user"
  end

  defmutation :update_user do
    field :id, :integer, required: true
    field :name, :string, nullable: true
    returns User
    config sql_function: "update_user"
  end
end

# Use mutations with pattern matching
def create_new_user(name, email) do
  case execute_mutation(:create_user, %{name: name, email: email}) do
    {:ok, %{id: user_id} = user} -> {:ok, user}
    {:error, msg} -> {:error, "Failed: #{msg}"}
  end
end

# Chain mutations with pipe operator
def migrate_user_data(old_id, new_name) do
  %{id: old_id}
  |> execute_query(:user_by_id)
  |> case do
    {:ok, user} ->
      %{id: user.id, name: new_name}
      |> execute_mutation(:update_user)
    error -> error
  end
end
```

### 3. Subscriptions with OTP GenServer

Real-time pub/sub using GenServer and supervisor patterns:

```elixir
defmodule MyApp.Schema.Subscriptions do
  use FraiseQL.Schema

  defsubscription :user_updated do
    field :user_id, :integer, required: true
    returns User
    topic "users:#{user_id}:updated"
  end
end

defmodule MyApp.UserEvents do
  use GenServer

  def start_link(_opts), do: GenServer.start_link(__MODULE__, [], name: __MODULE__)
  def publish(user), do: GenServer.cast(__MODULE__, {:publish, user})

  @impl true
  def init(_), do: {:ok, %{}}

  @impl true
  def handle_cast({:publish, user}, state) do
    FraiseQL.publish_subscription("users:created", user)
    {:noreply, state}
  end
end

defmodule MyApp.Application do
  use Application

  @impl true
  def start(_type, _args) do
    children = [{MyApp.UserEvents, []}]
    Supervisor.start_link(children, strategy: :one_for_one)
  end
end
```

---

## Advanced Features

### 1. Fact Tables for Analytics

Define analytical tables with dimensions and measures:

```elixir
defmodule MyApp.Schema.Analytics do
  use FraiseQL.Schema

  deffacttable :sales_fact do
    dimension :date, :string
    dimension :product_id, :integer
    measure :amount, :decimal
    measure :quantity, :integer
    config sql_source: "fact_sales"
  end
end

# Aggregation queries
defmodule MyApp.Schema.AggregateQueries do
  use FraiseQL.Schema

  defaggregate :sales_by_product do
    from :sales_fact
    group_by :product_id
    aggregate :amount, :sum
    order_by :amount, :desc
  end
end
```

### 2. RBAC with Pattern Matching

Authorization using Elixir guards and pattern matching:

```elixir
defmodule MyApp.Authorization do
  def can_access_user?(user_id, %{role: :admin}), do: true
  def can_access_user?(user_id, %{user_id: ^user_id}), do: true
  def can_access_user?(_, _), do: false

  def can_create_user?(%{role: :admin}), do: true
  def can_create_user?(%{role: role}) when role != :anonymous, do: true
  def can_create_user?(_), do: false
end

defmodule MyApp.Schema.Security do
  use FraiseQL.Schema

  defsecurity :users_query do
    allow :admin
    allow :user, where: "user_id = context.user_id"
    deny :public
  end
end
```

### 3. Field Validation with Pipes

Chain validators using pipes and pattern matching:

```elixir
defmodule MyApp.Validators do
  def validate_user_input(input) do
    input
    |> validate_required([:email, :username])
    |> validate_email()
    |> validate_username()
  end

  defp validate_required({:error, _} = error), do: error
  defp validate_required(input) when is_map(input) do
    has_email = Map.has_key?(input, :email)
    has_username = Map.has_key?(input, :username)
    if has_email && has_username, do: {:ok, input}, else: {:error, "Missing fields"}
  end

  defp validate_email({:ok, %{email: email} = input}) do
    if String.contains?(email, "@"), do: {:ok, input}, else: {:error, "Invalid email"}
  end
  defp validate_email(error), do: error

  defp validate_username({:ok, %{username: u} = input}) when byte_size(u) >= 3 do
    {:ok, input}
  end
  defp validate_username({:ok, _}), do: {:error, "Username too short"}
  defp validate_username(error), do: error
end
```

---

## Scalar Types

### Type Mappings with Serialization

```elixir
defschema Types do
  field :count, :integer                # GraphQL: Int
  field :price, :float                  # GraphQL: Float
  field :name, :string                  # GraphQL: String
  field :active, :boolean               # GraphQL: Boolean
  field :created_at, :datetime          # GraphQL: DateTime
  field :metadata, :map                 # GraphQL: JSON
  field :tags, list_of(:string)         # GraphQL: [String]
  field :email, :string, nullable: true # Nullable String
end

# Serialization with pattern matching
def serialize(value, :datetime) when is_struct(value, DateTime) do
  DateTime.to_iso8601(value)
end

def serialize(value, :date) when is_struct(value, Date) do
  Date.to_iso8601(value)
end

def serialize(value, _type), do: value

# Deserialization
def deserialize(value, :datetime) when is_binary(value) do
  case DateTime.from_iso8601(value) do
    {:ok, dt, _} -> dt
    :error -> nil
  end
end

def deserialize(nil, _type), do: nil
def deserialize(value, _type), do: value
```

---

## Schema Export

### Export Workflow

Generate `schema.json` from Elixir module definitions:

```elixir
defmodule MyApp.SchemaExporter do
  def export! do
    FraiseQL.export_schema(
      file: "schema.json",
      modules: [
        MyApp.Schema.User,
        MyApp.Schema.Queries,
        MyApp.Schema.Mutations
      ]
    )
  end
end

# Mix task: mix FraiseQL.export
defmodule Mix.Tasks.Fraiseql.Export do
  use Mix.Task

  @impl true
  def run(_args) do
    Application.ensure_all_started(:FraiseQL)
    MyApp.SchemaExporter.export!()
    Mix.shell().info("✓ Schema exported")
  end
end
```

### FraiseQL.toml Configuration

```toml
[FraiseQL]
version = "2.0.0"

[FraiseQL.database]
primary = "postgres"
url = "${DATABASE_URL}"

[FraiseQL.security]
rate_limiting = { enabled = true, max_requests = 100 }
audit_logging = { enabled = true, log_level = "info" }
```

---

## Type Mapping

| Elixir Type | GraphQL Type |
|------------|-------------|
| `:integer` | `Int` |
| `:float` | `Float` |
| `:string` | `String` |
| `:boolean` | `Boolean` |
| `:datetime` | `DateTime` |
| `:map` | `JSON` |
| `list_of(:integer)` | `[Int]` |

---

## Common Patterns

### CRUD Operations

```elixir
defmodule MyApp.UserCRUD do
  def create_user(%{email: e, username: u} = attrs) when is_binary(e) and is_binary(u) do
    execute_mutation(:create_user, attrs)
  end
  def create_user(_), do: {:error, "Invalid input"}

  def get_user(id) do
    case execute_query(:user_by_id, %{id: id}) do
      {:ok, user} when user != nil -> {:ok, user}
      {:ok, nil} -> {:error, :not_found}
      error -> error
    end
  end

  def list_users(limit \\ 10, offset \\ 0) do
    execute_query(:users, %{limit: limit, offset: offset})
  end

  def update_user(id, updates) do
    %{id: id}
    |> Map.merge(updates)
    |> execute_mutation(:update_user)
  end
end
```

### Pagination

```elixir
defmodule MyApp.Pagination do
  def fetch_page(cursor \\ nil, limit \\ 20) do
    params = %{limit: limit + 1}
    params = if cursor, do: Map.put(params, :after, decode_cursor(cursor)), else: params

    case execute_query(:users_paginated, params) do
      {:ok, users} ->
        has_next = Enum.count(users) > limit
        items = Enum.take(users, limit)
        next_cursor = if has_next, do: encode_cursor(List.last(items)), else: nil
        {:ok, %{items: items, cursor: next_cursor}}

      error -> error
    end
  end

  defp encode_cursor(%{id: id}), do: Base.encode64(Integer.to_string(id))
  defp decode_cursor(encoded), do: Base.decode64(encoded)
end
```

### Phoenix Integration

```elixir
# GraphQL Controller
defmodule MyAppWeb.GraphQLController do
  use MyAppWeb, :controller

  def handle(conn, %{"query" => query_str, "variables" => vars}) do
    context = %{user_id: conn.assigns[:user_id], role: conn.assigns[:role]}

    case FraiseQL.execute(query_str, vars, context) do
      {:ok, result} -> json(conn, result)
      {:error, errors} -> json(conn |> put_status(400), %{errors: errors})
    end
  end
end

# lib/my_app_web/router.ex
defmodule MyAppWeb.Router do
  use MyAppWeb, :router

  scope "/api" do
    pipe_through :api
    post "/graphql", GraphQLController, :handle
  end
end
```

### Ecto Integration

```elixir
defmodule MyApp.UserOps do
  import Ecto.Query

  def fetch_users(limit, offset) do
    MyApp.User
    |> limit(^limit)
    |> offset(^offset)
    |> order_by([u], desc: u.created_at)
    |> MyApp.Repo.all()
  end

  def fetch_user(id), do: MyApp.Repo.get(MyApp.User, id)

  def create_user(attrs) do
    %MyApp.User{}
    |> MyApp.User.changeset(attrs)
    |> MyApp.Repo.insert()
  end
end

# Schema: pipe Ecto queries to GraphQL
defmodule MyApp.Schema.Queries do
  use FraiseQL.Schema

  defquery :users do
    field :limit, :integer, default: 10
    field :offset, :integer, default: 0
    returns list_of(User)
    config sql_source: "v_users"
  end

  def execute(:users, %{limit: limit, offset: offset}) do
    MyApp.UserOps.fetch_users(limit, offset)
  end
end
```

---

## Error Handling

### Pattern Matching Errors

```elixir
defmodule MyApp.Errors do
  def handle_error({:error, reason}) do
    case reason do
      {:validation, msg} -> {:error, %{type: :validation_error, message: msg}}
      {:not_found, resource} -> {:error, %{type: :not_found, message: "#{resource} not found"}}
      {:database, msg} -> {:error, %{type: :database_error, message: msg}}
      {:unauthorized, msg} -> {:error, %{type: :unauthorized, message: msg}}
      msg when is_binary(msg) -> {:error, %{type: :error, message: msg}}
    end
  end

  def create_user_safe(attrs) do
    attrs
    |> validate_attrs()
    |> case do
      {:ok, valid} -> do_create_user(valid)
      error -> error
    end
  end
end
```

### Error Recovery with GenServer

```elixir
defmodule MyApp.UserWorker do
  use GenServer
  require Logger

  def start_link(_opts), do: GenServer.start_link(__MODULE__, [], name: __MODULE__)
  def create_async(attrs), do: GenServer.cast(__MODULE__, {:create, attrs})

  @impl true
  def init(_), do: {:ok, %{}}

  @impl true
  def handle_cast({:create, attrs}, state) do
    case execute_mutation(:create_user, attrs) do
      {:ok, user} -> Logger.info("Created: #{user.id}")
      {:error, reason} -> Logger.error("Failed: #{inspect(reason)}")
    end

    {:noreply, state}
  end
end
```

---

## Testing

### ExUnit Patterns

```elixir
defmodule MyApp.UserQueryTest do
  use ExUnit.Case

  describe "User Queries" do
    test "fetches users with default limit" do
      insert_users(15)
      {:ok, users} = execute_query(:users, %{})
      assert Enum.count(users) == 10
    end

    test "respects limit parameter" do
      insert_users(20)
      {:ok, users} = execute_query(:users, %{limit: 5})
      assert Enum.count(users) == 5
    end

    test "handles pagination" do
      insert_users(30)
      {:ok, p1} = execute_query(:users, %{limit: 10, offset: 0})
      {:ok, p2} = execute_query(:users, %{limit: 10, offset: 10})
      refute Enum.any?(p1, &Enum.member?(p2, &1))
    end

    test "returns not_found for missing user" do
      assert {:error, :not_found} = execute_query(:user_by_id, %{id: 99999})
    end
  end

  describe "Mutations" do
    test "creates user with valid input" do
      assert {:ok, user} = execute_mutation(:create_user, %{
        name: "Alice",
        email: "alice@example.com"
      })

      assert user.name == "Alice"
      assert is_integer(user.id)
    end

    test "validates required fields" do
      assert {:error, reason} = execute_mutation(:create_user, %{name: "Bob"})
      assert String.contains?(reason, "email")
    end
  end
end

# Test helpers
defmodule MyApp.TestHelpers do
  def execute_query(name, params), do: FraiseQL.execute_query(name, params)
  def execute_mutation(name, params), do: FraiseQL.execute_mutation(name, params)

  def insert_user(opts \\ []) do
    MyApp.Repo.insert!(%MyApp.User{
      name: opts[:name] || "Test User",
      email: opts[:email] || "test@example.com"
    })
  end

  def insert_users(count) do
    Enum.map(1..count, fn i ->
      insert_user(name: "User #{i}", email: "user#{i}@example.com")
    end)
  end
end
```

---

## See Also

- [FraiseQL Architecture Guide](../../architecture/README.md)
- [Schema Compilation Pipeline](../../foundation/06-compilation-pipeline.md)
- [Phoenix Framework Integration](../../guides/choosing-fraiseql.md)
- [Analytics & OLAP Guide](../../guides/analytics-patterns.md)
- [Security & RBAC Guide](../../guides/authorization-quick-start.md)
- [Python Reference](./python-reference.md)
- [TypeScript Reference](./typescript-reference.md)
- [Go Reference](./go-reference.md)

---

## Troubleshooting

### Common Setup Issues

#### Mix Dependency Issues

**Issue**: `could not find dependency FraiseQL`

**Solution**:

```elixir
# mix.exs
def deps do
  [
    {:FraiseQL, "~> 2.0"},
    {:httpoison, "~> 2.0"}
  ]
end
```

```bash
mix deps.get
mix deps.update FraiseQL
```

#### Erlang Version Issues

**Issue**: `Unsupported Erlang/OTP version`

**Check version** (OTP 24+ required):

```bash
erl -version
elixir --version
```

**Update**:

```bash
asdf install erlang 26.0
asdf install elixir 1.15.0
asdf local erlang 26.0
asdf local elixir 1.15.0
```

#### Macro Issues

**Issue**: `undefined function use/1`

**Solution - Define module first**:

```elixir
defmodule MyApp.Schema do
  use FraiseQL.Schema

  type User do
    field :id, :integer
    field :email, :string
  end
end
```

#### Supervision Tree Issues

**Issue**: `connection refused` when running server

**Solution - Start properly**:

```elixir
# application.ex
def start(_type, _args) do
  children = [
    {FraiseQL.Server, [
      compiled_schema: "schema.compiled.json",
      database_url: System.get_env("DATABASE_URL")
    ]}
  ]

  opts = [strategy: :one_for_one, name: MyApp.Supervisor]
  Supervisor.start_link(children, opts)
end
```

---

### Type System Issues

#### Pattern Match Issues

**Issue**: `no function clause matching`

**Solution - Handle all patterns**:

```elixir
# ❌ Incomplete
def process({:ok, result}), do: result

# ✅ Complete
def process({:ok, result}), do: result
def process({:error, reason}), do: {:error, reason}
```

#### Type Spec Issues

**Issue**: `Type mismatch in spec: ...`

**Solution - Define proper specs**:

```elixir
# ✅ With spec
@spec execute(String.t(), map()) :: {:ok, map()} | {:error, String.t()}
def execute(query, variables) do
  # implementation
end
```

#### Macro Expansion Issues

**Issue**: `Macro undefined: type/1`

**Solution - Use correct syntax**:

```elixir
# ✅ Inside schema block
defmodule MyApp.Schema do
  use FraiseQL.Schema

  type User do
    field :id, :integer
  end
end
```

---

### Runtime Errors

#### Supervision Tree Failures

**Issue**: `GenServer ... terminating`

**Debug with observer**:

```elixir
# In iex
iex> :observer.start()
```

**Handle errors**:

```elixir
defmodule FraiseQLSupervisor do
  def start_link(opts) do
    Supervisor.start_link([
      {FraiseQL.Server, opts}
    ], strategy: :one_for_one)
  end
end
```

#### Async/Concurrency Issues

**Issue**: `Process mailbox overflow`

**Solution - Use flow control**:

```elixir
# Limit concurrency
Task.async_stream(queries, fn q ->
  FraiseQL.execute(q)
end, max_concurrency: 10)
```

#### Database Connection Issues

**Issue**: `no connections available`

**Configure pool**:

```elixir
# config/config.exs
config :FraiseQL,
  database_url: System.get_env("DATABASE_URL"),
  pool_size: 20,
  pool_overflow: 10
```

---

### Performance Issues

#### Compilation Time

**Issue**: Compilation takes >30 seconds

**Use caching**:

```bash
MIX_ENV=prod mix compile.app
```

#### Memory Usage

**Issue**: Application memory grows over time

**Monitor with observer**:

```elixir
iex> :observer.start()
```

**Clean up resources**:

```elixir
def cleanup do
  # Close connections
  FraiseQL.close()
end
```

---

### Debugging Techniques

#### Logger Setup

```elixir
# config/config.exs
config :logger,
  level: :debug,
  format: "\n$time $metadata[$level] $message\n"

# In code
require Logger
Logger.debug("Query: #{query}")
```

#### IEx Debugging

```bash
iex -S mix

iex> require Logger
iex> Logger.level(:debug)
iex> result = FraiseQL.execute(query)
```

#### Profiler

```bash
mix profile.fprof
```

---

### Getting Help

Provide:

1. Erlang version: `erl -version`
2. Elixir version: `elixir --version`
3. FraiseQL version: `mix deps`
4. Error message
5. Stack trace

---

**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community | **Status**: Production Ready ✅
