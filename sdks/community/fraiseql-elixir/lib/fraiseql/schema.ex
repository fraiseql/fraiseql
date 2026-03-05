defmodule FraiseQL.Schema do
  @moduledoc """
  Facade for schema management and minimal types export (TOML-based workflow)
  """

  use Agent

  def start_link(_opts) do
    Agent.start_link(fn -> %{} end, name: __MODULE__)
  end

  def register_type(name, fields, description \\ nil) do
    # Validate scope fields in all fields
    validate_field_scopes(fields, name)

    Agent.update(__MODULE__, fn types ->
      Map.put(types, name, {fields, description})
    end)
  end

  defp validate_field_scopes(fields, type_name) do
    Enum.each(fields, fn {field_name, field_config} ->
      has_scope = Map.has_key?(field_config, :requires_scope)
      has_scopes = Map.has_key?(field_config, :requires_scopes)

      # Check for conflicting scope and scopes
      if has_scope and has_scopes do
        raise FraiseQL.ScopeValidationError.exception({:conflict, field_name})
      end

      # Validate requires_scope if present
      if has_scope do
        scope = Map.get(field_config, :requires_scope)

        if !is_binary(scope) do
          raise FraiseQL.ScopeValidationError.exception({:invalid_format, field_name})
        end

        case FraiseQL.ScopeValidator.validate(scope) do
          :ok -> :ok
          {:error, msg} -> raise FraiseQL.ScopeValidationError.exception(msg)
        end
      end

      # Validate requires_scopes if present
      if has_scopes do
        scopes = Map.get(field_config, :requires_scopes)

        if !is_list(scopes) do
          raise FraiseQL.ScopeValidationError.exception({:invalid_format, field_name})
        end

        if Enum.empty?(scopes) do
          raise FraiseQL.ScopeValidationError.exception(:empty_scope)
        end

        Enum.each(scopes, fn scope ->
          if !is_binary(scope) do
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

            # Add requires_scope if present
            field =
              if Map.has_key?(field_config, :requires_scope) do
                Map.put(field, "requires_scope", Map.get(field_config, :requires_scope))
              else
                field
              end

            # Add requires_scopes if present
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
      schema |> Jason.encode!(pretty: true)
    else
      schema |> Jason.encode!()
    end
  end

  def export_types_file(output_path) do
    json = export_types(true)

    output_path
    |> Path.dirname()
    |> File.mkdir_p!()

    File.write!(output_path, json)

    types_count = Agent.get(__MODULE__, &map_size/1)

    IO.puts("✅ Types exported to #{output_path}")
    IO.puts("   Types: #{types_count}")
    IO.puts("")
    IO.puts("🎯 Next steps:")
    IO.puts("   1. fraiseql compile fraiseql.toml --types #{output_path}")
    IO.puts("   2. This merges types with TOML configuration")
    IO.puts("   3. Result: schema.compiled.json with types + all config")
  rescue
    e ->
      raise "Failed to write types file: #{output_path}"
  end

  def reset do
    Agent.update(__MODULE__, fn _types -> %{} end)
  end

  def get_type_names do
    Agent.get(__MODULE__, &Map.keys/1)
  end

  def get_type(name) do
    Agent.get(__MODULE__, fn types ->
      Map.get(types, name)
    end)
  end
end
