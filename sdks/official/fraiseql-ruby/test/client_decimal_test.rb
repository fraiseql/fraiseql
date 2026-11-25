# frozen_string_literal: true

require_relative "test_helper"
require "bigdecimal"
require "socket"

# A Decimal travels as the string of its digits. The server takes a Decimal argument as a
# string of digits with no exponent ("1.50"), or an exact integer, and refuses a number
# with a fraction. JSON.generate writes a BigDecimal as "0.15e1", which the server
# refuses, so the client writes it as to_s("F").
#
# A BigDecimal holds a value, not a scale: BigDecimal("100.00") and BigDecimal("100") are
# the same object state, so the value is sent exactly and the scale is not kept.
class ClientDecimalTest < Minitest::Test
  # One request on a local socket: records the body, answers with its variables.
  def round_trip(variables)
    server = TCPServer.new("127.0.0.1", 0)
    sent = nil
    thread = Thread.new do
      socket = server.accept
      length = 0
      while (line = socket.gets) && line != "\r\n"
        length = line.split(":", 2).last.to_i if line.downcase.start_with?("content-length:")
      end
      sent = socket.read(length)
      body = JSON.generate({ data: { echo: JSON.parse(sent)["variables"] } })
      socket.write("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n" \
                   "Content-Length: #{body.bytesize}\r\nConnection: close\r\n\r\n#{body}")
      socket.close
    end
    client = FraiseQL::Client.new("http://127.0.0.1:#{server.addr[1]}/graphql", timeout: 5)
    data = client.query("query($a: Decimal) { echo(a: $a) }", variables: variables)
    thread.join
    [JSON.parse(sent)["variables"], data["echo"]]
  ensure
    server&.close
  end

  {
    "1.50" => "1.5",
    "100.00" => "100.0",
    "12345678901234567890.12" => "12345678901234567890.12",
    "123456789012345678901234567890" => "123456789012345678901234567890.0",
    "-0.0015" => "-0.0015",
    "1E+3" => "1000.0",
  }.each_with_index do |(written, wire), i|
    define_method("test_a_bigdecimal_variable_is_sent_as_a_string_of_digits_#{i}") do
      sent, echoed = round_trip({ "a" => BigDecimal(written) })
      assert_equal({ "a" => wire }, sent)
      assert_match(/\A-?\d+(\.\d+)?\z/, sent["a"], "the server's Decimal digits: no exponent")
      assert_equal BigDecimal(written), BigDecimal(echoed["a"]), "the value is exact"
    end
  end

  def test_the_scale_is_not_kept_because_a_bigdecimal_holds_none
    sent, = round_trip({ "a" => BigDecimal("100.00") })
    assert_equal "100.0", sent["a"]
    assert_equal BigDecimal("100.00"), BigDecimal(sent["a"])
  end

  def test_a_bigdecimal_nested_in_an_input_object_is_sent_as_a_string
    sent, = round_trip({ "a" => { "amounts" => [BigDecimal("1.50"), BigDecimal("2")] } })
    assert_equal({ "a" => { "amounts" => ["1.5", "2.0"] } }, sent)
  end

  def test_other_values_are_unchanged
    sent, = round_trip({ "i" => 3, "f" => 0.5, "s" => "1.50", "n" => nil, "b" => true })
    assert_equal({ "i" => 3, "f" => 0.5, "s" => "1.50", "n" => nil, "b" => true }, sent)
  end

  def test_a_decimal_response_is_read_exactly
    _, echoed = round_trip({ "a" => "12345678901234567890.12" })
    assert_equal "12345678901234567890.12", echoed["a"]
    assert_equal "12345678901234567890.12", BigDecimal(echoed["a"]).to_s("F")
  end

  %w[NaN Infinity -Infinity].each do |value|
    define_method("test_a_bigdecimal_that_is_not_finite_is_refused_#{value.delete('-')}#{value.start_with?('-') ? '_neg' : ''}") do
      error = assert_raises(ArgumentError) do
        FraiseQL::Client.new("http://127.0.0.1:9/graphql")
                        .query("query($a: Decimal) { echo(a: $a) }", variables: { "a" => BigDecimal(value) })
      end
      assert_match(/finite/, error.message)
    end
  end
end
