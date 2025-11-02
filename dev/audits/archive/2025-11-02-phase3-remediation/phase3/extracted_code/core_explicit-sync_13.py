# Extracted from: docs/core/explicit-sync.md
# Block number: 13
from unittest.mock import AsyncMock

import pytest


@pytest.mark.asyncio
async def test_create_post():
    """Test post creation without syncing."""
    # Mock the sync function
    sync = AsyncMock()

    # Create post
    post_id = await create_post(title="Test Post", content="...", author_id=UUID("..."), sync=sync)

    # Verify sync was called
    sync.sync_post.assert_called_once_with([post_id], mode="incremental")
