package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;
import java.util.stream.Collectors;
import java.util.stream.Stream;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;

import static org.junit.jupiter.api.Assertions.*;

/**
 * The options an input field can and cannot declare: one that configures an output field
 * is refused on a registered input type, naming the type, the field and every such option,
 * and is left off the input types {@code crud} generates from a type's own fields.
 */
@DisplayName("Field options on an input field")
public class InputFieldOptionsTest {

    /**
     * One refused option: the {@code GraphQLFieldInfo} member that carries it, the name it
     * has in the exported document, and an input class declaring it on {@code note}.
     */
    record Refused(String member, String wireName, Class<?> inputClass) {
        @Override
        public String toString() {
            return wireName;
        }
    }

    /** Every option refused on an input field. */
    static final List<Refused> REFUSED = List.of(
        new Refused("requiresScope", "requires_scope", ScopeInput.class),
        new Refused("requiresScopes", "requires_scopes", ScopesInput.class),
        new Refused("computed", "computed", ComputedInput.class),
        new Refused("vectorConfig", "vector_config", VectorInput.class),
        new Refused("vectorDistance", "vector_distance", DistanceInput.class),
        new Refused("scopedLikeParent", "scoped_like_parent", ScopedLikeParentInput.class));

    /** Every member an input field keeps. */
    static final Set<String> ALLOWED = Set.of(
        "name", "type", "nullable", "description", "isDeprecated", "deprecationReason",
        "isList", "localized");

    /** The exported keys no input field carries. */
    static final List<String> OUTPUT_ONLY_KEYS = List.of(
        "requires_scope", "requires_scopes", "computed", "vector_config", "vector_distance",
        "scoped_like_parent");

    static Stream<Refused> refused() {
        return REFUSED.stream();
    }

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

    private static JsonNode field(JsonNode schema, String section, String type, String name) {
        return named(named(schema.get(section), type).get("fields"), name);
    }

    private static boolean hasField(JsonNode schema, String section, String type, String name) {
        for (JsonNode item : named(schema.get(section), type).get("fields")) {
            if (name.equals(item.get("name").asText())) {
                return true;
            }
        }
        return false;
    }

    private static void assertNoOutputOnlyKey(JsonNode schema, String inputType) {
        for (JsonNode inputField : named(schema.get("input_types"), inputType).get("fields")) {
            for (String key : OUTPUT_ONLY_KEYS) {
                assertFalse(inputField.has(key), inputType + " carries " + key + ": " + inputField);
            }
        }
    }

    @ParameterizedTest(name = "{0} is refused on an input field")
    @MethodSource("refused")
    void anOutputOnlyOptionIsRefused(Refused option) {
        IllegalArgumentException refusal = assertThrows(IllegalArgumentException.class,
            () -> FraiseQL.getRegistry().registerInputType("OrderInput", option.inputClass(), ""));
        assertEquals(
            "OrderInput.note: " + option.wireName()
                + " applies to output fields only; an input field cannot declare it",
            refusal.getMessage());
        assertTrue(FraiseQL.getRegistry().getInputType("OrderInput").isEmpty(),
            "a refused input type is not registered");
    }

    @Test
    @DisplayName("every refused option on one field is named, sorted")
    void everyRefusedOptionOnAFieldIsNamed() {
        IllegalArgumentException refusal = assertThrows(IllegalArgumentException.class,
            () -> FraiseQL.getRegistry().registerInputType("OrderInput", TwoOptionsInput.class, ""));
        assertEquals(
            "OrderInput.note: requires_scope, vector_config applies to output fields only; "
                + "an input field cannot declare it",
            refusal.getMessage());
        assertTrue(FraiseQL.getRegistry().getInputType("OrderInput").isEmpty());
    }

    @Test
    @DisplayName("the refusal applies to fields given as a map, on any field")
    void aFieldMapIsRefusedToo() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new LinkedHashMap<>();
        fields.put("id", new TypeConverter.GraphQLFieldInfo("id", "ID", false, ""));
        fields.put("total", new TypeConverter.GraphQLFieldInfo(
            "total", "Float", false, "", null, new String[]{"read:Order.total"}, true));

        IllegalArgumentException refusal = assertThrows(IllegalArgumentException.class,
            () -> FraiseQL.getRegistry().registerInputType("OrderInput", fields, ""));
        assertEquals(
            "OrderInput.total: computed, requires_scopes applies to output fields only; "
                + "an input field cannot declare it",
            refusal.getMessage());
        assertTrue(FraiseQL.getRegistry().getInputType("OrderInput").isEmpty());
    }

    @Test
    @DisplayName("description, deprecated and localized stay on an input field")
    void inputOptionsAreAccepted() {
        FraiseQL.getRegistry().registerInputType("OrderInput", AcceptedInput.class, "");

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        JsonNode note = field(schema, "input_types", "OrderInput", "note");
        assertEquals("A note", note.get("description").asText());
        assertEquals("use memo", note.get("deprecated").get("reason").asText());
        assertTrue(note.get("localized").asBoolean());
    }

    @Test
    @DisplayName("every field member is either refused on an input field or kept on it")
    void everyFieldMemberIsClassified() {
        Set<String> refusedMembers = REFUSED.stream().map(Refused::member).collect(Collectors.toSet());
        Set<String> members = new TreeSet<>();
        for (Field member : TypeConverter.GraphQLFieldInfo.class.getDeclaredFields()) {
            if (member.isSynthetic() || Modifier.isStatic(member.getModifiers())) {
                continue;
            }
            members.add(member.getName());
            boolean refused = refusedMembers.contains(member.getName());
            boolean allowed = ALLOWED.contains(member.getName());
            assertTrue(refused != allowed,
                "GraphQLFieldInfo." + member.getName() + " is in "
                    + (refused ? "both sets" : "neither set")
                    + ": decide whether an input field may declare it. If it configures an "
                    + "output field, refuse it in SchemaRegistry.registerInputType, strip it "
                    + "in CrudGenerator, and add it to REFUSED; otherwise add it to ALLOWED.");
        }
        Set<String> classified = new TreeSet<>(refusedMembers);
        classified.addAll(ALLOWED);
        assertEquals(members, classified, "a classified member no longer exists");
    }

    @Test
    @DisplayName("the input types crud generates carry none of them; the type keeps them")
    void generatedInputTypesCarryNoOutputOnlyOption() {
        FraiseQL.registerType(Document.class);

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        assertEquals("read:Document.note",
            field(schema, "types", "Document", "note").get("requires_scope").asText());
        assertEquals("read:Document.owner",
            field(schema, "types", "Document", "owner").get("requires_scope").asText());
        assertEquals(3,
            field(schema, "types", "Document", "embedding").get("vector_config")
                .get("dimensions").asInt());
        assertEquals("embedding",
            field(schema, "types", "Document", "distance").get("vector_distance").asText());
        assertTrue(hasField(schema, "types", "Document", "slug"));

        assertNoOutputOnlyKey(schema, "CreateDocumentInput");
        assertNoOutputOnlyKey(schema, "UpdateDocumentInput");
        for (String inputType : List.of("CreateDocumentInput", "UpdateDocumentInput")) {
            for (String kept : List.of("id", "note", "owner", "embedding", "distance")) {
                assertTrue(hasField(schema, "input_types", inputType, kept), inputType + "." + kept);
            }
            assertFalse(hasField(schema, "input_types", inputType, "slug"),
                "a computed field is not an input");
        }
    }

    @Test
    @DisplayName("the generated input types keep description, deprecation, localized and nullability")
    void generatedInputTypesKeepInputOptions() {
        FraiseQL.registerType(Article.class);

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        JsonNode created = field(schema, "input_types", "CreateArticleInput", "title");
        assertEquals("String", created.get("type").asText());
        assertFalse(created.get("nullable").asBoolean());
        assertEquals("The title", created.get("description").asText());
        assertEquals("use headline", created.get("deprecated").get("reason").asText());
        assertTrue(created.get("localized").asBoolean());
        assertFalse(created.has("requires_scope"));

        JsonNode updated = field(schema, "input_types", "UpdateArticleInput", "title");
        assertTrue(updated.get("nullable").asBoolean(), "an update leaves any field out");
        assertEquals("The title", updated.get("description").asText());
        assertEquals("use headline", updated.get("deprecated").get("reason").asText());
        assertTrue(updated.get("localized").asBoolean());
        assertFalse(updated.has("requires_scope"));

        assertEquals("[String!]",
            field(schema, "input_types", "UpdateArticleInput", "tags").get("type").asText());
        assertFalse(field(schema, "input_types", "UpdateArticleInput", "id").get("nullable").asBoolean());
    }

    @Test
    @DisplayName("a crud type whose key is computed and scoped still registers")
    void aComputedScopedKeyRegisters() {
        FraiseQL.registerType(Ledger.class);

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        assertEquals("read:Ledger.id",
            field(schema, "types", "Ledger", "id").get("requires_scope").asText());
        assertFalse(hasField(schema, "input_types", "CreateLedgerInput", "id"));
        assertTrue(hasField(schema, "input_types", "UpdateLedgerInput", "id"));
        assertNoOutputOnlyKey(schema, "CreateLedgerInput");
        assertNoOutputOnlyKey(schema, "UpdateLedgerInput");
    }

    /** Declares a required scope. */
    @GraphQLType(name = "OrderInput")
    public static class ScopeInput {
        @GraphQLField(requiresScope = "read:Order.note")
        public String note;
    }

    /** Declares the plural spelling of a required scope. */
    @GraphQLType(name = "OrderInput")
    public static class ScopesInput {
        @GraphQLField(requiresScopes = {"read:Order.note"})
        public String note;
    }

    /** Declares a computed field. */
    @GraphQLType(name = "OrderInput")
    public static class ComputedInput {
        @GraphQLField(computed = true)
        public String note;
    }

    /** Declares a vector config. */
    @GraphQLType(name = "OrderInput")
    public static class VectorInput {
        @GraphQLField(type = Scalars.VECTOR, vector = @VectorConfig(dimensions = 3))
        public float[] note;
    }

    /** Declares a vector distance. */
    @GraphQLType(name = "OrderInput")
    public static class DistanceInput {
        @GraphQLField(type = "Float", vectorDistance = "embedding")
        public double note;
    }

    /** Declares a field scoped like its parent. */
    @GraphQLType(name = "OrderInput")
    public static class ScopedLikeParentInput {
        @GraphQLField(type = "ID", scopedLikeParent = true)
        public String note;
    }

    /** Declares two refused options on one field, beside a field declaring none. */
    @GraphQLType(name = "OrderInput")
    public static class TwoOptionsInput {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(type = Scalars.VECTOR, vector = @VectorConfig(dimensions = 3),
            requiresScope = "read:Order.note")
        public float[] note;
    }

    /** Declares only what an input field may. */
    @GraphQLType(name = "OrderInput")
    public static class AcceptedInput {
        @GraphQLField(description = "A note", deprecated = "use memo", localized = true)
        public String note;
    }

    /** A crud type whose fields carry each option an input field cannot. */
    @GraphQLType(name = "Document", sqlSource = "v_document", crud = true)
    public static class Document {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(requiresScope = "read:Document.note")
        public String note;

        @GraphQLField(requiresScopes = {"read:Document.owner"})
        public String owner;

        @GraphQLField(computed = true)
        public String slug;

        @GraphQLField(type = Scalars.VECTOR, vector = @VectorConfig(dimensions = 3))
        public float[] embedding;

        @GraphQLField(type = "Float", vectorDistance = "embedding")
        public double distance;
    }

    /** A crud type whose scoped field also carries the options an input field keeps. */
    @GraphQLType(name = "Article", sqlSource = "v_article", crud = true)
    public static class Article {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(description = "The title", deprecated = "use headline", localized = true,
            requiresScope = "read:Article.title")
        public String title;

        @GraphQLField(requiresScope = "read:Article.tags")
        public List<String> tags;
    }

    /** A crud type whose key is computed and scoped. */
    @GraphQLType(name = "Ledger", sqlSource = "v_ledger", crud = true)
    public static class Ledger {
        @GraphQLField(type = "ID", computed = true, requiresScope = "read:Ledger.id")
        public String id;

        @GraphQLField
        public String label;
    }
}
