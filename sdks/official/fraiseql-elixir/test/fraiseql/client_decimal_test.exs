defmodule FraiseQL.ClientDecimalTest do
  @moduledoc """
  A `Decimal` travels as the string of its exact digits.

  The server takes a `Decimal` argument as a string of digits with no exponent (`"1.50"`),
  or an exact integer, and refuses a number with a fraction or an exponent. The client
  encodes with Jason, which writes a `%Decimal{}` as `Decimal.to_string(d, :normal)`: every
  digit and the scale, never an exponent (`1E+3` is `"1000"`). These tests hold that.
  """
  use ExUnit.Case, async: true

  setup do
    {:ok, bypass: Bypass.open()}
  end

  # Sends `variables`; returns the variables the server received and what it echoed back.
  defp round_trip(bypass, variables) do
    test = self()

    Bypass.expect_once(bypass, "POST", "/graphql", fn conn ->
      {:ok, body, conn} = Plug.Conn.read_body(conn)
      send(test, {:sent, body})
      echo = Jason.encode!(%{data: %{echo: Jason.decode!(body)["variables"]}})

      conn
      |> Plug.Conn.put_resp_content_type("application/json")
      |> Plug.Conn.resp(200, echo)
    end)

    client = FraiseQL.Client.new("http://localhost:#{bypass.port}/graphql", timeout: 5_000)
    {:ok, data} = FraiseQL.Client.query(client, "query($a: Decimal) { echo(a: $a) }", variables: variables)
    assert_received {:sent, sent}
    {Jason.decode!(sent)["variables"], data["echo"]}
  end

  for {written, wire} <- [
        {"1.50", "1.50"},
        {"100.00", "100.00"},
        {"12345678901234567890.12", "12345678901234567890.12"},
        {"123456789012345678901234567890", "123456789012345678901234567890"},
        {"-0.00", "-0.00"},
        {"1E+3", "1000"},
        {"1.5E-3", "0.0015"}
      ] do
    test "a Decimal variable #{written} is sent as #{wire}", %{bypass: bypass} do
      {sent, echoed} = round_trip(bypass, %{"a" => Decimal.new(unquote(written))})
      assert sent == %{"a" => unquote(wire)}
      assert echoed == %{"a" => unquote(wire)}
      assert Decimal.equal?(Decimal.new(echoed["a"]), Decimal.new(unquote(written)))
    end
  end

  test "the scale survives the round trip", %{bypass: bypass} do
    {_, echoed} = round_trip(bypass, %{"a" => Decimal.new("100.00")})
    assert Decimal.new(echoed["a"]).exp == -2
  end

  test "a Decimal nested in an input object or a list is sent as a string", %{bypass: bypass} do
    {sent, _} = round_trip(bypass, %{a: %{amounts: [Decimal.new("1.50"), Decimal.new("1E+2")]}})
    assert sent == %{"a" => %{"amounts" => ["1.50", "100"]}}
  end

  test "other values are unchanged", %{bypass: bypass} do
    variables = %{"i" => 3, "f" => 0.5, "s" => "1.50", "n" => nil, "b" => true, "d" => ~D[2026-11-25]}
    {sent, _} = round_trip(bypass, variables)
    assert sent == %{"i" => 3, "f" => 0.5, "s" => "1.50", "n" => nil, "b" => true, "d" => "2026-11-25"}
  end

  test "a Decimal response is read exactly", %{bypass: bypass} do
    {_, echoed} = round_trip(bypass, %{"a" => "12345678901234567890.12"})
    assert echoed["a"] == "12345678901234567890.12"
    assert Decimal.to_string(Decimal.new(echoed["a"]), :normal) == "12345678901234567890.12"
  end

  # Not a number: Jason writes "NaN" or "Infinity", a string the server refuses as not
  # the digits of a decimal, so nothing is rounded or guessed.
  for value <- ["NaN", "Infinity", "-Infinity"] do
    test "a Decimal #{value} is sent as a string that is not digits", %{bypass: bypass} do
      {sent, _} = round_trip(bypass, %{"a" => Decimal.new(unquote(value))})
      refute sent["a"] =~ ~r/\A-?\d+(\.\d+)?\z/
    end
  end
end
