#!/usr/bin/env python3
"""
Cleanup tasks for a specific account_id from MongoDB/CosmosDB.

This script is a maintainer utility for existing deployments. It is not part of the supported
open-source self-hosting or first-time contributor workflow.

Usage:
    python cleanup_tasks_by_account.py <account_id> [--dry-run]
    python cleanup_tasks_by_account.py <user_id> --task-index [--dry-run]

Example:
    python cleanup_tasks_by_account.py 26a8b960-bef3-4329-a4b1-6ccfbfd49bbf --dry-run
    python cleanup_tasks_by_account.py 26a8b960-bef3-4329-a4b1-6ccfbfd49bbf --execute
    python cleanup_tasks_by_account.py 610f4a95-a220-4b50-b333-6176b67a2a9c --task-index --execute
"""

import argparse
import os
import sys
import time
from pymongo import MongoClient


def load_env(env_path: str) -> tuple[str, str]:
    """Load MongoDB connection string and database name from .env file."""
    conn_str = None
    db_name = None

    with open(env_path, "r") as f:
        for line in f:
            line = line.strip()
            if line.startswith("MONGODB_URI="):
                conn_str = line.split("=", 1)[1].strip().strip('"').strip("'")
            elif line.startswith("MONGODB_DATABASE="):
                db_name = line.split("=", 1)[1].strip().strip('"').strip("'")

    if not conn_str:
        raise ValueError("MONGODB_URI not found in .env")
    if not db_name:
        raise ValueError("MONGODB_DATABASE not found in .env")

    return conn_str, db_name


def cleanup_tasks(account_id: str, dry_run: bool = True):
    """Delete all tasks associated with an account_id."""

    # Find .env file (check common locations)
    env_paths = [
        os.path.join(os.path.dirname(__file__), "..", ".env"),
        "/home/azureuser/server/DoWhiz/.env",
        os.path.expanduser("~/DoWhiz/.env"),
    ]

    env_path = None
    for path in env_paths:
        if os.path.exists(path):
            env_path = path
            break

    if not env_path:
        print("ERROR: Could not find .env file")
        print(f"Searched: {env_paths}")
        sys.exit(1)

    print(f"Using env file: {env_path}")
    conn_str, db_name = load_env(env_path)

    print(f"Connecting to database: {db_name}")
    client = MongoClient(conn_str)
    db = client[db_name]

    print(f"Looking for tasks with account_id: {account_id}")
    print(f"Mode: {'DRY RUN' if dry_run else 'LIVE DELETE'}")
    print()

    # Search in task_json string for the account_id
    query = {"task_json": {"$regex": account_id}}

    # List matching tasks
    print("=== tasks collection ===")
    tasks_to_delete = list(db.tasks.find(query))
    print(f"Found {len(tasks_to_delete)} tasks")

    task_ids = []
    for doc in tasks_to_delete:
        task_id = doc.get("task_id")
        owner = doc.get("owner_scope", {})
        enabled = doc.get("enabled")
        kind = doc.get("kind")
        task_ids.append(task_id)
        status = "ENABLED" if enabled else "disabled"
        print(f"  - {task_id} | {status} | {kind} | owner: {owner.get('id', 'unknown')}")

    if not tasks_to_delete:
        print("  No tasks found.")
        return

    print()

    # Check task_index for these task_ids
    print("=== task_index collection ===")
    index_query = {"task_id": {"$in": task_ids}}
    index_count = db.task_index.count_documents(index_query)
    print(f"Found {index_count} entries in task_index for these task_ids")

    if dry_run:
        print()
        print("DRY RUN - no changes made. Run with --execute to delete.")
        return

    print()
    print("Deleting...")

    # Delete from tasks collection with retry for rate limiting
    for attempt in range(3):
        try:
            result = db.tasks.delete_many(query)
            print(f"Deleted {result.deleted_count} from tasks collection")
            break
        except Exception as e:
            if "429" in str(e) or "TooManyRequests" in str(e):
                print(f"Rate limited, waiting 1 second... (attempt {attempt + 1}/3)")
                time.sleep(1)
            else:
                raise

    # Delete from task_index
    if index_count > 0:
        for attempt in range(3):
            try:
                result = db.task_index.delete_many(index_query)
                print(f"Deleted {result.deleted_count} from task_index")
                break
            except Exception as e:
                if "429" in str(e) or "TooManyRequests" in str(e):
                    print(f"Rate limited, waiting 1 second... (attempt {attempt + 1}/3)")
                    time.sleep(1)
                else:
                    raise

    print()
    print("Done!")


def cleanup_task_index_by_user(user_id: str, dry_run: bool = True):
    """Delete all task_index entries for a user_id.

    Use this when tasks are stuck in task_index but not in tasks collection.
    The scheduler uses task_index to find due tasks, so orphaned entries there
    can cause repeated polling.
    """

    # Find .env file (check common locations)
    env_paths = [
        os.path.join(os.path.dirname(__file__), "..", ".env"),
        "/home/azureuser/server/DoWhiz/.env",
        os.path.expanduser("~/DoWhiz/.env"),
    ]

    env_path = None
    for path in env_paths:
        if os.path.exists(path):
            env_path = path
            break

    if not env_path:
        print("ERROR: Could not find .env file")
        print(f"Searched: {env_paths}")
        sys.exit(1)

    print(f"Using env file: {env_path}")
    conn_str, db_name = load_env(env_path)

    print(f"Connecting to database: {db_name}")
    client = MongoClient(conn_str)
    db = client[db_name]

    print(f"Looking for task_index entries with user_id: {user_id}")
    print(f"Mode: {'DRY RUN' if dry_run else 'LIVE DELETE'}")
    print()

    # Query task_index by user_id
    query = {"user_id": user_id}

    print("=== task_index collection ===")
    entries = list(db.task_index.find(query))
    print(f"Found {len(entries)} entries")

    task_ids = []
    for doc in entries:
        task_id = doc.get("task_id")
        next_run = doc.get("next_run")
        enabled = doc.get("enabled")
        task_ids.append(task_id)
        status = "ENABLED" if enabled else "disabled"
        print(f"  - {task_id} | {status} | next_run: {next_run}")

    if not entries:
        print("  No entries found.")
        return

    print()

    # Also check tasks collection for these task_ids
    print("=== tasks collection (for these task_ids) ===")
    tasks_query = {"task_id": {"$in": task_ids}}
    tasks_count = db.tasks.count_documents(tasks_query)
    print(f"Found {tasks_count} matching tasks in tasks collection")

    if dry_run:
        print()
        print("DRY RUN - no changes made. Run with --execute to delete.")
        return

    print()
    print("Deleting from task_index (with rate limit handling)...")

    # Delete one by one to handle rate limiting
    deleted = 0
    for task_id in task_ids:
        for attempt in range(5):
            try:
                db.task_index.delete_one({"task_id": task_id, "user_id": user_id})
                deleted += 1
                break
            except Exception as e:
                if "16500" in str(e) or "429" in str(e) or "TooManyRequests" in str(e):
                    time.sleep(0.1)
                else:
                    print(f"Error deleting {task_id}: {e}")
                    break
        if deleted % 20 == 0 and deleted > 0:
            print(f"  Deleted {deleted}...")
            time.sleep(0.5)

    print(f"Deleted {deleted} from task_index")

    # Also delete from tasks collection
    if tasks_count > 0:
        print("Deleting from tasks collection...")
        tasks_deleted = 0
        for task_id in task_ids:
            for attempt in range(5):
                try:
                    result = db.tasks.delete_many({"task_id": task_id})
                    tasks_deleted += result.deleted_count
                    break
                except Exception as e:
                    if "16500" in str(e) or "429" in str(e) or "TooManyRequests" in str(e):
                        time.sleep(0.1)
                    else:
                        print(f"Error: {e}")
                        break
            if tasks_deleted > 0 and tasks_deleted % 20 == 0:
                time.sleep(0.5)
        print(f"Deleted {tasks_deleted} from tasks collection")

    print()
    print("Done!")


def main():
    parser = argparse.ArgumentParser(
        description="Cleanup tasks for a specific account_id from MongoDB/CosmosDB"
    )
    parser.add_argument("account_id", help="account_id (default) or user_id (with --task-index)")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="List tasks without deleting (default behavior)",
    )
    parser.add_argument(
        "--execute",
        action="store_true",
        help="Actually delete the tasks (required to make changes)",
    )
    parser.add_argument(
        "--task-index",
        action="store_true",
        help="Delete from task_index by user_id. Get user_id from scheduler logs (format: task_id@user_id)",
    )

    args = parser.parse_args()

    if not args.execute:
        args.dry_run = True

    if args.task_index:
        cleanup_task_index_by_user(args.account_id, dry_run=args.dry_run)
    else:
        cleanup_tasks(args.account_id, dry_run=args.dry_run)


if __name__ == "__main__":
    main()
