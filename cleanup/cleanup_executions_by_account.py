#!/usr/bin/env python3
import argparse
import os
import time
from pymongo import MongoClient


def load_env(env_path):
    conn_str = None
    db_name = None
    with open(env_path, "r") as f:
        for line in f:
            line = line.strip()
            if line.startswith("MONGODB_URI="):
                conn_str = line.split("=", 1)[1].strip().strip("\"").strip("'")
            elif line.startswith("MONGODB_DATABASE="):
                db_name = line.split("=", 1)[1].strip().strip("\"").strip("'")
    return conn_str, db_name


def cleanup_executions(account_id, dry_run=True):
    env_path = os.path.join(os.path.dirname(__file__), "..", ".env")
    conn_str, db_name = load_env(env_path)
    print(f"Database: {db_name}")
    
    client = MongoClient(conn_str)
    db = client[db_name]
    
    query = {"owner_scope.id": account_id}
    count = db.task_executions.count_documents(query)
    print(f"Found {count} executions for {account_id}")
    
    if count > 0 and not dry_run:
        print("Deleting in small batches...")
        deleted = 0
        while True:
            docs = list(db.task_executions.find(query, {"_id": 1}).limit(10))
            if not docs:
                break
            for d in docs:
                for attempt in range(3):
                    try:
                        db.task_executions.delete_one({"_id": d["_id"]})
                        deleted += 1
                        break
                    except Exception as e:
                        if "16500" in str(e):
                            time.sleep(1)
                        else:
                            raise
            print(f"  Deleted {deleted}...")
            time.sleep(0.5)
        print(f"Total deleted: {deleted}")
    elif dry_run:
        print("DRY RUN - use --execute to delete")
    print("Done!")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("account_id")
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    cleanup_executions(args.account_id, dry_run=not args.execute)
