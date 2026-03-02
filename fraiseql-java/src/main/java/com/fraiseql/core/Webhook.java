package com.fraiseql.core;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Factory for webhook observer actions.
 *
 * <p>Example:
 * <pre>
 * new ObserverBuilder("onOrderCreated")
 *     .entity("Order")
 *     .event("INSERT")
 *     .addAction(Webhook.create("https://example.com/hook"))
 *     .register();
 * </pre>
 */
public final class Webhook {

    private Webhook() {
        // factory class
    }

    /**
     * Create a webhook action with an explicit URL.
     *
     * @param url the target URL
     * @return action map
     */
    public static Map<String, Object> create(String url) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "webhook");
        action.put("url", url);
        action.put("headers", new LinkedHashMap<String, String>());
        return action;
    }

    /**
     * Create a webhook action whose URL is read from an environment variable at runtime.
     *
     * @param envVar environment variable name
     * @return action map
     */
    public static Map<String, Object> withEnv(String envVar) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "webhook");
        action.put("url_env", envVar);
        action.put("headers", new LinkedHashMap<String, String>());
        return action;
    }
}
