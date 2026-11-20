package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * {@code retryOnAbort} authoring (#1936): a mutation that opts out of transaction retry is
 * exported with {@code retry_on_abort: false}; one that does not declare it is exported
 * without the key, so it follows the server.
 */
@DisplayName("Mutation retry on abort")
public class RetryOnAbortTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    private static JsonNode named(JsonNode array, String name) {
        for (JsonNode item : array) {
            if (name.equals(item.get("name").asText())) {
                return item;
            }
        }
        fail(name + " is absent from " + array);
        return null;
    }

    @Test
    @DisplayName("an opt-out is exported, and an undeclared flag is left out")
    void retryOnAbortIsExportedOnlyWhenDeclared() {
        FraiseQL.mutation("chargeCard")
            .returnType("Charge")
            .sqlSource("fn_charge_card")
            .retryOnAbort(false)
            .register();
        FraiseQL.mutation("refund")
            .returnType("Charge")
            .sqlSource("fn_refund")
            .register();

        JsonNode mutations = SchemaFormatter.formatSchema(SchemaRegistry.getInstance()).get("mutations");
        JsonNode charge = named(mutations, "chargeCard").get("retry_on_abort");
        assertNotNull(charge, "the opt-out is exported");
        assertFalse(charge.asBoolean());
        assertFalse(named(mutations, "refund").has("retry_on_abort"));
    }
}
