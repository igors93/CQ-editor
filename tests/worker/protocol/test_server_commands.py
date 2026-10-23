"""Less common request sequences that must remain safe for the GUI."""

from cq_worker import network

BOX = "part = cq.Workplane().box(1, 2, 3)\nshow_object(part)"


def test_successful_run_then_failed_run_retains_previous_scene(conversation):
    talk = conversation(
        {"cmd": "run", "id": 1, "script": BOX},
        {"cmd": "run", "id": 2, "script": "raise ValueError('bad')"},
    )
    talk.serve()
    first, second = talk.of_type("run_finished")
    assert first["ok"] and len(first["objects"]) == 1
    assert not second["ok"] and second["objects"] is None
    assert second["variables"] is None


def test_release_removes_objects_from_inspection_registry(conversation):
    talk = conversation(
        {"cmd": "run", "id": 1, "script": BOX},
        {"cmd": "inspect", "id": 2, "key": "n1"},
        {"cmd": "release", "id": 3, "keys": ["n1"]},
        {"cmd": "inspect", "id": 4, "key": "n1"},
        {"cmd": "inspect_geometry", "id": 5, "key": "n1", "frame": 0},
    )
    talk.serve()
    assert talk.of_type("inspection")[0]["id"] == 2
    assert [error["id"] for error in talk.of_type("error")] == [4, 5]


def test_measurement_uses_live_occt_vertices_and_rejects_stale_ids(conversation):
    talk = conversation(
        {
            "cmd": "run",
            "id": 1,
            "script": (
                "first = cq.Vertex.makeVertex(0, 0, 0)\n"
                "second = cq.Vertex.makeVertex(3, 4, 0)\n"
            ),
        },
        {
            "cmd": "measure",
            "id": 2,
            "selections": [
                {"key": "n1", "part": 0, "kind": "vertex", "identifier": 1},
                {"key": "n2", "part": 0, "kind": "vertex", "identifier": 1},
            ],
        },
        {"cmd": "release", "keys": ["n1", "n2"]},
        {
            "cmd": "measure",
            "id": 3,
            "selections": [{"key": "n1", "part": 0, "kind": "vertex", "identifier": 1}],
        },
    )

    talk.serve()

    (reply,) = talk.of_type("measurement")
    assert reply["id"] == 2
    assert reply["pair"]["distance"] == 5.0
    assert reply["identity"] == {"scope": "scene_entry", "persistent": False}
    (error,) = talk.of_type("error")
    assert error["id"] == 3
    assert "previous Python session" in error["message"]


def test_selector_for_picked_faces_and_its_errors(conversation):
    script = "box = cq.Workplane().box(2, 4, 6)\nshow_object(box)\n"
    # Face 6 of a box is its top: OCCT's order is deterministic for one box.
    top, bottom = 6, 5
    talk = conversation(
        {"cmd": "run", "id": 1, "script": script},
        {
            "cmd": "selector",
            "id": 2,
            "selections": [{"key": "n1", "part": 0, "kind": "face", "identifier": top}],
        },
        {
            "cmd": "selector",
            "id": 3,
            "selections": [
                {"key": "n1", "part": 0, "kind": "face", "identifier": top},
                {"key": "n1", "part": 0, "kind": "face", "identifier": bottom},
            ],
        },
        {
            "cmd": "selector",
            "id": 4,
            "selections": [{"key": "n1", "part": 0, "kind": "face", "identifier": 99}],
        },
    )
    talk.serve()

    first, both = talk.of_type("selector")
    assert first["id"] == 2 and first["method"] == "faces"
    assert first["candidates"][0] == {
        "expression": '.faces(">Z")',
        "selector": ">Z",
        "description": "largest Z",
        "index_based": False,
    }
    assert both["candidates"][0]["expression"] == '.faces("|Z")'
    (error,) = talk.of_type("error")
    assert error["id"] == 4 and "Unknown face id 99" in error["message"]


def test_selectors_are_available_while_debugging(conversation):
    talk = conversation(
        {
            "cmd": "run",
            "id": 1,
            "script": "box = cq.Workplane().box(1, 1, 1)\nx = 1\n",
            "debug": True,
            "breakpoints": [2],
        },
        {
            "cmd": "selector",
            "id": 2,
            "selections": [{"key": "n1", "part": 0, "kind": "edge", "identifier": 1}],
        },
        {"cmd": "debug", "action": "continue"},
    )
    talk.serve()

    (reply,) = talk.of_type("selector")
    assert reply["id"] == 2 and reply["candidates"]
    assert not talk.of_type("error")


def test_console_reset_discards_previous_script_variables(conversation):
    talk = conversation(
        {"cmd": "run", "id": 1, "script": "value = 7"},
        {"cmd": "console", "id": 2, "source": "print(value)"},
        {"cmd": "console_reset", "id": 3},
        {"cmd": "console", "id": 4, "source": "print('value' in globals())"},
    )
    talk.serve()
    assert talk.output(2) == "7\n"
    assert talk.output(4) == "False\n"
    assert [item["id"] for item in talk.of_type("console_result")] == [2, 3, 4]


def test_inspection_geometry_without_prior_inspection_reports_error(conversation):
    talk = conversation(
        {"cmd": "run", "id": 1, "script": BOX},
        {"cmd": "inspect_geometry", "id": 2, "key": "n1", "frame": 0},
    )
    talk.serve()
    (error,) = talk.of_type("error")
    assert error["id"] == 2
    assert "no longer inspected" in error["message"]


def test_python_upload_uses_utf8_source_and_no_model_selection(
    conversation, monkeypatch
):
    seen = {}

    def fake_upload(url, data, filename):
        seen.update(url=url, data=data, filename=filename)
        return {"status": 200, "body": "ok"}

    monkeypatch.setattr(network, "upload", fake_upload)
    talk = conversation(
        {
            "cmd": "upload",
            "id": 2,
            "url": "https://example.test",
            "file_type": "py",
            "source": "label = 'ação'",
        }
    )
    talk.serve()
    assert seen == {
        "url": "https://example.test",
        "data": "label = 'ação'".encode(),
        "filename": "model.py",
    }
    assert talk.of_type("upload_finished") == [
        {"id": 2, "type": "upload_finished", "status": 200, "body": "ok"}
    ]


def test_model_upload_requires_a_selection(conversation, monkeypatch):
    monkeypatch.setattr(network, "upload", lambda *_args: None)
    talk = conversation(
        {"cmd": "upload", "id": 1, "url": "https://example.test", "file_type": "step"}
    )
    talk.serve()
    (error,) = talk.of_type("error")
    assert error["id"] == 1
    assert "select a model" in error["message"]


def test_update_check_returns_current_and_newer_versions(conversation, monkeypatch):
    monkeypatch.setattr(network, "newer_releases", lambda _current: ["v99.0.0"])
    talk = conversation({"cmd": "check_updates", "id": 8})
    talk.serve()
    (reply,) = talk.of_type("updates")
    assert reply["id"] == 8
    assert reply["current"]
    assert reply["newer"] == ["v99.0.0"]


def test_console_requests_are_blocked_while_debugger_is_paused(conversation):
    talk = conversation(
        {
            "cmd": "run",
            "id": 1,
            "debug": True,
            "breakpoints": [2],
            "script": "a = 1\nb = 2\n",
        },
        {"cmd": "console", "id": 2, "source": "print(a)"},
        {"cmd": "debug", "action": "continue"},
    )
    talk.serve()
    assert talk.of_type("debug_paused")[0]["line"] == 2
    (error,) = talk.of_type("error")
    assert error["id"] == 2
    assert "Not available while debugging" in error["message"]
    assert talk.of_type("run_finished")[0]["ok"]


def test_brep_export_writes_a_nonempty_file(conversation, tmp_path):
    path = tmp_path / "model.brep"
    talk = conversation(
        {"cmd": "run", "id": 1, "script": BOX},
        {"cmd": "export", "id": 2, "keys": ["n1"], "format": "brep", "path": str(path)},
    )
    talk.serve()
    assert talk.of_type("export_finished")[0]["path"] == str(path)
    assert path.stat().st_size > 100


def test_step_upload_sends_actual_model_bytes(conversation, monkeypatch):
    captured = {}

    def fake_upload(url, data, filename):
        captured.update(url=url, data=data, filename=filename)
        return {"status": 202, "body": "accepted"}

    monkeypatch.setattr(network, "upload", fake_upload)
    talk = conversation(
        {"cmd": "run", "id": 1, "script": BOX},
        {
            "cmd": "upload",
            "id": 2,
            "url": "https://example.test",
            "file_type": "step",
            "keys": ["n1"],
        },
    )
    talk.serve()
    assert captured["url"] == "https://example.test"
    assert captured["filename"] == "model.step"
    assert len(captured["data"]) > 100
    assert b"ISO-10303-21" in captured["data"]
    assert talk.of_type("upload_finished")[0]["status"] == 202
