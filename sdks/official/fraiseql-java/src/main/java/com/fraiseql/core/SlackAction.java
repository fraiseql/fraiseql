package com.fraiseql.core;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Factory for Slack observer actions.
 *
 * <p>The message is emitted as {@code message_template}, the key the server's observer
 * runtime reads; its placeholders are written {@code {{ field }}}.
 */
public final class SlackAction {

    private SlackAction() {
        // factory class
    }

    /**
     * Create a Slack action using the default {@code SLACK_WEBHOOK_URL} environment variable.
     *
     * @param channel         Slack channel (e.g. "#orders")
     * @param messageTemplate message template with {@code {{ field }}} placeholders
     * @return action map
     */
    public static Map<String, Object> create(String channel, String messageTemplate) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "slack");
        action.put("channel", channel);
        action.put("message_template", messageTemplate);
        action.put("webhook_url_env", "SLACK_WEBHOOK_URL");
        return action;
    }

    /**
     * Create a Slack action with an explicit incoming-webhook URL.
     *
     * @param channel         Slack channel
     * @param messageTemplate message template
     * @param webhookUrl      explicit Slack incoming-webhook URL
     * @return action map
     */
    public static Map<String, Object> withWebhookUrl(String channel, String messageTemplate, String webhookUrl) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "slack");
        action.put("channel", channel);
        action.put("message_template", messageTemplate);
        action.put("webhook_url", webhookUrl);
        return action;
    }

    /**
     * Create a Slack action whose incoming-webhook URL is read from an environment variable.
     *
     * @param channel         Slack channel
     * @param messageTemplate message template
     * @param envVar          environment variable holding the incoming-webhook URL
     * @return action map
     */
    public static Map<String, Object> withWebhookEnv(String channel, String messageTemplate, String envVar) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "slack");
        action.put("channel", channel);
        action.put("message_template", messageTemplate);
        action.put("webhook_url_env", envVar);
        return action;
    }
}
