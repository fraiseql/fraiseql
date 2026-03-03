defmodule FraiseQL.MixTaskTest do
  @moduledoc false
  use ExUnit.Case

  test "mix task module exists and is a Mix.Task" do
    assert Code.ensure_loaded?(Mix.Tasks.Fraiseql.Export)
    assert Mix.Task.task?(Mix.Tasks.Fraiseql.Export)
  end

  test "exports fixture schema to a temp file" do
    path = "/tmp/fraiseql_mix_task_test_#{:os.getpid()}.json"
    Mix.Tasks.Fraiseql.Export.run([
      "--module", "FraiseQL.Test.FixtureSchema",
      "--output", path
    ])
    assert File.exists?(path)
    parsed = path |> File.read!() |> Jason.decode!()
    assert parsed["version"] == "2.0.0"
    assert length(parsed["types"]) == 2
    File.rm!(path)
  end

  test "raises Mix.Error when --module is omitted" do
    assert_raise Mix.Error, ~r/--module is required/, fn ->
      Mix.Tasks.Fraiseql.Export.run([])
    end
  end

  test "raises Mix.Error when module does not exist" do
    assert_raise Mix.Error, ~r/not found/, fn ->
      Mix.Tasks.Fraiseql.Export.run([
        "--module", "DoesNotExist.Schema",
        "--output", "/tmp/noop_#{:os.getpid()}.json"
      ])
    end
  end

  test "raises Mix.Error when module is not a FraiseQL schema" do
    assert_raise Mix.Error, ~r/not a FraiseQL\.Schema module/, fn ->
      Mix.Tasks.Fraiseql.Export.run([
        "--module", "String",
        "--output", "/tmp/noop_#{:os.getpid()}.json"
      ])
    end
  end

  test "compact flag produces single-line output" do
    path = "/tmp/fraiseql_mix_compact_#{:os.getpid()}.json"
    Mix.Tasks.Fraiseql.Export.run([
      "--module", "FraiseQL.Test.FixtureSchema",
      "--output", path,
      "--compact"
    ])
    content = File.read!(path)
    refute String.contains?(content, "\n")
    File.rm!(path)
  end

  test "default output is schema.json when --output is omitted" do
    cwd = File.cwd!()
    default_path = Path.join(cwd, "schema.json")
    File.rm(default_path)

    try do
      Mix.Tasks.Fraiseql.Export.run(["--module", "FraiseQL.Test.FixtureSchema"])
      assert File.exists?(default_path)
    after
      File.rm(default_path)
    end
  end
end
