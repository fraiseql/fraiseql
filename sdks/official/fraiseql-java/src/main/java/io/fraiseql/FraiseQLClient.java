package io.fraiseql;

import java.io.Closeable;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.function.Supplier;

import com.fasterxml.jackson.databind.ObjectMapper;

/**
 * HTTP client for executing GraphQL queries and mutations against a FraiseQL server.
 *
 * <p>Create instances via {@link #builder(String)}:
 *
 * <pre>{@code
 * FraiseQLClient client = FraiseQLClient.builder("https://api.example.com/graphql")
 *     .authorization("Bearer <token>")
 *     .timeout(Duration.ofSeconds(10))
 *     .build();
 * }</pre>
 */
public final class FraiseQLClient implements Closeable {

    private final String url;
    private final Supplier<String> authorizationSupplier;
    private final Duration timeout;
    @SuppressWarnings("unused")
    private final RetryConfig retry;
    private final HttpClient httpClient;
    private final ObjectMapper mapper;

    private FraiseQLClient(Builder builder) {
        this.url = builder.url;
        this.authorizationSupplier = builder.authorizationSupplier;
        this.timeout = builder.timeout;
        this.retry = builder.retry != null ? builder.retry : RetryConfig.noRetry();
        this.httpClient = builder.httpClient != null ? builder.httpClient
                : HttpClient.newBuilder().connectTimeout(this.timeout).build();
        this.mapper = new ObjectMapper();
    }

    /**
     * Returns a new {@link Builder} for the given GraphQL endpoint URL.
     *
     * @param url the GraphQL endpoint URL
     * @return a builder instance
     */
    public static Builder builder(String url) { return new Builder(url); }

    /**
     * Executes a GraphQL query synchronously and deserialises the {@code data} field.
     *
     * @param <T>           the expected response type
     * @param query         the GraphQL query string
     * @param variables     query variables (may be {@code null})
     * @param responseType  the class to deserialise the data into
     * @return the deserialised data, or {@code null} if the response contained no data
     * @throws GraphQLException        if the response contained GraphQL errors
     * @throws AuthenticationException if the server returned 401 or 403
     * @throws RateLimitException      if the server returned 429
     * @throws TimeoutException        if the request timed out
     * @throws NetworkException        for any other transport-level failure
     */
    public <T> T query(String query, Map<String, Object> variables, Class<T> responseType) {
        return executeSync(query, variables, null, responseType);
    }

    /**
     * Executes a GraphQL query synchronously with an operation name.
     *
     * @see #query(String, Map, Class)
     */
    public <T> T query(String query, Map<String, Object> variables, String operationName, Class<T> responseType) {
        return executeSync(query, variables, operationName, responseType);
    }

    /**
     * Executes a GraphQL query synchronously with no variables.
     *
     * @see #query(String, Map, Class)
     */
    public <T> T query(String query, Class<T> responseType) {
        return executeSync(query, null, null, responseType);
    }

    /**
     * Executes a GraphQL mutation synchronously.
     *
     * @see #query(String, Map, Class)
     */
    public <T> T mutate(String mutation, Map<String, Object> variables, Class<T> responseType) {
        return executeSync(mutation, variables, null, responseType);
    }

    /**
     * Executes a GraphQL mutation synchronously with an operation name.
     *
     * @see #mutate(String, Map, Class)
     */
    public <T> T mutate(String mutation, Map<String, Object> variables, String operationName, Class<T> responseType) {
        return executeSync(mutation, variables, operationName, responseType);
    }

    /**
     * Executes a GraphQL query asynchronously.
     *
     * @see #query(String, Map, Class)
     */
    public <T> CompletableFuture<T> queryAsync(String query, Map<String, Object> variables,
            Class<T> responseType) {
        return CompletableFuture.supplyAsync(() -> executeSync(query, variables, null, responseType));
    }

    /**
     * Executes a GraphQL mutation asynchronously.
     *
     * @see #mutate(String, Map, Class)
     */
    public <T> CompletableFuture<T> mutateAsync(String mutation, Map<String, Object> variables,
            Class<T> responseType) {
        return CompletableFuture.supplyAsync(() -> executeSync(mutation, variables, null, responseType));
    }

    @SuppressWarnings("unchecked")
    private <T> T executeSync(String gqlQuery, Map<String, Object> variables, String operationName, Class<T> responseType) {
        try {
            Map<String, Object> body = new HashMap<>();
            body.put("query", gqlQuery);
            if (variables != null) {
                body.put("variables", variables);
            }
            if (operationName != null) {
                body.put("operationName", operationName);
            }

            String bodyJson = mapper.writeValueAsString(body);

            HttpRequest.Builder reqBuilder = HttpRequest.newBuilder()
                    .uri(URI.create(url))
                    .timeout(timeout)
                    .header("Content-Type", "application/json")
                    .POST(HttpRequest.BodyPublishers.ofString(bodyJson));

            if (authorizationSupplier != null) {
                String token = authorizationSupplier.get();
                if (token != null) {
                    reqBuilder.header("Authorization", token);
                }
            }

            HttpResponse<String> response = httpClient.send(
                    reqBuilder.build(), HttpResponse.BodyHandlers.ofString());

            int status = response.statusCode();
            if (status == 401 || status == 403) {
                throw new AuthenticationException(status);
            }
            if (status == 429) {
                throw new RateLimitException();
            }

            Map<String, Object> result = mapper.readValue(response.body(), Map.class);
            Object errorsObj = result.get("errors");
            // null errors = success (cross-SDK invariant — do not throw on absent errors)
            if (errorsObj instanceof List) {
                List<?> errorList = (List<?>) errorsObj;
                if (!errorList.isEmpty()) {
                    List<GraphQLError> gqlErrors = new ArrayList<>();
                    for (Object e : errorList) {
                        GraphQLError err = new GraphQLError();
                        if (e instanceof Map) {
                            Map<?, ?> em = (Map<?, ?>) e;
                            Object msg = em.get("message");
                            if (msg != null) {
                                err.setMessage(msg.toString());
                            }
                        }
                        gqlErrors.add(err);
                    }
                    throw new GraphQLException(gqlErrors);
                }
            }

            Object data = result.get("data");
            if (data == null) {
                return null;
            }
            String dataJson = mapper.writeValueAsString(data);
            return mapper.readValue(dataJson, responseType);
        } catch (FraiseQLException e) {
            throw e;
        } catch (java.net.http.HttpTimeoutException e) {
            throw new TimeoutException("Request timed out", e);
        } catch (Exception e) {
            throw new NetworkException("Request failed: " + e.getMessage(), e);
        }
    }

    /**
     * Closes the client. The underlying {@link HttpClient} does not require explicit shutdown
     * on Java 11–20; on Java 21+ {@code close()} would be available but is not needed here.
     */
    @Override
    public void close() {
        // HttpClient.close() is Java 21+. Nothing required for Java 17 compatibility.
    }

    /** Builder for {@link FraiseQLClient}. */
    public static final class Builder {
        private final String url;
        private Supplier<String> authorizationSupplier;
        private Duration timeout = Duration.ofSeconds(30);
        private RetryConfig retry;
        private HttpClient httpClient;

        private Builder(String url) { this.url = url; }

        /**
         * Sets a static Bearer / API-key token to send in the {@code Authorization} header.
         *
         * @param token the full Authorization header value (e.g. {@code "Bearer xyz"})
         * @return this builder
         */
        public Builder authorization(String token) {
            this.authorizationSupplier = () -> token;
            return this;
        }

        /**
         * Sets a dynamic supplier for the {@code Authorization} header value.
         * The supplier is called on every request, enabling token rotation.
         *
         * @param supplier a function that returns the current token
         * @return this builder
         */
        public Builder authorizationSupplier(Supplier<String> supplier) {
            this.authorizationSupplier = supplier;
            return this;
        }

        /**
         * Sets the per-request timeout (default 30 s).
         *
         * @param timeout the timeout duration
         * @return this builder
         */
        public Builder timeout(Duration timeout) {
            this.timeout = timeout;
            return this;
        }

        /**
         * Sets the retry configuration.
         *
         * @param retry the retry config
         * @return this builder
         */
        public Builder retry(RetryConfig retry) {
            this.retry = retry;
            return this;
        }

        /**
         * Injects a custom {@link HttpClient} (useful for testing with mock servers).
         *
         * @param httpClient the HTTP client to use
         * @return this builder
         */
        public Builder httpClient(HttpClient httpClient) {
            this.httpClient = httpClient;
            return this;
        }

        /** Builds and returns the configured {@link FraiseQLClient}. */
        public FraiseQLClient build() { return new FraiseQLClient(this); }
    }
}
