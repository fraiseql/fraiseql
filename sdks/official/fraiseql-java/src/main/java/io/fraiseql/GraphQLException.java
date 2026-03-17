package io.fraiseql;

import java.util.Collections;
import java.util.List;

/** Thrown when the GraphQL response contains one or more errors. */
public class GraphQLException extends FraiseQLException {
    private final List<GraphQLError> errors;

    public GraphQLException(List<GraphQLError> errors) {
        super(errors.isEmpty() ? "GraphQL error" : errors.get(0).getMessage());
        this.errors = Collections.unmodifiableList(errors);
    }

    public List<GraphQLError> getErrors() { return errors; }
}
