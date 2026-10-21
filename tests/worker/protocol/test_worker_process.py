"""The Python worker's real stdin/stdout process boundary."""

import json
import os
from pathlib import Path
import subprocess
import sys

PROJECT_ROOT = Path(__file__).resolve().parents[3]


def run_worker(*requests):
    environment = os.environ.copy()
    environment["PYTHONPATH"] = str(PROJECT_ROOT / "worker")
    payload = "".join(json.dumps(request) + "\n" for request in requests)
    process = subprocess.run(
        [sys.executable, "-m", "cq_worker"],
        input=payload,
        text=True,
        encoding="utf-8",
        capture_output=True,
        cwd=PROJECT_ROOT,
        env=environment,
        timeout=30,
        check=False,
    )
    messages = [json.loads(line) for line in process.stdout.splitlines()]
    return process, messages


def test_worker_process_keeps_native_stdout_out_of_protocol():
    process, messages = run_worker(
        {
            "cmd": "run",
            "id": 12,
            "script": "import os\nos.write(1, b'native warning\\n')\nprint('captured')\nresult = cq.Workplane().box(1, 1, 1)\nshow_object(result)",
        },
        {"cmd": "shutdown"},
    )
    assert process.returncode == 0
    assert "native warning" in process.stderr
    assert messages[0]["type"] == "ready"
    assert any(
        message.get("type") == "output"
        and message.get("id") == 12
        and message.get("text") == "captured\n"
        for message in messages
    )
    (finished,) = [m for m in messages if m["type"] == "run_finished"]
    assert finished["ok"]
    assert [obj["name"] for obj in finished["objects"]] == ["result"]


def test_worker_can_be_restarted_after_a_native_process_exit():
    crashed, messages = run_worker(
        {"cmd": "run", "id": 1, "script": "import os; os._exit(7)"}
    )
    assert crashed.returncode == 7
    assert messages[0]["type"] == "ready"

    restarted, messages = run_worker(
        {"cmd": "run", "id": 2, "script": "value = 42"},
        {"cmd": "shutdown"},
    )
    assert restarted.returncode == 0
    (finished,) = [m for m in messages if m["type"] == "run_finished"]
    assert finished["id"] == 2 and finished["ok"]
