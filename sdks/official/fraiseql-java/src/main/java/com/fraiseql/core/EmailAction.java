package com.fraiseql.core;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Factory for email observer actions.
 *
 * <p>The body is emitted as {@code body_template}, the key the server's observer runtime
 * reads. The sender is the server's SMTP configuration; an action cannot set one.
 */
public final class EmailAction {

    private EmailAction() {
        // factory class
    }

    /**
     * Create an email action.
     *
     * @param to      recipient address
     * @param subject email subject
     * @param body    email body template with {@code {{ field }}} placeholders
     * @return action map
     */
    public static Map<String, Object> create(String to, String subject, String body) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "email");
        action.put("to", to);
        action.put("subject", subject);
        action.put("body_template", body);
        return action;
    }
}
