package io.fraiseql;

/** Represents a single GraphQL error returned in an error response. */
public class GraphQLError {
    private String message;

    public GraphQLError() {}

    public GraphQLError(String message) { this.message = message; }

    public String getMessage() { return message; }

    public void setMessage(String message) { this.message = message; }
}
