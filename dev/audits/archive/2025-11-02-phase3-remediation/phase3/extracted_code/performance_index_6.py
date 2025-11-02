# Extracted from: docs/performance/index.md
# Block number: 6


@connection
def users(info: Info, first: int = 100) -> Connection[User]:
    return info.context.repo.find("users_view", limit=first)
