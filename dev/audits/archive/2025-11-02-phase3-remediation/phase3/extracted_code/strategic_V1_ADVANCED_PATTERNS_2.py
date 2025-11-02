# Extracted from: docs/strategic/V1_ADVANCED_PATTERNS.md
# Block number: 2
from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    # Trinity identifier pattern (DEFAULT in v1)
    trinity_identifiers=True,
    # Naming conventions
    primary_key_prefix="pk_",  # pk_user, pk_post
    foreign_key_prefix="fk_",  # fk_organisation, fk_user
    public_id_column="id",  # UUID column
    identifier_column="identifier",  # Human-readable column
)
