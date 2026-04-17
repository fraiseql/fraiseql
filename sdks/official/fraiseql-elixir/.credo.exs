# Credo configuration for FraiseQL Elixir SDK

%{
  version: 2,
  checks: [
    # Consistency checks
    {Credo.Check.Consistency.ExceptionNames, []},
    {Credo.Check.Consistency.LineEndings, []},
    {Credo.Check.Consistency.ParameterPatternMatching, []},
    {Credo.Check.Consistency.SpaceAroundOperators, []},
    {Credo.Check.Consistency.SpaceInParentheses, []},
    {Credo.Check.Consistency.TabsOrSpaces, []},

    # Design checks
    {Credo.Check.Design.AliasUsage, [priority: :low, if_nested_deeper_than: 2, if_called_more_often_than: 0]},
    {Credo.Check.Design.DuplicatedCode, [excluded_macros: [:test]]},
    {Credo.Check.Design.IfThenElse, []},
    {Credo.Check.Design.TagFQN, [all: true]},
    {Credo.Check.Design.TagTODO, [exit_status: 0]},

    # Readability checks
    {Credo.Check.Readability.AliasOrder, []},
    {Credo.Check.Readability.FunctionNames, []},
    {Credo.Check.Readability.LargeNumbers, []},
    {Credo.Check.Readability.MaxLineLength, [priority: :low, max_length: 120]},
    {Credo.Check.Readability.ModuleAttributeNames, []},
    {Credo.Check.Readability.ModuleDoc, [exclude: ["test/"]]},
    {Credo.Check.Readability.ModuleNames, []},
    {Credo.Check.Readability.ParenthesesInCondition, []},
    {Credo.Check.Readability.ParenthesesOnZeroArityDefs, []},
    {Credo.Check.Readability.PipeIntoAnonymousFunctions, []},
    {Credo.Check.Readability.PredicateFunctionNames, []},
    {Credo.Check.Readability.RedundantBlankLines, []},
    {Credo.Check.Readability.Semicolons, []},
    {Credo.Check.Readability.SeparatedComments, []},
    {Credo.Check.Readability.SingleFunctionToBlockNotation, []},
    {Credo.Check.Readability.SinglePipe, []},
    {Credo.Check.Readability.SpaceAfterCommas, []},
    {Credo.Check.Readability.TrailingBlankLine, []},
    {Credo.Check.Readability.TrailingWhiteSpace, []},
    {Credo.Check.Readability.UnnecessaryAliasExpansion, []},
    {Credo.Check.Readability.VariableNames, []},

    # Refactor checks
    {Credo.Check.Refactor.AppendSingleItem, []},
    {Credo.Check.Refactor.CondReductions, []},
    {Credo.Check.Refactor.CyclomaticComplexity, []},
    {Credo.Check.Refactor.FunctionArity, [max_arity: 8, excluded_macros: []]},
    {Credo.Check.Refactor.LongQuoteBlocks, []},
    {Credo.Check.Refactor.MapInto, []},
    {Credo.Check.Refactor.MatchInCondition, []},
    {Credo.Check.Refactor.NegatedConditionsInUnless, []},
    {Credo.Check.Refactor.NegatedConditionsWithElse, []},
    {Credo.Check.Refactor.Nesting, [
      max_nesting: 4,
      excluded_macros: [:test, :test_with_setup, :assert, :refute, :assert_raise, :assert_async]
    ]},
    {Credo.Check.Refactor.PipeChainStart, [
      excluded_argument_types: [:atom, :binary, :fn, :keyword, :number, :sigil],
      excluded_macro_source: nil
    ]},
    {Credo.Check.Refactor.RedundantWithFileRead, []},
    {Credo.Check.Refactor.UnlessWithElse, []},
    {Credo.Check.Refactor.UnreachableCode, []},
    {Credo.Check.Refactor.UnusedParams, [exit_status: 0]},

    # Warning checks
    {Credo.Check.Warning.ApplicationConfigInExceptionHandler, []},
    {Credo.Check.Warning.BoolOperationOnSameValues, []},
    {Credo.Check.Warning.ExpressionsAsConditions, []},
    {Credo.Check.Warning.IExPry, []},
    {Credo.Check.Warning.IoInspect, [excluded_macro_source: "test/"]},
    {Credo.Check.Warning.LazyLogging, [excluded_macro_source: nil]},
    {Credo.Check.Warning.MixEnv, []},
    {Credo.Check.Warning.OperationOnSameValues, []},
    {Credo.Check.Warning.OperationWithConstantResult, []},
    {Credo.Check.Warning.RaiseInsideRescue, []},
    {Credo.Check.Warning.UnusedEnumOperation, []},
    {Credo.Check.Warning.UnusedFileOperation, []},
    {Credo.Check.Warning.UnusedKeywordOperation, []},
    {Credo.Check.Warning.UnusedListOperation, []},
    {Credo.Check.Warning.UnusedPathOperation, []},
    {Credo.Check.Warning.UnusedPipe, []},
    {Credo.Check.Warning.UnusedStringOperation, []},
    {Credo.Check.Warning.UnusedTupleOperation, []},
    {Credo.Check.Warning.UnsafeExec, []}
  ]
}
