package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a class as a GraphQL type for FraiseQL schema authoring.
 *
 * Usage:
 * <pre>
 * @GraphQLType
 * public class User {
 *     @GraphQLField
 *     public int id;
 *
 *     @GraphQLField
 *     public String name;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface GraphQLType {
    /**
     * Optional custom name for the GraphQL type.
     * If not specified, the class name is used.
     */
    String name() default "";

    /**
     * Optional description for the GraphQL type.
     */
    String description() default "";

    /**
     * Whether this type implements the Relay Node interface.
     * When true, the type participates in global object identification via node(id: ID!)
     * and can be used as the return type of relay-enabled list queries.
     * Requires pk_{entity} (BIGINT) to be present in the view's data JSONB.
     */
    boolean relay() default false;

    /**
     * Whether this type is tenant-scoped.
     * When true, the compiler enforces tenant isolation via RLS.
     */
    boolean tenantScoped() default false;

    /**
     * CRUD operations to auto-generate for this type.
     * Empty (default) = disabled. Use {"all"} for all operations,
     * or specific ops like {"read", "create", "update", "delete"}.
     */
    String[] crud() default {};

    /**
     * Federation key fields for entity resolution.
     * Defaults to {"id"} when federation is enabled on export.
     * Set explicitly for compound keys, e.g. {"id", "region"}.
     */
    String[] keyFields() default {};

    /**
     * Whether this type extends a type defined in another subgraph.
     */
    boolean extends_() default false;
}
