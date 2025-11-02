# Extracted from: docs/advanced/llm-integration.md
# Block number: 13
QUERY_TEMPLATES = {
    "list_all": """
query List{entities} {
  {entities} {
    id
    {fields}
  }
}
""",
    "get_by_id": """
query Get{entity}($id: ID!) {
  {entity}(id: $id) {
    id
    {fields}
  }
}
""",
    "search": """
query Search{entities}($query: String!) {
  {entities}(filter: { search: $query }) {
    id
    {fields}
  }
}
""",
}


def fill_template(template_name: str, **kwargs) -> str:
    """Fill query template with parameters."""
    template = QUERY_TEMPLATES[template_name]
    return template.format(**kwargs)


# Usage
query = fill_template("list_all", entities="users", fields="name\nemail")
