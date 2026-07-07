defmodule FraiseQL.Config do
  @moduledoc """
  Loads `fraiseql.toml` and extracts `inject_defaults` configuration.

  The `[inject_defaults]` TOML section defines parameters that are automatically
  injected into queries and mutations at compile time. Subsections
  `[inject_defaults.queries]` and `[inject_defaults.mutations]` override the
  base defaults for their respective operation types.

  ## Example TOML

      [inject_defaults]
      tenant_id = "jwt:tenant_id"

      [inject_defaults.queries]
      user_id = "jwt:sub"

      [inject_defaults.mutations]
      actor_id = "jwt:sub"
  """

  @doc """
  Parse `inject_defaults` from a TOML file.

  Returns a `{base, queries, mutations}` tuple where each element is a map of
  `%{param_name => "source:path"}` strings.

  ## Errors

  Raises if the file cannot be read.
  """
  @spec load_inject_defaults(Path.t()) :: {map(), map(), map()}
  def load_inject_defaults(toml_path \\ "fraiseql.toml") do
    content = File.read!(toml_path)
    parse_inject_defaults(content)
  end

  @doc false
  @spec parse_inject_defaults(String.t()) :: {map(), map(), map()}
  def parse_inject_defaults(content) do
    {base, queries, mutations, _section} =
      content
      |> String.split("\n")
      |> Enum.reduce({%{}, %{}, %{}, :root}, &reduce_inject_line/2)

    {base, queries, mutations}
  end

  # Fold one config line into the {base, queries, mutations, section} accumulator.
  defp reduce_inject_line(line, {base, queries, mutations, _section} = acc) do
    case String.trim(line) do
      "" -> acc
      "#" <> _ -> acc
      "[inject_defaults]" -> {base, queries, mutations, :base}
      "[inject_defaults.queries]" -> {base, queries, mutations, :queries}
      "[inject_defaults.mutations]" -> {base, queries, mutations, :mutations}
      "[" <> _ -> {base, queries, mutations, :other}
      trimmed -> put_inject_kv(trimmed, acc)
    end
  end

  # Parse and store a `key = value` line, but only inside a recognized section.
  defp put_inject_kv(line, {base, queries, mutations, section} = acc)
       when section in [:base, :queries, :mutations] do
    case parse_kv(line) do
      {key, value} -> put_section_kv(section, key, value, base, queries, mutations)
      nil -> acc
    end
  end

  defp put_inject_kv(_line, acc), do: acc

  defp put_section_kv(:base, k, v, base, q, m), do: {Map.put(base, k, v), q, m, :base}
  defp put_section_kv(:queries, k, v, base, q, m), do: {base, Map.put(q, k, v), m, :queries}
  defp put_section_kv(:mutations, k, v, base, q, m), do: {base, q, Map.put(m, k, v), :mutations}

  defp parse_kv(line) do
    case String.split(line, "=", parts: 2) do
      [key, value] ->
        key = String.trim(key)
        value = value |> String.trim() |> String.trim("\"")
        {key, value}

      _ ->
        nil
    end
  end
end
