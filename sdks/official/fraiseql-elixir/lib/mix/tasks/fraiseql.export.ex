defmodule Mix.Tasks.Fraiseql.Export do
  @shortdoc "Export schema.json from a FraiseQL schema module"

  @moduledoc """
  Exports a FraiseQL schema module to an intermediate schema JSON file.

  ## Usage

      mix fraiseql.export --module MyApp.Schema --output schema.json

  ## Options

    * `--module`, `-m` — (required) The schema module to export, e.g. `MyApp.Schema`
    * `--output`, `-o` — (optional) Output file path. Defaults to `schema.json`
    * `--compact` — (optional) Write compact (single-line) JSON instead of pretty-printed

  ## Examples

      mix fraiseql.export --module MyApp.Schema
      mix fraiseql.export --module MyApp.Schema --output priv/schema.json
      mix fraiseql.export -m MyApp.Schema -o schema.json --compact

  After exporting, run the FraiseQL compiler:

      fraiseql compile schema.json
  """

  use Mix.Task

  @switches [module: :string, output: :string, compact: :boolean]
  @aliases [m: :module, o: :output]

  @impl Mix.Task
  def run(args) do
    {opts, _rest, _invalid} = OptionParser.parse(args, strict: @switches, aliases: @aliases)
    Mix.Task.run("compile")
    do_export(opts)
  end

  defp do_export(opts) do
    module_str = Keyword.get(opts, :module) || usage_error!("--module is required")
    output = Keyword.get(opts, :output, "schema.json")
    compact = Keyword.get(opts, :compact, false)

    module = resolve_module!(module_str)

    try do
      FraiseQL.SchemaExporter.export_to_file!(module, output, compact: compact)
      Mix.shell().info("Exported schema to #{output}")
    rescue
      e in ArgumentError -> Mix.raise(e.message)
      e in File.Error -> Mix.raise("Could not write #{output}: #{e.reason}")
    end
  end

  defp resolve_module!(str) do
    module = Module.concat([str])

    case Code.ensure_loaded(module) do
      {:module, mod} ->
        mod

      {:error, :nofile} ->
        Mix.raise(
          "Module #{str} not found. Did you run mix compile?\n\n" <>
            "Make sure the module is defined and your project compiles successfully."
        )
    end
  end

  @dialyzer {:nowarn_function, usage_error!: 1}
  defp usage_error!(msg) do
    Mix.raise(
      "fraiseql.export: #{msg}\n\n" <>
        "Usage: mix fraiseql.export --module MyApp.Schema [--output schema.json] [--compact]"
    )
  end
end
