package com.fraiseql.core;

import java.util.Arrays;
import java.util.List;
import java.util.stream.Collectors;

/**
 * The {@code ActorType} roster #966's actor gate is an allow-list of.
 *
 * <p>snake_case, as the compiler spells it
 * ({@code crates/fraiseql-core/src/security/actor_type.rs}) and as the change-log
 * {@code actor_type TEXT} column stores it.
 */
public final class ActorType {
    public static final String HUMAN_USER = "human_user";
    public static final String SERVICE_ACCOUNT = "service_account";
    public static final String AI_AGENT = "ai_agent";
    public static final String SYSTEM_JOB = "system_job";

    public static final List<String> ALL =
        List.of(HUMAN_USER, SERVICE_ACCOUNT, AI_AGENT, SYSTEM_JOB);

    private ActorType() {}

    /**
     * Check an allow-list where the author wrote it.
     *
     * <p>The compiler refuses an unknown token by name, but only at compile time, and this
     * is a security gate enforced in the same executor arm as {@code requires_role} on
     * every transport — one that fails late fails after the author has stopped looking
     * (#1123).
     *
     * <p>An empty list is refused rather than passed on: the compiled schema omits the key
     * when empty, so an empty allow-list reads as a declared gate and compiles to none.
     *
     * @param operationName the operation being declared, for the message
     * @param actors the declared allow-list
     * @throws IllegalArgumentException if the list is empty or names an unknown token
     */
    public static void validate(String operationName, List<String> actors) {
        if (actors.isEmpty()) {
            throw new IllegalArgumentException(operationName
                + ": requiresActor was given an empty list. An empty allow-list admits "
                + "nobody and is dropped from the compiled schema, which admits everybody "
                + "\u2014 name the actor types instead. Valid: " + String.join(", ", ALL));
        }
        List<String> unknown = actors.stream().filter(a -> !ALL.contains(a))
            .collect(Collectors.toList());
        if (!unknown.isEmpty()) {
            throw new IllegalArgumentException(operationName
                + ": requiresActor names unknown actor type(s) " + String.join(", ", unknown)
                + ". Valid: " + String.join(", ", ALL));
        }
    }
}
