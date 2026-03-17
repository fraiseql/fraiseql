package io.fraiseql;

/** Thrown when a FraiseQL request exceeds the configured timeout. */
public class TimeoutException extends NetworkException {
    public TimeoutException(String message) { super(message); }
    public TimeoutException(String message, Throwable cause) { super(message, cause); }
}
