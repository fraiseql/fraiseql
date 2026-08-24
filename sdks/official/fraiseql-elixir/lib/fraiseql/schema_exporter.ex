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
      enums: enums_of(module),
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
    # `function_exported?/3` does not load the module; ensure it is loaded first so a
    # not-yet-referenced schema module (e.g. a `test/support` fixture) is recognized.
    Code.ensure_loaded(module)

    unless function_exported?(module, :__fraiseql_types__, 0) do
      raise ArgumentError,
            "#{inspect(module)} is not a FraiseQL.Schema module. " <>
              "Make sure the module uses `use FraiseQL.Schema`."
    end
  end

  # A schema module compiled before `fraiseql_enum` existed has no accessor; treat that
  # as "no enums" rather than crashing the export.
  defp enums_of(module) do
    if function_exported?(module, :__fraiseql_enums__, 0), do: module.__fraiseql_enums__(), else: []
  end

  defp schema_to_map(%FraiseQL.IntermediateSchema{} = s) do
    base = %{
      "version" => s.version,
      "types" => Enum.map(s.types, &type_to_map/1),
      "queries" => Enum.map(s.queries, &query_to_map/1),
      "mutations" => Enum.map(s.mutations, &mutation_to_map/1)
    }

    if s.enums == [] do
      base
    else
      Map.put(base, "enums", Enum.map(s.enums, &enum_to_map/1))
    end
  end

  defp enum_to_map(%FraiseQL.EnumDefinition{} = e) do
    base = %{
      "name" => e.name,
      "values" => Enum.map(e.values, &%{"name" => &1})
    }

    maybe_put(base, "description", e.description)
  end

  defp type_to_map(%FraiseQL.TypeDefinition{} = t) do
    base = %{
      "name" => t.name,
      "fields" => Enum.map(t.fields, &field_to_map/1)
    }

    base
    # An `is_input` type carries no `sql_source`: an input object has no backing view and
    # the compiler refuses one that names a source.
    |> maybe_put("sql_source", t.sql_source)
    |> maybe_put("description", t.description)
    |> maybe_put_bool("relay", t.relay)
    |> maybe_put_bool("is_input", t.is_input)
    |> maybe_put_bool("is_error", t.is_error)
  end

  defp field_to_map(%FraiseQL.FieldDefinition{} = f) do
    base = %{
      "name" => f.name,
      "type" => f.type,
      "nullable" => f.nullable
    }

    base
    |> maybe_put("description", f.description)
    |> maybe_put("requires_scope", single_scope(f))
    |> maybe_put("vector_config", vector_config_to_map(f.vector_config))
    |> maybe_put("vector_distance", f.vector_distance)
    |> maybe_put("deprecated", deprecation_to_map(f.deprecated))
  end

  # `IntermediateField.deprecated` has been readable since #1025. `true` means deprecated
  # with no stated reason, which the compiler models as an absent `reason`; `false` and
  # `nil` drop the key rather than emitting an empty deprecation.
  defp deprecation_to_map(nil), do: nil
  defp deprecation_to_map(false), do: nil
  defp deprecation_to_map(true), do: %{}
  defp deprecation_to_map(reason) when is_binary(reason), do: %{"reason" => reason}

  # A `Vector` field without its config is refused by the compiler, so dropping this
  # would not be a silent loss — it would make the four pgvector field types unauthorable
  # in Elixir.
  defp vector_config_to_map(nil), do: nil

  defp vector_config_to_map(%FraiseQL.VectorConfig{} = config) do
    %{
      "dimensions" => config.dimensions,
      "index_type" => config.index_type,
      "distance_metric" => config.distance_metric
    }
  end

  # `requires_scopes` is a key the compiler does not read, and the compiled schema and the
  # runtime field filter represent exactly one required scope. Emitting the array produced
  # a field with no scope at all — silently public before the compiler denied unknown
  # fields, and a hard compile error after (#807). A singleton list is the same
  # requirement as a single scope and is emitted as one; anything longer is refused rather
  # than written as a declaration nothing can honour.
  defp single_scope(%FraiseQL.FieldDefinition{requires_scope: scope}) when is_binary(scope),
    do: scope

  defp single_scope(%FraiseQL.FieldDefinition{requires_scopes: nil}), do: nil
  defp single_scope(%FraiseQL.FieldDefinition{requires_scopes: []}), do: nil
  defp single_scope(%FraiseQL.FieldDefinition{requires_scopes: [scope]}), do: scope

  defp single_scope(%FraiseQL.FieldDefinition{name: name, requires_scopes: scopes}) do
    raise ArgumentError,
          "Field #{inspect(name)} requires #{length(scopes)} scopes; multiple required " <>
            "scopes are not supported — declare a single `requires_scope`."
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
    |> maybe_put_auto_params(q.auto_params)
    |> maybe_put_inject_params(q.inject_params)
    |> maybe_put("requires_role", q.requires_role)
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
    |> maybe_put_bool("cascade", m.cascade)
    |> maybe_put_inject_params(m.inject_params)
    |> maybe_put("requires_role", m.requires_role)
    |> maybe_put("invalidates_views", m.invalidates_views)
    |> maybe_put("invalidates_fact_tables", m.invalidates_fact_tables)
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

  # The wire key is `inject_params` and the value is the nested `{source, claim}` form.
  defp maybe_put_inject_params(map, nil), do: map
  defp maybe_put_inject_params(map, params) when map_size(params) == 0, do: map

  defp maybe_put_inject_params(map, params) do
    Map.put(
      map,
      "inject_params",
      Map.new(params, fn {param, source} ->
        {to_string(param), inject_source_to_map(source)}
      end)
    )
  end

  defp inject_source_to_map(source) when is_binary(source) do
    case String.split(source, ":", parts: 2) do
      [src, claim] ->
        %{"source" => src, "claim" => claim}

      _ ->
        raise ArgumentError,
              "inject_params value #{inspect(source)} must be \"<source>:<claim>\", " <>
                "for example \"jwt:tenant_id\"."
    end
  end

  defp maybe_put_rest(map, nil, _method, _default_method), do: map
  defp maybe_put_rest(map, path, nil, default_method), do: Map.put(map, "rest", %{"path" => path, "method" => default_method})
  defp maybe_put_rest(map, path, method, _default_method), do: Map.put(map, "rest", %{"path" => path, "method" => method})

  defp maybe_put_auto_params(map, nil), do: map
  defp maybe_put_auto_params(map, params) when is_map(params) do
    string_params = Map.new(params, fn {k, v} -> {to_string(k), v} end)
    Map.put(map, "auto_params", string_params)
  end

  # Only include boolean flags in output when they are true (avoid cluttering schema.json)
  defp maybe_put_bool(map, _key, false), do: map
  defp maybe_put_bool(map, key, true), do: Map.put(map, key, true)
end
