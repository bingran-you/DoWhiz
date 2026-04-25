#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

from grader import FIXTURES_ROOT, REPO_ROOT, load_fixture_specs


SCRIPT_DIR = Path(__file__).resolve().parent
SERVICE_ROOT = REPO_ROOT / "DoWhiz_service"
DEFAULT_ARTIFACT_ROOT = REPO_ROOT / "artifacts" / "oliver_execution" / "latest"
DEFAULT_BINARY = SERVICE_ROOT / "target" / "debug" / "launch_execution_eval"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture-ids", nargs="*")
    parser.add_argument("--artifact-root", type=Path, default=DEFAULT_ARTIFACT_ROOT)
    parser.add_argument("--skip-grade", action="store_true")
    return parser.parse_args()


def build_binary() -> None:
    cmd = [
        "cargo",
        "build",
        "--manifest-path",
        str(SERVICE_ROOT / "Cargo.toml"),
        "-p",
        "scheduler_module",
        "--bin",
        "launch_execution_eval",
    ]
    subprocess.run(cmd, cwd=REPO_ROOT, check=True)


def read_fixture_text(fixture_dir: Path, filename: str) -> str | None:
    path = fixture_dir / filename
    if not path.exists():
        return None
    return path.read_text()


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2))


def run_stage(
    *,
    binary_path: Path,
    artifact_dir: Path,
    request_payload: dict[str, Any],
) -> tuple[bool, dict[str, Any] | None]:
    request_path = artifact_dir / "request.json"
    debug_path = artifact_dir / "debug_response.json"
    write_json(request_path, request_payload)

    cmd = [
        str(binary_path),
        "--request-file",
        str(request_path),
        "--output-file",
        str(debug_path),
    ]
    completed = subprocess.run(
        cmd,
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=240,
    )
    (artifact_dir / "stdout.txt").write_text(completed.stdout or "")
    (artifact_dir / "stderr.txt").write_text(completed.stderr or "")

    if completed.returncode != 0 or not debug_path.exists():
        write_json(
            artifact_dir / "run_error.json",
            {
                "command": cmd,
                "returncode": completed.returncode,
                "stdout": completed.stdout,
                "stderr": completed.stderr,
            },
        )
        return False, None

    payload = json.loads(debug_path.read_text())
    (artifact_dir / "raw_model_output.txt").write_text(payload["raw_model_output"])
    (artifact_dir / "system_prompt.txt").write_text(payload["system_prompt"])
    (artifact_dir / "user_prompt.txt").write_text(payload["user_prompt"])
    write_json(artifact_dir / "parsed_model_output.json", payload["parsed_model_output"])
    write_json(artifact_dir / "final_response.json", payload["response"])
    write_json(
        artifact_dir / "launch_execution_plan.json",
        payload["response"]["launch_execution_plan"],
    )
    write_json(
        artifact_dir / "readiness_brief.json",
        payload["response"]["readiness_brief"],
    )
    write_json(
        artifact_dir / "follow_up_items.json",
        payload["response"]["launch_execution_plan"]["follow_up_items"],
    )
    return True, payload


def main() -> int:
    args = parse_args()
    selected_ids = set(args.fixture_ids or []) or None
    fixtures = load_fixture_specs(selected_ids)
    if not fixtures:
        raise SystemExit("No fixtures matched the requested ids.")

    if args.artifact_root.exists() and not args.fixture_ids:
        shutil.rmtree(args.artifact_root)
    args.artifact_root.mkdir(parents=True, exist_ok=True)
    (args.artifact_root / ".gitkeep").write_text("")

    build_binary()
    run_index: list[dict[str, Any]] = []

    for fixture in fixtures:
        fixture_dir = Path(fixture["_fixture_dir"])
        fixture_artifact_dir = args.artifact_root / fixture["fixture_id"]
        if fixture_artifact_dir.exists():
            shutil.rmtree(fixture_artifact_dir)
        fixture_artifact_dir.mkdir(parents=True, exist_ok=True)
        for name in ["context.txt", "ground_truth.json", "notes.md", "update.txt"]:
            source = fixture_dir / name
            if source.exists():
                shutil.copy2(source, fixture_artifact_dir / name)

        context_text = read_fixture_text(fixture_dir, "context.txt")
        if not context_text:
            raise SystemExit(f"Missing context.txt for {fixture['fixture_id']}")

        initial_request = {
            "source_type": "pasted_thread",
            "source_label": fixture["source_label"],
            "context_text": context_text,
            "update_text": None,
            "prior_plan": None,
        }
        initial_dir = fixture_artifact_dir / "initial"
        initial_dir.mkdir(exist_ok=True)
        initial_ok, initial_payload = run_stage(
            binary_path=DEFAULT_BINARY,
            artifact_dir=initial_dir,
            request_payload=initial_request,
        )
        run_index.append(
            {
                "fixture_id": fixture["fixture_id"],
                "stage": "initial",
                "ok": initial_ok,
                "artifact_dir": str(initial_dir),
            }
        )

        if "refresh" in fixture.get("stages", {}) and read_fixture_text(fixture_dir, "update.txt"):
            refresh_dir = fixture_artifact_dir / "refresh"
            refresh_dir.mkdir(exist_ok=True)
            if not initial_ok or not initial_payload:
                write_json(
                    refresh_dir / "run_error.json",
                    {
                        "command": [],
                        "returncode": 1,
                        "stdout": "",
                        "stderr": "refresh skipped because initial stage failed",
                    },
                )
                refresh_ok = False
            else:
                refresh_request = {
                    "source_type": "pasted_thread",
                    "source_label": fixture["source_label"],
                    "context_text": context_text,
                    "update_text": read_fixture_text(fixture_dir, "update.txt"),
                    "prior_plan": initial_payload["response"]["launch_execution_plan"],
                }
                refresh_ok, _refresh_payload = run_stage(
                    binary_path=DEFAULT_BINARY,
                    artifact_dir=refresh_dir,
                    request_payload=refresh_request,
                )
            run_index.append(
                {
                    "fixture_id": fixture["fixture_id"],
                    "stage": "refresh",
                    "ok": refresh_ok,
                    "artifact_dir": str(refresh_dir),
                }
            )

    write_json(args.artifact_root / "run_index.json", run_index)

    if not args.skip_grade:
        cmd = [
            sys.executable,
            str(SCRIPT_DIR / "grade_validation.py"),
            "--artifact-root",
            str(args.artifact_root),
        ]
        if args.fixture_ids:
            cmd.extend(["--fixture-ids", *args.fixture_ids])
        subprocess.run(cmd, cwd=REPO_ROOT, check=True)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
