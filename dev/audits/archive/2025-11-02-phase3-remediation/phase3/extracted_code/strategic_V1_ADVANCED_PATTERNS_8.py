# Extracted from: docs/strategic/V1_ADVANCED_PATTERNS.md
# Block number: 8
from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    # Trinity identifier pattern (DEFAULT in v1)
    trinity_identifiers=True,
    primary_key_prefix="pk_",  # pk_user, pk_post
    foreign_key_prefix="fk_",  # fk_organisation, fk_user
    public_id_column="id",  # UUID (exposed in GraphQL)
    identifier_column="identifier",  # Human-readable
    # Mutations as functions (DEFAULT in v1)
    mutations_as_functions=True,
    mutation_function_prefix="fn_",
    sync_function_prefix="fn_sync_tv_",
    # Query side
    query_view_prefix="tv_",
    jsonb_column="data",
)
