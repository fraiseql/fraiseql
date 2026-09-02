package com.fraiseql.core;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * A relationship to another type, followed by REST resource embedding (#1266).
 *
 * <p>{@code name} is what a client writes in {@code ?select=orders(id,total)},
 * {@code ?select=orders.count} and {@code ?orders.status=paid}; it is also what the
 * generated client's {@code relationships} module and the served OpenAPI document
 * publish.
 *
 * <p>{@code foreignKey} and {@code referencedKey} are SQL <strong>column</strong> names,
 * and which side each is read from swaps with the cardinality — {@code OneToMany} reads
 * {@code referencedKey} off the declaring type and filters {@code foreignKey} on the
 * target; {@code ManyToOne} and {@code OneToOne} do the reverse. Under the default
 * {@code camelCase} naming convention the column {@code fk_user} is published as the
 * field {@code fkUser}, and the compiler resolves one to the other.
 *
 * <p>Which relationships are <em>followable</em> is the compiler's business, not this
 * SDK's: it refuses a target type it does not declare, a join column no field on that
 * side publishes, and a target no list query returns. This SDK carries no second copy of
 * those rules; a copy is what drifts.
 *
 * <p>Usage:
 * <pre>
 * &#64;GraphQLType(name = "User", sqlSource = "v_user", relationships = {
 *     &#64;GraphQLRelationship(name = "orders", targetType = "Order",
 *         cardinality = "OneToMany", foreignKey = "fk_user", referencedKey = "id")
 * })
 * public class User { }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface GraphQLRelationship {
    /** Relationship name — the key in {@code ?select=} and in the response. */
    String name();

    /**
     * Target GraphQL type name. Must be a declared type that some <strong>list</strong>
     * query returns: an embed sources its rows from that query.
     */
    String targetType();

    /** One of {@code "OneToMany"}, {@code "ManyToOne"}, {@code "OneToOne"}. */
    String cardinality();

    /** Foreign key <strong>column</strong> on the child table, e.g. {@code "fk_user"}. */
    String foreignKey();

    /** Referenced key <strong>column</strong> on the parent table, e.g. {@code "id"}. */
    String referencedKey();
}
