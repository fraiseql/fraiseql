# Extracted from: docs/reference/config.md
# Block number: 8
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    introspection_policy=IntrospectionPolicy.DISABLED,
    enable_playground=False,
    max_query_depth=10,
    query_timeout=15,
    auto_camel_case=True,
)
