package io.fraiseql;

import com.fasterxml.jackson.core.JsonGenerator;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializerProvider;
import com.fasterxml.jackson.databind.module.SimpleModule;
import com.fasterxml.jackson.databind.ser.std.StdSerializer;
import java.io.IOException;
import java.math.BigDecimal;

/**
 * How a {@code Decimal} travels: as a JSON string of its exact digits.
 *
 * <p>The server takes a {@code Decimal} argument as a string such as {@code "1.50"}, or an
 * exact integer, and refuses a number with a fraction or an exponent. Jackson writes a
 * {@link BigDecimal} as a JSON number by default, so the client's mapper writes it as
 * {@link BigDecimal#toPlainString()} instead: every digit and the scale, never an exponent
 * ({@code 1E+3} is {@code "1000"}). Reading needs nothing: Jackson reads a JSON string
 * into a {@link BigDecimal} exactly, scale included.
 */
final class DecimalWire {

    private DecimalWire() {
    }

    /** The mapper the client sends and reads with. */
    static ObjectMapper newMapper() {
        SimpleModule module = new SimpleModule("fraiseql-decimal");
        module.addSerializer(BigDecimal.class, new PlainDigits());
        return new ObjectMapper().registerModule(module);
    }

    private static final class PlainDigits extends StdSerializer<BigDecimal> {
        private static final long serialVersionUID = 1L;

        PlainDigits() {
            super(BigDecimal.class);
        }

        @Override
        public void serialize(BigDecimal value, JsonGenerator gen, SerializerProvider provider)
                throws IOException {
            gen.writeString(value.toPlainString());
        }
    }
}
