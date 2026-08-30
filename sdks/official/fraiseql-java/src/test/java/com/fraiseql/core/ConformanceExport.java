package com.fraiseql.core;

import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Author the cross-SDK conformance fixture with the Java SDK's public API.
 *
 * <p>Driven by {@code sdks/official/conformance/run.py}; see
 * {@code sdks/official/conformance/README.md}.
 *
 * <p>The one rule for every SDK's copy of this file: author through the SDK, never
 * hand-assemble the JSON. The pre-existing {@code GenerateParitySchema} built the
 * expected bytes with Jackson directly and never touched {@code SchemaFormatter}, which
 * is why it stayed green while <b>no Java-authored schema could be compiled at all</b>
 * (#851).
 *
 * <p>It runs as a JUnit test because that is the only entry point the Maven build exposes
 * without adding an exec plugin. It writes the schema and asserts nothing: the harness
 * compiles the output and does the asserting.
 */
public class ConformanceExport {

    @Test
    void exportConformanceFixture() throws IOException {
        String fixture = System.getenv("FRAISEQL_CONFORMANCE_FIXTURE");
        String out = System.getenv("FRAISEQL_CONFORMANCE_OUT");
        if (fixture == null || out == null) {
            // Not being driven by the harness — nothing to do. Failing here would break
            // an ordinary `mvn test`.
            return;
        }

        FraiseQL.clear();
        switch (fixture) {
            case "minimal" -> authorMinimal();
            case "full" -> authorFull();
            default -> throw new IllegalArgumentException("unknown fixture " + fixture);
        }
        FraiseQL.exportSchema(out);
    }

    private static void authorMinimal() {
        FraiseQL.registerType(ConformanceMinimalUser.class);

        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .sqlSource("v_user")
            .register();
    }

    private static void authorFull() {
        FraiseQL.registerType(ConformanceUser.class);
        FraiseQL.registerType(ConformanceOrder.class);
        FraiseQL.registerType(ConformanceSupportTicket.class);
        FraiseQL.registerErrorType(ConformanceUserNotFound.class);
        FraiseQL.registerType(ConformanceDocument.class);

        FraiseQL.getRegistry().registerEnum("OrderStatus", enumValues(), "");
        FraiseQL.getRegistry().registerInputType(
            "CreateUserInput", ConformanceCreateUserInput.class, "");

        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .sqlSource("v_user")
            .register();

        FraiseQL.query("user")
            .returnType("User")
            .nullable(true)
            .sqlSource("v_user")
            // Argument types are GraphQL type expressions: the trailing `!` is what makes
            // this argument required.
            .arg("id", "ID!")
            .register();

        Map<String, String> tenantInject = new LinkedHashMap<>();
        tenantInject.put("tenant_id", "jwt:tenant_id");
        FraiseQL.query("tenantOrders")
            .returnType("Order")
            .returnsArray(true)
            .sqlSource("v_order")
            .inject(tenantInject)
            .cacheTtlSeconds(300)
            .requiresRole("admin")
            // #966's actor allow-list, enforced in the same executor gate as requiresRole
            // on every transport, and authorable in no SDK until #1123.
            .requiresActor(ActorType.HUMAN_USER, ActorType.SERVICE_ACCOUNT)
            .register();

        FraiseQL.mutation("createUser")
            .returnType("User")
            .sqlSource("fn_create_user")
            .operation("insert")
            .arg("email", "String!")
            .arg("name", "String")
            .invalidatesViews(List.of("v_user", "v_user_summary"))
            .invalidatesFactTables(List.of("tf_signup"))
            .requiresActor(ActorType.SERVICE_ACCOUNT)
            .register();

        Map<String, String> userInject = new LinkedHashMap<>();
        userInject.put("user_id", "jwt:sub");
        FraiseQL.mutation("placeOrder")
            .returnType("Order")
            .sqlSource("fn_place_order")
            .operation("insert")
            .inject(userInject)
            .invalidatesViews(List.of("v_order_summary"))
            .invalidatesFactTables(List.of("tf_sale"))
            .register();

        FraiseQL.subscription("orderUpdated")
            .entityType("Order")
            .arg("orderId", "ID")
            .description("Stream of order update events")
            .topic("order_events")
            .filterCondition("orderId", "$.id")
            .fields("id", "total")
            .register();
    }

    private static Map<String, String> enumValues() {
        Map<String, String> values = new LinkedHashMap<>();
        values.put("PENDING", "PENDING");
        values.put("SHIPPED", "SHIPPED");
        values.put("CANCELLED", "CANCELLED");
        return values;
    }

    @GraphQLType(name = "User", sqlSource = "v_user")
    public static class ConformanceMinimalUser {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(type = "String")
        public String email;
    }

    @GraphQLType(name = "User", sqlSource = "v_user", relay = true)
    public static class ConformanceUser {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(type = "String")
        public String email;

        @GraphQLField(type = "String", nullable = true,
            description = "The user's \"display\" name", deprecated = "use displayName")
        public String name;

        @GraphQLField(type = "Float", nullable = true, requiresScope = "read:User.salary")
        public Double salary;

        // Two words and a digit segment (#1249). A Java field is idiomatically
        // camelCase and is emitted verbatim, so these match the reference as written;
        // the translation is exercised by the SDKs whose identifiers are snake_case
        // or PascalCase (Python, Ruby, Elixir, C#, F#).
        @GraphQLField(type = "String", nullable = true)
        public String lastLoginAt;

        @GraphQLField(type = "String", nullable = true)
        public String phone1;
    }

    @GraphQLType(name = "Order", sqlSource = "v_order")
    public static class ConformanceOrder {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(type = "Float")
        public Double total;

        @GraphQLField(type = "String")
        public String status;
    }

    // `crud` is an authoring-time expansion the compiler has no concept of, so the only
    // evidence this SDK implements it is that the operations and input objects appear in
    // the compiled schema. `computed` is the same: emitting the flag makes the document
    // uncompilable, so the sole evidence it was honoured is `slug` on the type and absent
    // from both input objects.
    @GraphQLType(name = "SupportTicket", sqlSource = "v_support_ticket", crud = true)
    public static class ConformanceSupportTicket {
        @GraphQLField(type = "Int")
        public int id;

        @GraphQLField(type = "String")
        public String title;

        @GraphQLField(type = "String")
        public String dueDate;

        @GraphQLField(type = "String", computed = true)
        public String slug;
    }

    @GraphQLType(name = "UserNotFound", sqlSource = "v_user_not_found")
    public static class ConformanceUserNotFound {
        @GraphQLField(type = "String")
        public String message;

        @GraphQLField(type = "String")
        public String code;
    }

    @GraphQLType(name = "Document", sqlSource = "v_document")
    public static class ConformanceDocument {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(type = Scalars.VECTOR, vector = @VectorConfig(
            dimensions = 1536,
            indexType = VectorConfig.INDEX_IVF_FLAT,
            distanceMetric = VectorConfig.METRIC_L2))
        public float[] embedding;

        @GraphQLField(type = Scalars.BIT_VECTOR, vector = @VectorConfig(
            dimensions = 768,
            distanceMetric = VectorConfig.METRIC_HAMMING))
        public String fingerprint;

        @GraphQLField(type = Scalars.HALF_VECTOR, nullable = true, vector = @VectorConfig(
            dimensions = 1536,
            distanceMetric = VectorConfig.METRIC_INNER_PRODUCT))
        public float[] compact;

        @GraphQLField(type = Scalars.SPARSE_VECTOR, nullable = true, vector = @VectorConfig(
            dimensions = 30000,
            indexType = VectorConfig.INDEX_NONE))
        public String terms;

        @GraphQLField(type = "Float", vectorDistance = "embedding")
        public float similarity;
    }

    @GraphQLType(name = "CreateUserInput")
    public static class ConformanceCreateUserInput {
        @GraphQLField(type = "String")
        public String email;

        @GraphQLField(type = "String", nullable = true)
        public String name;
    }
}
