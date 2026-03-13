defmodule FraiseQL.Schema.Legacy do
  @moduledoc """
  Legacy Agent-based schema registration API (v1.x).

  This module is preserved for backward compatibility. New code should use
  `use FraiseQL.Schema` with the declarative macro DSL instead.

  The Agent-based approach requires the process to be started before
  registration and uses global mutable state, which means tests must call
  `reset/0` between runs. The new macro DSL avoids both problems.
  """

  use Agent

  @doc """
  Starts the schema registry Agent.
  """
  @spec start_link(keyword()) :: Agent.on_start()
  def start_link(_opts) do
    Agent.start_link(fn -> %{} end, name: __MODULE__)
  end

  @doc """
  Registers a type with its fields and optional description.
  """
  @spec register_type(String.t(), map(), String.t() | nil) :: :ok
  def register_type(name, fields, description \\ nil) do
    validate_field_scopes(fields, name)

    Agent.update(__MODULE__, fn types ->
      Map.put(types, name, {fields, description})
    end)
  end

  @doc """
  Exports all registered types as a JSON string.

  Pass `pretty: true` (the default) for indented output,
  or `pretty: false` for compact single-line JSON.
  """
  @spec export_types(boolean()) :: String.t()
  def export_types(pretty \\ true) do
    types = Agent.get(__MODULE__, & &1)

    types_array =
      Enum.map(types, fn {name, {fields, description}} ->
        fields_array =
          Enum.map(fields, fn {field_name, field_config} ->
            field = %{
              "name" => field_name,
              "type" => Map.get(field_config, :type, "String"),
              "nullable" => Map.get(field_config, :nullable, false)
            }

            field =
              if Map.has_key?(field_config, :requires_scope) do
                Map.put(field, "requires_scope", Map.get(field_config, :requires_scope))
              else
                field
              end

            if Map.has_key?(field_config, :requires_scopes) do
              Map.put(field, "requires_scopes", Map.get(field_config, :requires_scopes))
            else
              field
            end
          end)

        type_obj = %{
          "name" => name,
          "fields" => fields_array
        }

        if description do
          Map.put(type_obj, "description", description)
        else
          type_obj
        end
      end)

    schema = %{"types" => types_array}

    if pretty do
      Jason.encode!(schema, pretty: true)
    else
      Jason.encode!(schema)
    end
  end

  @doc """
  Exports all registered types to a file at `output_path`.
  """
  @spec export_types_file(Path.t()) :: :ok
  def export_types_file(output_path) do
    json = export_types(true)

    output_path
    |> Path.dirname()
    |> File.mkdir_p!()

    File.write!(output_path, json)

    types_count = Agent.get(__MODULE__, &map_size/1)

    IO.puts("Exported #{types_count} type(s) to #{output_path}")
  rescue
    _e -> raise "Failed to write types file: #{output_path}"
  end

  @doc """
  Resets the schema registry, removing all registered types.
  """
  @spec reset() :: :ok
  def reset do
    Agent.update(__MODULE__, fn _types -> %{} end)
  end

  @doc """
  Returns the names of all registered types.
  """
  @spec get_type_names() :: [String.t()]
  def get_type_names do
    Agent.get(__MODULE__, &Map.keys/1)
  end

  @doc """
  Returns the field map and description for the named type, or `nil` if not found.
  """
  @spec get_type(String.t()) :: {map(), String.t() | nil} | nil
  def get_type(name) do
    Agent.get(__MODULE__, fn types ->
      Map.get(types, name)
    end)
  end

  defp validate_field_scopes(fields, _type_name) do
    Enum.each(fields, fn {field_name, field_config} ->
      has_scope = Map.has_key?(field_config, :requires_scope)
      has_scopes = Map.has_key?(field_config, :requires_scopes)

      if has_scope and has_scopes do
        raise FraiseQL.ScopeValidationError.exception({:conflict, field_name})
      end

      if has_scope do
        scope = Map.get(field_config, :requires_scope)

        unless is_binary(scope) do
          raise FraiseQL.ScopeValidationError.exception({:invalid_format, field_name})
        end

        case FraiseQL.ScopeValidator.validate(scope) do
          :ok -> :ok
          {:error, msg} -> raise FraiseQL.ScopeValidationError.exception(msg)
        end
      end

      if has_scopes do
        scopes = Map.get(field_config, :requires_scopes)

        unless is_list(scopes) do
          raise FraiseQL.ScopeValidationError.exception({:invalid_format, field_name})
        end

        if Enum.empty?(scopes) do
          raise FraiseQL.ScopeValidationError.exception(:empty_scope)
        end

        Enum.each(scopes, fn scope ->
          unless is_binary(scope) do
            raise FraiseQL.ScopeValidationError.exception({:invalid_format, field_name})
          end

          case FraiseQL.ScopeValidator.validate(scope) do
            :ok -> :ok
            {:error, msg} -> raise FraiseQL.ScopeValidationError.exception(msg)
          end
        end)
      end
    end)
  end
end
