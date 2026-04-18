#!/usr/bin/env python3
"""
Cleanup tasks for a specific account_id from MongoDB/CosmosDB.

This script is a maintainer utility for existing deployments. It is not part of the supported
open-source self-hosting or first-time contributor workflow.

Usage:
    python cleanup_tasks_by_account.py <account_id> [--dry-run]

Example:
    python cleanup_tasks_by_account.py 26a8b960-bef3-4329-a4b1-6ccfbfd49bbf --dry-run
    python cleanup_tasks_by_account.py 26a8b960-bef3-4329-a4b1-6ccfbfd49bbf --execute
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


def main():
    parser = argparse.ArgumentParser(
        description="Cleanup tasks for a specific account_id from MongoDB/CosmosDB"
    )
    parser.add_argument("account_id", help="The account UUID to cleanup tasks for")
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

    args = parser.parse_args()

    if not args.execute:
        args.dry_run = True

    cleanup_tasks(args.account_id, dry_run=args.dry_run)


if __name__ == "__main__":
    main()
