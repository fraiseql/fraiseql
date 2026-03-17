package io.fraiseql;

/** Base class for all FraiseQL SDK exceptions. */
public class FraiseQLException extends RuntimeException {
    public FraiseQLException(String message) { super(message); }
    public FraiseQLException(String message, Throwable cause) { super(message, cause); }
}
