defmodule FraiseQL.MixProject do
  use Mix.Project

  def project do
    [
      app: :fraiseql,
      version: "1.0.0",
      elixir: "~> 1.14",
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      description: "FraiseQL Elixir - Security module with 100% feature parity",
      package: package(),
      docs: docs()
    ]
  end

  def application do
    [
      extra_applications: [:logger]
    ]
  end

  defp deps do
    [
      {:jason, "~> 1.4"},
      {:ex_doc, "~> 0.30", only: :dev, runtime: false},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false}
    ]
  end

  defp package do
    [
      files: ["lib", "test", "mix.exs", "README.md", "LICENSE"],
      maintainers: ["FraiseQL Contributors"],
      licenses: ["Apache-2.0"],
      links: %{
        "GitHub" => "https://github.com/fraiseql/fraiseql",
        "Documentation" => "https://github.com/fraiseql/fraiseql/tree/main/fraiseql-elixir"
      }
    ]
  end

  defp docs do
    [
      main: "readme",
      extras: ["README.md"],
      source_url: "https://github.com/fraiseql/fraiseql/tree/main/fraiseql-elixir"
    ]
  end
end
