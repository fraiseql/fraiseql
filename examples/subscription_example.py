"""Example demonstrating GraphQL subscriptions with FraiseQL."""

import asyncio
from typing import AsyncGenerator
from uuid import UUID, uuid4
import random

import fraiseql
from fraiseql import subscription
from fraiseql.optimization import DataLoader, dataloader_context


# Define types
@fraiseql.type
class Task:
    id: UUID
    title: str
    status: str
    project_id: UUID


@fraiseql.type  
class Project:
    id: UUID
    name: str
    

# Create data loaders
class TaskLoader(DataLoader[UUID, Task]):
    """Batch load tasks by ID."""
    
    async def batch_load(self, task_ids: list[UUID]) -> list[Task | None]:
        print(f"Loading {len(task_ids)} tasks in batch")
        # Simulate database query
        return [
            Task(
                id=task_id,
                title=f"Task {i}",
                status="pending",
                project_id=uuid4()
            )
            for i, task_id in enumerate(task_ids)
        ]


class ProjectTasksLoader(DataLoader[UUID, list[Task]]):
    """Batch load tasks by project ID."""
    
    async def batch_load(self, project_ids: list[UUID]) -> list[list[Task]]:
        print(f"Loading tasks for {len(project_ids)} projects in batch")
        # Simulate database query
        return [
            [
                Task(
                    id=uuid4(),
                    title=f"Task {j} for Project {i}",
                    status="pending",
                    project_id=project_id
                )
                for j in range(3)
            ]
            for i, project_id in enumerate(project_ids)
        ]


# Define subscriptions
@subscription
async def task_updates(info, project_id: UUID) -> AsyncGenerator[Task, None]:
    """Subscribe to task updates for a project."""
    print(f"Starting task updates subscription for project {project_id}")
    
    # Simulate real-time updates
    for i in range(5):
        await asyncio.sleep(1)
        
        task = Task(
            id=uuid4(),
            title=f"Updated Task {i}",
            status=random.choice(["pending", "in_progress", "completed"]),
            project_id=project_id
        )
        
        print(f"Yielding task update: {task.title} - {task.status}")
        yield task
    
    print("Task updates subscription completed")


@subscription
async def project_stats(info) -> AsyncGenerator[dict, None]:
    """Subscribe to project statistics updates."""
    print("Starting project stats subscription")
    
    # Simulate periodic stats updates
    for i in range(3):
        await asyncio.sleep(2)
        
        stats = {
            "total_projects": random.randint(10, 100),
            "active_tasks": random.randint(50, 200),
            "completed_today": random.randint(5, 50),
            "timestamp": f"2025-01-19T{10+i}:00:00Z"
        }
        
        print(f"Yielding stats update: {stats}")
        yield stats
    
    print("Project stats subscription completed")


# Example usage with DataLoader
async def demo_dataloader():
    """Demonstrate DataLoader usage."""
    print("\n=== DataLoader Demo ===")
    
    async with dataloader_context() as ctx:
        task_loader = TaskLoader(context=ctx)
        project_tasks_loader = ProjectTasksLoader(context=ctx)
        
        # Load multiple tasks - will batch
        task_ids = [uuid4() for _ in range(5)]
        tasks = await asyncio.gather(*[
            task_loader.load(task_id) 
            for task_id in task_ids
        ])
        
        print(f"Loaded {len(tasks)} tasks")
        
        # Load tasks for multiple projects - will batch
        project_ids = [uuid4() for _ in range(3)]
        project_tasks = await asyncio.gather(*[
            project_tasks_loader.load(project_id)
            for project_id in project_ids
        ])
        
        print(f"Loaded tasks for {len(project_tasks)} projects")
        
        # Load same task again - will use cache
        cached_task = await task_loader.load(task_ids[0])
        print(f"Loaded cached task: {cached_task.title}")


# Example subscription usage
async def demo_subscriptions():
    """Demonstrate subscription usage."""
    print("\n=== Subscription Demo ===")
    
    # Subscribe to task updates
    project_id = uuid4()
    print(f"\nSubscribing to task updates for project {project_id}")
    
    async for task in task_updates(None, project_id):
        print(f"Received task update: {task.title} - {task.status}")
    
    # Subscribe to project stats
    print("\nSubscribing to project stats")
    
    async for stats in project_stats(None):
        print(f"Received stats: {stats}")


async def main():
    """Run all demos."""
    await demo_dataloader()
    await demo_subscriptions()


if __name__ == "__main__":
    asyncio.run(main())