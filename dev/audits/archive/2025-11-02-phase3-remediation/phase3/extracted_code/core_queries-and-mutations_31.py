# Extracted from: docs/core/queries-and-mutations.md
# Block number: 31
import asyncio


@subscription
async def on_task_updates(info, project_id: UUID) -> AsyncGenerator[Task, None]:
    db = info.context["db"]
    last_check = datetime.utcnow()

    while True:
        # Poll for new/updated tasks
        updated_tasks = await db.find(
            "v_task", where={"project_id": project_id, "updated_at__gt": last_check}
        )

        for task in updated_tasks:
            yield task

        last_check = datetime.utcnow()
        await asyncio.sleep(1)  # Poll every second
