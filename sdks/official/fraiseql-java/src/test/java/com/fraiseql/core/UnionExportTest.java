package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

/**
 * A registered union reaches the exported {@code schema.json}, in the shape the compiler reads:
 * {@code {name, member_types, description?}}, ordered by name.
 */
@DisplayName("Union export")
class UnionExportTest {

    private static final ObjectMapper MAPPER = new ObjectMapper();

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    @Test
    @DisplayName("formatSchema exports every registered union, ordered by name")
    void registeredUnionsAreExported() throws Exception {
        registry.registerUnion("SearchResult", List.of("User", "Post"), "A search hit");
        registry.registerUnion("Actor", List.of("User", "Bot"), null);

        JsonNode unions = SchemaFormatter.formatSchema(registry).get("unions");

        assertEquals(
            MAPPER.readTree(
                "[{\"name\":\"Actor\",\"member_types\":[\"User\",\"Bot\"]},"
                    + "{\"name\":\"SearchResult\",\"member_types\":[\"User\",\"Post\"],"
                    + "\"description\":\"A search hit\"}]"),
            unions);
    }

    @Test
    @DisplayName("formatMinimalSchema exports registered unions too")
    void minimalSchemaExportsUnions() throws Exception {
        registry.registerUnion("Actor", List.of("User", "Bot"), "");

        JsonNode minimal = MAPPER.readTree(SchemaFormatter.formatMinimalSchema(registry));

        assertEquals(
            MAPPER.readTree("[{\"name\":\"Actor\",\"member_types\":[\"User\",\"Bot\"]}]"),
            minimal.get("unions"));
    }

    /**
     * The exported union compiles: the real {@code fraiseql compile} reads it into the compiled
     * schema with its members. Runs where {@code FRAISEQL_CLI} names the CLI binary; the
     * SDK conformance suite compiles a Java-authored union on every run besides.
     */
    @Test
    @DisplayName("an exported union compiles with the real CLI")
    void exportedUnionCompiles(@org.junit.jupiter.api.io.TempDir Path dir) throws Exception {
        String cli = System.getenv("FRAISEQL_CLI");
        assumeTrue(cli != null && !cli.isEmpty(), "FRAISEQL_CLI is not set");
        FraiseQL.clear();
        FraiseQL.registerType(UnionUser.class);
        FraiseQL.registerType(UnionBot.class);
        FraiseQL.query("users").returnType("User").returnsArray(true).sqlSource("v_user").register();
        FraiseQL.getRegistry().registerUnion("Actor", List.of("User", "Bot"), "Who acted");
        Path schema = dir.resolve("schema.json");
        Path compiled = dir.resolve("schema.compiled.json");
        FraiseQL.exportSchema(schema.toString());

        List<String> command = new ArrayList<>(
            List.of(cli, "compile", schema.toString(), "-o", compiled.toString()));
        Process process = new ProcessBuilder(command).redirectErrorStream(true).start();
        String output = new String(process.getInputStream().readAllBytes());
        assertTrue(process.waitFor(120, TimeUnit.SECONDS), "fraiseql compile timed out");
        assertEquals(0, process.exitValue(), output);

        JsonNode unions = MAPPER.readTree(Files.readString(compiled)).get("unions");
        assertEquals(
            MAPPER.readTree(
                "[{\"name\":\"Actor\",\"member_types\":[\"User\",\"Bot\"],"
                    + "\"description\":\"Who acted\"}]"),
            unions,
            output);
    }

    @GraphQLType(name = "User", sqlSource = "v_user")
    public static class UnionUser {
        @GraphQLField(type = "ID")
        public String id;
    }

    @GraphQLType(name = "Bot", sqlSource = "v_bot")
    public static class UnionBot {
        @GraphQLField(type = "ID")
        public String id;
    }

    @Test
    @DisplayName("a schema with no union has no unions key")
    void noUnionNoKey() {
        assertFalse(SchemaFormatter.formatSchema(registry).has("unions"));
        assertFalse(SchemaFormatter.formatMinimalSchema(registry).contains("\"unions\""));
    }
}
