package com.fraiseql.core;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Factory for email observer actions.
 */
public final class EmailAction {

    private EmailAction() {
        // factory class
    }

    /**
     * Create an email action.
     *
     * @param to      recipient address
     * @param subject email subject (supports {@code {field}} substitutions)
     * @param body    email body (supports {@code {field}} substitutions)
     * @return action map
     */
    public static Map<String, Object> create(String to, String subject, String body) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "email");
        action.put("to", to);
        action.put("subject", subject);
        action.put("body", body);
        return action;
    }

    /**
     * Create an email action with an explicit sender address.
     *
     * @param to      recipient address
     * @param subject email subject
     * @param body    email body
     * @param from    sender address
     * @return action map
     */
    public static Map<String, Object> withFrom(String to, String subject, String body, String from) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "email");
        action.put("to", to);
        action.put("subject", subject);
        action.put("body", body);
        action.put("from", from);
        return action;
    }
}
