package com.fraiseql.core;

import java.util.*;

/**
 * Information about a GraphQL field type.
 * Captures type name, nullability, and whether it's a list type.
 */
public class TypeInfo {
    public final String typeName;
    public final boolean nullable;
    public final boolean isList;
    public final String description;

    public TypeInfo(String typeName, boolean nullable, boolean isList, String description) {
        this.typeName = typeName;
        this.nullable = nullable;
        this.isList = isList;
        this.description = description;
    }

    public TypeInfo(String typeName, boolean nullable, boolean isList) {
        this(typeName, nullable, isList, "");
    }

    public TypeInfo(String typeName, boolean nullable) {
        this(typeName, nullable, false, "");
    }

    /**
     * Get the full GraphQL type string (e.g., "[String]", "Int!")
     */
    public String getGraphQLType() {
        String type = isList ? "[" + typeName + "]" : typeName;
        if (!nullable) {
            type += "!";
        }
        return type;
    }

    @Override
    public String toString() {
        return "TypeInfo{" + getGraphQLType() + "}";
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        TypeInfo typeInfo = (TypeInfo) o;
        return nullable == typeInfo.nullable &&
               isList == typeInfo.isList &&
               Objects.equals(typeName, typeInfo.typeName);
    }

    @Override
    public int hashCode() {
        return Objects.hash(typeName, nullable, isList);
    }
}
