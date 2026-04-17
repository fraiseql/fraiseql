# Credo configuration for FraiseQL Elixir SDK

%{
  version: 2,
  checks: [
    # Refactor checks
    {Credo.Check.Refactor.Nesting, [
      max_nesting: 3,
      excluded_macros: [:test, :test_with_setup, :assert, :refute, :assert_raise, :assert_async]
    ]},
    {Credo.Check.Refactor.UnusedParams, [exit_status: 0]},

    # Readability checks
    {Credo.Check.Readability.ModuleDoc, [exclude: ["test/"]]},

    # Design checks - ignore duplicated code in tests
    {Credo.Check.Design.DuplicatedCode, [excluded_macros: [:test, :describe, :it]]}
  ]
}
