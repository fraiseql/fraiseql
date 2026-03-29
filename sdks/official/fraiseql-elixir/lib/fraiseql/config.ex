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
    lines = String.split(content, "\n")

    {base, queries, mutations, _section} =
      Enum.reduce(lines, {%{}, %{}, %{}, :root}, fn line, {base, queries, mutations, section} ->
        line = String.trim(line)

        cond do
          line == "" or String.starts_with?(line, "#") ->
            {base, queries, mutations, section}

          line == "[inject_defaults]" ->
            {base, queries, mutations, :base}

          line == "[inject_defaults.queries]" ->
            {base, queries, mutations, :queries}

          line == "[inject_defaults.mutations]" ->
            {base, queries, mutations, :mutations}

          String.starts_with?(line, "[") ->
            {base, queries, mutations, :other}

          section in [:base, :queries, :mutations] ->
            case parse_kv(line) do
              {key, value} ->
                case section do
                  :base -> {Map.put(base, key, value), queries, mutations, section}
                  :queries -> {base, Map.put(queries, key, value), mutations, section}
                  :mutations -> {base, queries, Map.put(mutations, key, value), section}
                end

              nil ->
                {base, queries, mutations, section}
            end

          true ->
            {base, queries, mutations, section}
        end
      end)

    {base, queries, mutations}
  end

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
