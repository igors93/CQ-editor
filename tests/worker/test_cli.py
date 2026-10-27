"""The terminal entry point uses the same run, check and export handlers."""

import json
import os
from pathlib import Path
import subprocess
import sys

import cadquery as cq

from cq_worker.cli import main

WORKER_DIR = Path(__file__).resolve().parents[2] / "worker"


def invoke(*args: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["PYTHONPATH"] = str(WORKER_DIR)
    return subprocess.run(
        [sys.executable, "-m", "cq_worker.cli", *args],
        text=True,
        capture_output=True,
        env=env,
        timeout=45,
        check=False,
    )


def test_build_overrides_parameter_and_exports_readable_step(tmp_path):
    script = tmp_path / "part.py"
    script.write_text(
        "width = 2\npart = cq.Workplane().box(width, 3, 4)\nshow_object(part)\n",
        encoding="utf-8",
    )
    output = tmp_path / "part.step"
    result = invoke(
        "build", str(script), "--export", str(output), "--set", "width=7", "--json"
    )
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["ok"] is True
    assert cq.importers.importStep(str(output)).val().BoundingBox().xlen == 7
    checked = invoke("check", str(script), "--set", "width=7", "--json")
    assert checked.returncode == 0
    assert json.loads(checked.stdout)["reports"][0]["valid"] is True


def test_check_reports_invalid_geometry_as_json_and_failure(tmp_path):
    script = tmp_path / "open.py"
    script.write_text(
        "box = cq.Workplane().box(1, 1, 1).val()\n"
        "show_object(cq.Shell.makeShell(box.Faces()[:-1]), name='open')\n",
        encoding="utf-8",
    )
    result = invoke("check", str(script), "--json")
    assert result.returncode == 1
    payload = json.loads(result.stdout)
    assert payload["reports"][0]["name"] == "open"
    assert payload["reports"][0]["open_shells"] == 1
    assert payload["ok"] is False
    output = tmp_path / "open.step"
    built = invoke("build", str(script), "--export", str(output), "--json")
    assert built.returncode == 1
    assert not output.exists()


def test_check_preserves_script_traceback_and_parameter_errors(tmp_path):
    script = tmp_path / "part.py"
    script.write_text(
        "width = 2\nraise RuntimeError('broken model')\n", encoding="utf-8"
    )
    failed = invoke("check", str(script), "--json")
    assert failed.returncode == 1
    assert "RuntimeError: broken model" in json.loads(failed.stdout)["traceback"]

    unknown = invoke("check", str(script), "--set", "missing=3", "--json")
    assert unknown.returncode == 1
    assert "missing" in json.loads(unknown.stdout)["error"]


def test_cli_dispatch_and_reports_in_the_test_process(tmp_path, capsys):
    # Process tests above protect the entry point; this exercises the actual
    # command handlers under branch coverage in the same Python process.
    script = tmp_path / "part.py"
    script.write_text(
        "width = 2\nshow_object(cq.Workplane().box(width, 3, 4))\n",
        encoding="utf-8",
    )
    output = tmp_path / "part.brep"
    assert (
        main(
            [
                "build",
                str(script),
                "--export",
                str(output),
                "--set",
                "width=5",
                "--json",
            ]
        )
        == 0
    )
    assert json.loads(capsys.readouterr().out)["ok"] is True
    assert abs(cq.importers.importBrep(str(output)).val().BoundingBox().xlen - 5) < 1e-5

    assert main(["check", str(script)]) == 0
    assert "valid" in capsys.readouterr().out

    assert main(["check", str(script), "--set", "missing=1", "--json"]) == 1
    assert "missing" in json.loads(capsys.readouterr().out)["error"]
