defmodule FraiseQL.SchemaExporter do
  @moduledoc """
  Converts a `FraiseQL.Schema` module to the intermediate schema format consumed
  by `fraiseql compile`.

  ## Usage

      # Return the struct
      schema = FraiseQL.SchemaExporter.to_intermediate_schema(MyApp.Schema)

      # Return a JSON string (pretty-printed by default)
      json = FraiseQL.SchemaExporter.export(MyApp.Schema)

      # Write to file
      :ok = FraiseQL.SchemaExporter.export_to_file!(MyApp.Schema, "schema.json")

  Each schema module that uses `use FraiseQL.Schema` also gets these delegates
  injected automatically:

      MyApp.Schema.export_to_file!("schema.json")
      MyApp.Schema.to_intermediate_schema()
  """

  @doc """
  Converts a schema module into a `%FraiseQL.IntermediateSchema{}` struct.

  The module must have been compiled with `use FraiseQL.Schema`.

  ## Errors

  Raises `ArgumentError` if `module` is not a FraiseQL schema module.
  """
  @spec to_intermediate_schema(module()) :: FraiseQL.IntermediateSchema.t()
  def to_intermediate_schema(module) when is_atom(module) do
    assert_fraiseql_schema!(module)

    %FraiseQL.IntermediateSchema{
      version: "2.0.0",
      types: module.__fraiseql_types__(),
      queries: module.__fraiseql_queries__(),
      mutations: module.__fraiseql_mutations__()
    }
  end

  @doc """
  Converts a schema module to a JSON string.

  ## Options

    * `:compact` — when `true`, produces single-line JSON (default `false`)

  ## Errors

  Raises `ArgumentError` if `module` is not a FraiseQL schema module.
  """
  @spec export(module(), keyword()) :: String.t()
  def export(module, opts \\ []) when is_atom(module) do
    schema = to_intermediate_schema(module)
    map = schema_to_map(schema)

    if Keyword.get(opts, :compact, false) do
      Jason.encode!(map)
    else
      Jason.encode!(map, pretty: true)
    end
  end

  @doc """
  Exports a schema module to a JSON file at `path`.

  Parent directories are created automatically. Returns `:ok` on success.

  ## Options

    * `:compact` — when `true`, writes single-line JSON (default `false`)

  ## Errors

  Raises `ArgumentError` if `module` is not a FraiseQL schema module.
  Raises on file system errors (e.g. permission denied).
  """
  @spec export_to_file!(module(), Path.t(), keyword()) :: :ok
  def export_to_file!(module, path, opts \\ []) do
    json = export(module, opts)
    path |> Path.dirname() |> File.mkdir_p!()
    File.write!(path, json)
  end

  # ---------------------------------------------------------------------------
  # Private helpers
  # ---------------------------------------------------------------------------

  defp assert_fraiseql_schema!(module) do
    unless function_exported?(module, :__fraiseql_types__, 0) do
      raise ArgumentError,
            "#{inspect(module)} is not a FraiseQL.Schema module. " <>
              "Make sure the module uses `use FraiseQL.Schema`."
    end
  end

  defp schema_to_map(%FraiseQL.IntermediateSchema{} = s) do
    %{
      "version" => s.version,
      "types" => Enum.map(s.types, &type_to_map/1),
      "queries" => Enum.map(s.queries, &query_to_map/1),
      "mutations" => Enum.map(s.mutations, &mutation_to_map/1)
    }
  end

  defp type_to_map(%FraiseQL.TypeDefinition{} = t) do
    base = %{
      "name" => t.name,
      "sql_source" => t.sql_source,
      "fields" => Enum.map(t.fields, &field_to_map/1)
    }

    base
    |> maybe_put("description", t.description)
    |> maybe_put_bool("relay", t.relay)
    |> maybe_put_bool("is_input", t.is_input)
    |> maybe_put_bool("is_error", t.is_error)
    |> maybe_put_bool("tenant_scoped", t.tenant_scoped)
  end

  defp field_to_map(%FraiseQL.FieldDefinition{} = f) do
    base = %{
      "name" => f.name,
      "type" => f.type,
      "nullable" => f.nullable
    }

    base
    |> maybe_put("description", f.description)
    |> maybe_put("requires_scope", f.requires_scope)
    |> maybe_put("requires_scopes", f.requires_scopes)
  end

  defp query_to_map(%FraiseQL.QueryDefinition{} = q) do
    base = %{
      "name" => q.name,
      "return_type" => q.return_type,
      "returns_list" => q.returns_list,
      "nullable" => q.nullable,
      "sql_source" => q.sql_source,
      "arguments" => Enum.map(q.arguments, &argument_to_map/1)
    }

    base
    |> maybe_put("description", q.description)
    |> maybe_put("cache_ttl_seconds", q.cache_ttl_seconds)
    |> maybe_put("inject_params", q.inject_params)
    |> maybe_put_rest(q.rest_path, q.rest_method, "GET")
  end

  defp mutation_to_map(%FraiseQL.MutationDefinition{} = m) do
    base = %{
      "name" => m.name,
      "return_type" => m.return_type,
      "sql_source" => m.sql_source,
      "operation" => m.operation,
      "arguments" => Enum.map(m.arguments, &argument_to_map/1)
    }

    base
    |> maybe_put("description", m.description)
    |> maybe_put("inject_params", m.inject_params)
    |> maybe_put_rest(m.rest_path, m.rest_method, "POST")
  end

  defp argument_to_map(%FraiseQL.ArgumentDefinition{} = a) do
    base = %{
      "name" => a.name,
      "type" => a.type,
      "nullable" => a.nullable
    }

    maybe_put(base, "description", a.description)
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)

  defp maybe_put_rest(map, nil, _method, _default_method), do: map
  defp maybe_put_rest(map, path, nil, default_method), do: Map.put(map, "rest", %{"path" => path, "method" => default_method})
  defp maybe_put_rest(map, path, method, _default_method), do: Map.put(map, "rest", %{"path" => path, "method" => method})

  # Only include boolean flags in output when they are true (avoid cluttering schema.json)
  defp maybe_put_bool(map, _key, false), do: map
  defp maybe_put_bool(map, key, true), do: Map.put(map, key, true)
end
