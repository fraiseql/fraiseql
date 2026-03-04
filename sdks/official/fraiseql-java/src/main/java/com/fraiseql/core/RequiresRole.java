package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a GraphQL type or field as requiring a specific role for access.
 * This is the simple single-role variant; for multi-role strategies use {@link RoleRequired}.
 *
 * <p>Example:
 * <pre>
 * {@literal @}GraphQLType
 * {@literal @}RequiresRole("admin")
 * public class AdminView {
 *     {@literal @}GraphQLField
 *     public String secretData;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE, ElementType.FIELD})
public @interface RequiresRole {
    /** The role required to access this type or field. */
    String value();
}
