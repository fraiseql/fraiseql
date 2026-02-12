package com.fraiseql.core;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Annotation to register a custom scalar with the schema.
 *
 * <p>This annotation registers the scalar globally so it can be:
 * <ul>
 *   <li>Used in @GraphQLField type parameters</li>
 *   <li>Exported to schema.json</li>
 *   <li>Validated at authoring time</li>
 * </ul>
 *
 * <p>Example:
 * <pre>{@code
 * @Scalar
 * public class Email extends CustomScalar {
 *     public String getName() {
 *         return "Email";
 *     }
 *
 *     public Object serialize(Object value) {
 *         return String.valueOf(value);
 *     }
 *
 *     public Object parseValue(Object value) {
 *         String str = String.valueOf(value);
 *         if (!str.contains("@")) {
 *             throw new IllegalArgumentException("Invalid email");
 *         }
 *         return str;
 *     }
 *
 *     public Object parseLiteral(Object ast) {
 *         if (ast instanceof Map) {
 *             Map<String, Object> astMap = (Map<String, Object>) ast;
 *             if (astMap.containsKey("value")) {
 *                 return parseValue(astMap.get("value"));
 *             }
 *         }
 *         throw new IllegalArgumentException("Invalid email literal");
 *     }
 * }
 *
 * // Use in type:
 * @GraphQLType
 * public class User {
 *     @GraphQLField(type = "Email")
 *     public String email;
 * }
 *
 * // Export schema
 * SchemaRegistry registry = new SchemaRegistry();
 * registry.registerScalar("Email", new Email());
 * Map<String, Object> schema = registry.exportSchema();
 * // schema contains: "customScalars": {"Email": {...}}
 * }</pre>
 *
 * <p>Notes:
 * <ul>
 *   <li>The annotated class must extend CustomScalar</li>
 *   <li>Registration is global (per-process)</li>
 *   <li>Scalar names must be unique within schema</li>
 *   <li>Scalar must be defined before @GraphQLType that uses it</li>
 * </ul>
 */
@Target(ElementType.TYPE)
@Retention(RetentionPolicy.RUNTIME)
public @interface Scalar {
    /**
     * Optional description for the scalar.
     *
     * @return the description, or empty string if not provided
     */
    String value() default "";
}
