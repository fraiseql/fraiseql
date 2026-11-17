package com.fraiseql.core;

import com.fraiseql.examples.EcommerceWithObserversSchema;
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.nio.file.Path;

/**
 * Run the SDK's observer example, so the schema it writes can be compiled.
 *
 * <p>Driven by {@code sdks/official/conformance/check_examples.sh}, which sets
 * {@code FRAISEQL_EXAMPLES_OUT} to a directory and compiles every schema left in it with
 * the real {@code fraiseql compile}. It runs as a JUnit test for the same reason
 * {@link ConformanceExport} does: the Maven build exposes no other entry point. It asserts
 * nothing; the compiler does the asserting.
 */
public class ExamplesExport {

    @Test
    void exportObserverExample() throws IOException {
        String out = System.getenv("FRAISEQL_EXAMPLES_OUT");
        if (out == null) {
            // Not being driven by the examples gate: an ordinary `mvn test` writes nothing.
            return;
        }

        FraiseQL.clear();
        EcommerceWithObserversSchema.main(
            new String[] {Path.of(out, "ecommerce_observers_schema.json").toString()});
        FraiseQL.clear();
    }
}
