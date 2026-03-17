package io.fraiseql;

/** Thrown when a network-level error occurs during a FraiseQL request. */
public class NetworkException extends FraiseQLException {
    public NetworkException(String message) { super(message); }
    public NetworkException(String message, Throwable cause) { super(message, cause); }
}
