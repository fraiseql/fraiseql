"""Vector/embedding specific operators for PostgreSQL pgvector.

This module exposes PostgreSQL's native pgvector distance operators:
- <=> : cosine distance (0.0 = identical, 2.0 = opposite)
- <-> : L2/Euclidean distance (0.0 = identical, ∞ = very different)
- <#> : negative inner product (more negative = more similar)

FraiseQL exposes these operators transparently without abstraction.
Distance values are returned raw from PostgreSQL (no conversion to similarity).
"""

from psycopg.sql import SQL, Composed, Literal


def build_cosine_distance_sql(path_sql: SQL, value: list[float]) -> Composed:
    """Build SQL for cosine distance using PostgreSQL <=> operator.

    Generates: column <=> '[0.1,0.2,...]'::vector
    Returns distance: 0.0 (identical) to 2.0 (opposite)
    """
    vector_literal = "[" + ",".join(str(v) for v in value) + "]"
    return Composed(
        [SQL("("), path_sql, SQL(")::vector <=> "), Literal(vector_literal), SQL("::vector")]
    )


def build_l2_distance_sql(path_sql: SQL, value: list[float]) -> Composed:
    """Build SQL for L2/Euclidean distance using PostgreSQL <-> operator.

    Generates: column <-> '[0.1,0.2,...]'::vector
    Returns distance: 0.0 (identical) to ∞ (very different)
    """
    vector_literal = "[" + ",".join(str(v) for v in value) + "]"
    return Composed(
        [SQL("("), path_sql, SQL(")::vector <-> "), Literal(vector_literal), SQL("::vector")]
    )


def build_inner_product_sql(path_sql: SQL, value: list[float]) -> Composed:
    """Build SQL for inner product using PostgreSQL <#> operator.

    Generates: column <#> '[0.1,0.2,...]'::vector
    Returns negative inner product: more negative = more similar
    """
    vector_literal = "[" + ",".join(str(v) for v in value) + "]"
    return Composed(
        [SQL("("), path_sql, SQL(")::vector <#> "), Literal(vector_literal), SQL("::vector")]
    )


def build_l1_distance_sql(path_sql: SQL, value: list[float]) -> Composed:
    """Build SQL for L1/Manhattan distance using PostgreSQL <+> operator.

    Generates: column <+> '[0.1,0.2,...]'::vector
    Returns distance: sum of absolute differences
    """
    vector_literal = "[" + ",".join(str(v) for v in value) + "]"
    return Composed(
        [SQL("("), path_sql, SQL(")::vector <+> "), Literal(vector_literal), SQL("::vector")]
    )


def build_hamming_distance_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Hamming distance using PostgreSQL <~> operator.

    Generates: column <~> '101010'::bit
    Returns distance: number of differing bits

    Note: Hamming distance works on bit type vectors, not float vectors.
    Use for categorical features, fingerprints, or binary similarity.
    """
    return Composed([SQL("("), path_sql, SQL(")::bit <~> "), Literal(value), SQL("::bit")])


def build_jaccard_distance_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Jaccard distance using PostgreSQL <%> operator.

    Generates: column <%> '111000'::bit
    Returns distance: 1 - (intersection / union) for bit sets

    Note: Jaccard distance works on bit type vectors for set similarity.
    Useful for recommendation systems, tag similarity, feature matching.
    """
    return Composed([SQL("("), path_sql, SQL(")::bit <%> "), Literal(value), SQL("::bit")])
