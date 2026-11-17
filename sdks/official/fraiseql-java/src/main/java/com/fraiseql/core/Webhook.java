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

    /**
     * Set the request body of a webhook action.
     *
     * @param action       action map produced by {@link #create} or {@link #withEnv}
     * @param bodyTemplate body with {@code {{ field }}} placeholders naming fields of the
     *                     observed entity by their stored key
     * @return the same action map
     */
    public static Map<String, Object> bodyTemplate(Map<String, Object> action, String bodyTemplate) {
        action.put("body_template", bodyTemplate);
        return action;
    }

    /**
     * Sign each request of a webhook action with the HMAC key held in an environment
     * variable. The key itself is never authored: an exported schema is a build artifact.
     *
     * @param action action map produced by {@link #create} or {@link #withEnv}
     * @param envVar environment variable holding the signing key
     * @return the same action map
     */
    public static Map<String, Object> signedWith(Map<String, Object> action, String envVar) {
        action.put("signing_secret_env", envVar);
        return action;
    }
}
