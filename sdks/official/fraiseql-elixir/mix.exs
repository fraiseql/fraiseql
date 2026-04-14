defmodule FraiseQL.MixProject do
  use Mix.Project

  def project do
    [
      app: :fraiseql,
      version: "2.1.6",
      elixir: "~> 1.15",
      start_permanent: Mix.env() == :prod,
      elixirc_paths: elixirc_paths(Mix.env()),
      deps: deps(),
      description: "FraiseQL Elixir SDK — schema authoring for the FraiseQL compiled GraphQL engine",
      package: package(),
      docs: docs(),
      dialyzer: [
        plt_file: {:no_warn, "priv/plts/dialyzer.plt"},
        plt_add_apps: [:mix]
      ]
    ]
  end

  defp elixirc_paths(:test), do: ["lib", "test/support"]
  defp elixirc_paths(_env), do: ["lib"]

  def application do
    [
      extra_applications: [:logger]
    ]
  end

  defp deps do
    [
      {:jason, "~> 1.4"},
      {:ex_doc, "~> 0.30", only: :dev, runtime: false},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false},
      {:dialyxir, "~> 1.4", only: [:dev], runtime: false},
      {:bypass, "~> 2.1", only: :test}
    ]
  end

  defp package do
    [
      files: ["lib", "test", "mix.exs", "README.md", "CHANGELOG.md", "LICENSE"],
      maintainers: ["FraiseQL Contributors"],
      licenses: ["Apache-2.0"],
      links: %{
        "GitHub" =>
          "https://github.com/fraiseql/fraiseql/tree/main/sdks/official/fraiseql-elixir",
        "Documentation" =>
          "https://hexdocs.pm/fraiseql"
      }
    ]
  end

  defp docs do
    [
      main: "readme",
      extras: ["README.md", "CHANGELOG.md"],
      source_url:
        "https://github.com/fraiseql/fraiseql/tree/main/sdks/official/fraiseql-elixir"
    ]
  end
end
