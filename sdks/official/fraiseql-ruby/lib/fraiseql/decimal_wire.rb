# frozen_string_literal: true

module FraiseQL
  # How a Decimal variable travels: as a string of its digits, with no exponent.
  #
  # The server takes a Decimal argument as such a string ("1.50"), or an exact integer,
  # and refuses a number with a fraction or an exponent. JSON.generate writes a BigDecimal
  # as its default to_s, "0.15e1", which the server refuses; to_s("F") is "1.5".
  #
  # A BigDecimal holds a value, not a scale, so the value is sent exactly and its scale
  # is not: BigDecimal("100.00") is sent as "100.0". Send a String to keep a scale.
  #
  # BigDecimal is not required here: since Ruby 3.4 it is a bundled gem, and a caller
  # that holds one has loaded it.
  module DecimalWire
    module_function

    # @param value [Object] a variables tree
    # @return [Object] the same tree, each BigDecimal replaced by its digits
    # @raise [ArgumentError] a BigDecimal is NaN or infinite
    def encodable(value)
      case value
      when Hash then value.transform_values { |v| encodable(v) }
      when Array then value.map { |v| encodable(v) }
      else
        defined?(::BigDecimal) && value.is_a?(::BigDecimal) ? digits(value) : value
      end
    end

    def digits(value)
      raise ArgumentError, "a Decimal variable must be finite, got #{value}" unless value.finite?

      value.to_s("F")
    end
  end
end
