"""Minimal Todo API - Single file implementation."""

import os
from datetime import datetime
from typing import List

import fraiseql
from fraiseql import fraise_field
from fraiseql.types.scalars import UUID


@fraiseql.type
class Task:
    """A task in the todo system."""

    id: UUID = fraise_field(description="Task ID")
    title: str = fraise_field(description="Task title")
    description: str | None = fraise_field(description="Task description")
    completed: bool = fraise_field(description="Whether task is completed")
    created_at: datetime = fraise_field(description="When task was created")


@fraiseql.input
class CreateTaskInput:
    """Input for creating a new task."""

    title: str = fraise_field(description="Task title")
    description: str | None = fraise_field(description="Task description")


@fraiseql.type
class QueryRoot:
    """Root query type."""

    tasks: List[Task] = fraise_field(description="List all tasks")
    task: Task | None = fraise_field(description="Get single task by ID")

    async def resolve_tasks(self, info, completed: bool | None = None):
        repo = info.context["repo"]
        where = {}
        if completed is not None:
            where["completed"] = completed
        results = await repo.find("v_task", where=where)
        return [Task(**result) for result in results]

    async def resolve_task(self, info, id: UUID):
        repo = info.context["repo"]
        result = await repo.find_one("v_task", where={"id": id})
        return Task(**result) if result else None


@fraiseql.type
class MutationRoot:
    """Root mutation type."""

    create_task: Task = fraise_field(description="Create a new task")
    complete_task: Task | None = fraise_field(description="Mark task as completed")

    async def resolve_create_task(self, info, input: CreateTaskInput):
        repo = info.context["repo"]
        task_id = await repo.call_function(
            "fn_create_task", p_title=input.title, p_description=input.description
        )
        result = await repo.find_one("v_task", where={"id": task_id})
        return Task(**result)

    async def resolve_complete_task(self, info, id: UUID):
        repo = info.context["repo"]
        success = await repo.call_function("fn_complete_task", p_id=id)
        if success:
            result = await repo.find_one("v_task", where={"id": id})
            return Task(**result) if result else None
        return None


# Create the FastAPI app
app = fraiseql.create_fraiseql_app(
    queries=[QueryRoot],
    mutations=[MutationRoot],
    database_url=os.getenv("FRAISEQL_DATABASE_URL"),
)

if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
