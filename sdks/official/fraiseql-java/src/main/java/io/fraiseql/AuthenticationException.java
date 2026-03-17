package io.fraiseql;

/** Thrown when the server responds with an authentication error (401 or 403). */
public class AuthenticationException extends FraiseQLException {
    private final int statusCode;

    public AuthenticationException(int statusCode) {
        super("Authentication failed (HTTP " + statusCode + ")");
        this.statusCode = statusCode;
    }

    public int getStatusCode() { return statusCode; }
}
