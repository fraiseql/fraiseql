# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 7
# In _resolve_fk_integer()
SELECT pk_continent FROM tb_continent WHERE deleted_at IS NULL LIMIT 1
# Returns: 42 (an integer)

# Row generated:
{
    'id': UUID('01020304-5001-0001-0000-000000000015'),  # Encoded UUID
    # 'pk_country': <skipped - DB generates>
    'fk_continent': 42,  # Integer FK - direct reference
    'identifier': 'france-15',
    'name': 'France',
    'iso_code': 'FR'
}
