"""End-to-end tests of the request/response protocol."""

import time

import cadquery as cq
import pytest

from cq_worker import completion

BOX_SCRIPT = (
    "result = cq.Workplane().box(1, 2, 3)\nshow_object(result)\nprint('hello')\n"
)


def test_ready_and_run(conversation):
    talk = conversation(
        {"cmd": "run", "id": 1, "script": BOX_SCRIPT}, {"cmd": "shutdown"}
    )
    talk.serve()

    ready = talk.of_type("ready")[0]
    assert ready["cadquery"]
    assert ready["ocp"]
    (finished,) = talk.of_type("run_finished")
    assert finished["id"] == 1 and finished["ok"] is True
    assert [o["name"] for o in finished["objects"]] == ["result"]
    variables = {row["name"]: row["type"] for row in finished["variables"]}
    assert variables["result"] == "Workplane"
    assert "show_object" not in variables

    (output,) = talk.of_type("output")
    assert output == {"type": "output", "id": 1, "stream": "stdout", "text": "hello\n"}


def test_failed_run_keeps_previous_scene(conversation):
    talk = conversation({"cmd": "run", "id": 7, "script": "1 / 0"})
    talk.serve()

    (finished,) = talk.of_type("run_finished")
    assert finished["ok"] is False
    assert finished["objects"] is None
    assert finished["error"]["type_name"] == "ZeroDivisionError"


def test_script_log_reaches_the_gui(conversation):
    talk = conversation({"cmd": "run", "id": 2, "script": "log('from script')"})
    import logging

    from cq_worker.server import ChannelLogHandler

    root = logging.getLogger()
    handler, level = ChannelLogHandler(talk.channel), root.level
    root.addHandler(handler)
    root.setLevel(logging.INFO)
    try:
        talk.serve()
    finally:
        root.removeHandler(handler)
        root.setLevel(level)

    messages = [m["message"] for m in talk.of_type("log")]
    assert "from script" in messages


def test_script_log_truncates_an_excessive_record(conversation):
    talk = conversation()
    from cq_worker.protocol import LOG_MESSAGE_CHARS
    from cq_worker.server import ChannelLogHandler
    import logging

    handler = ChannelLogHandler(talk.channel)
    record = logging.LogRecord(
        "script", logging.INFO, __file__, 1, "x" * (LOG_MESSAGE_CHARS + 10), (), None
    )

    handler.emit(record)

    (message,) = talk.of_type("log")
    assert message["message"].count("x") == LOG_MESSAGE_CHARS
    assert "log truncated" in message["message"]


def test_script_log_has_a_total_limit_per_request(conversation):
    talk = conversation()
    from cq_worker.protocol import LOG_CAPTURE_CHARS, LOG_MESSAGE_CHARS
    from cq_worker.server import ChannelLogHandler
    import logging

    talk.channel.current_id = 3
    handler = ChannelLogHandler(talk.channel)
    for _ in range(LOG_CAPTURE_CHARS // LOG_MESSAGE_CHARS):
        handler.emit(
            logging.LogRecord(
                "script", logging.INFO, __file__, 1, "x" * LOG_MESSAGE_CHARS, (), None
            )
        )
    handler.emit(
        logging.LogRecord("script", logging.INFO, __file__, 1, "lost", (), None)
    )
    handler.emit(
        logging.LogRecord("script", logging.INFO, __file__, 1, "lost again", (), None)
    )

    first_request = talk.of_type("log")
    assert (
        sum(message["message"].count("x") for message in first_request)
        == LOG_CAPTURE_CHARS
    )
    assert (
        sum("log output truncated" in message["message"] for message in first_request)
        == 1
    )

    talk.channel.current_id = 4
    handler.emit(
        logging.LogRecord("script", logging.INFO, __file__, 1, "new", (), None)
    )
    assert talk.of_type("log")[-1]["id"] == 4
    assert talk.of_type("log")[-1]["message"] == "new"


def test_restarted_worker_keeps_objects_when_stale_keys_are_released(conversation):
    # After Stop or Restart Python the GUI still shows the old objects, and
    # releases their keys once the first run of the new worker finishes.
    old = conversation({"cmd": "run", "id": 1, "script": BOX_SCRIPT}, session="1")
    old.serve()
    (stale_key,) = [o["key"] for o in old.of_type("run_finished")[0]["objects"]]

    new = conversation(
        {"cmd": "run", "id": 1, "script": BOX_SCRIPT},
        {"cmd": "release", "keys": [stale_key]},
        {"cmd": "inspect", "id": 2, "key": "s2-n1"},
        session="2",
    )
    new.serve()

    assert stale_key == "s1-n1"
    (finished,) = new.of_type("run_finished")
    assert [o["key"] for o in finished["objects"]] == ["s2-n1"]
    assert new.of_type("inspection")[0]["key"] == "s2-n1"
    assert not new.of_type("error")


def test_debug_session(conversation):
    script = "a = cq.Workplane().box(1, 1, 1)\nb = 2\nc = 3\n"
    talk = conversation(
        {"cmd": "run", "id": 3, "script": script, "debug": True, "breakpoints": [2]},
        {"cmd": "inspect", "id": 4, "key": "n1"},
        {"cmd": "debug", "action": "step"},
        {"cmd": "debug", "action": "continue"},
    )
    talk.serve()

    paused = talk.of_type("debug_paused")
    assert [p["line"] for p in paused] == [2, 3]
    assert [o["name"] for o in paused[0]["objects"]] == ["a"]
    # Requests keep being served while paused.
    assert talk.of_type("inspection")[0]["id"] == 4
    (finished,) = talk.of_type("run_finished")
    assert finished["debug"] is True and finished["ok"] is True


def test_debug_stop(conversation):
    talk = conversation(
        {
            "cmd": "run",
            "id": 1,
            "script": "a = 1\nb = 2\n",
            "debug": True,
            "breakpoints": [1],
        },
        {"cmd": "debug", "action": "stop"},
    )
    talk.serve()
    (finished,) = talk.of_type("run_finished")
    assert finished["stopped"] is True


def test_inspection(conversation):
    script = "w = cq.Workplane().box(1, 1, 1).faces('>Z').workplane().hole(0.2)\n"
    talk = conversation(
        {"cmd": "run", "id": 1, "script": script},
        {"cmd": "inspect", "id": 2, "key": "n1"},
        {"cmd": "inspect_geometry", "id": 3, "key": "n1", "frame": 0, "item": None},
        {"cmd": "inspect_geometry", "id": 4, "key": "n1", "frame": 2, "item": 0},
    )
    talk.serve()

    frames = talk.of_type("inspection")[0]["frames"]
    assert len(frames) == 4
    assert frames[0]["type"] == "Workplane"
    whole, single = talk.of_type("inspection_geometry")
    assert whole["plane"]["size"] > 0 and len(whole["meshes"]) == 1
    assert single["plane"] is None


def test_console(conversation):
    talk = conversation(
        {"cmd": "run", "id": 1, "script": "size = 3"},
        {"cmd": "console", "id": 2, "source": "print(size * 2)"},
        {"cmd": "console", "id": 3, "source": "for i in range(2):"},
        {"cmd": "console", "id": 4, "source": "    print(i)"},
        {"cmd": "console", "id": 5, "source": ""},
    )
    talk.serve()

    assert talk.output(2) == "6\n"
    assert talk.output(5) == "0\n1\n"
    results = {m["id"]: m["more"] for m in talk.of_type("console_result")}
    assert results == {2: False, 3: True, 4: True, 5: False}


def test_console_show_object(conversation):
    talk = conversation(
        {
            "cmd": "console",
            "id": 1,
            "source": "show_object(cq.Workplane().box(1, 1, 1))",
        }
    )
    talk.serve()
    (added,) = talk.of_type("scene_add")
    assert len(added["objects"]) == 1


def test_export(conversation, tmp_path):
    step, stl = tmp_path / "out.step", tmp_path / "out.stl"
    talk = conversation(
        {"cmd": "run", "id": 1, "script": BOX_SCRIPT},
        {"cmd": "export", "id": 2, "keys": ["n1"], "format": "step", "path": str(step)},
        {"cmd": "export", "id": 3, "keys": ["n1"], "format": "stl", "path": str(stl)},
        {"cmd": "export", "id": 4, "keys": ["n1"], "format": "obj", "path": str(stl)},
    )
    talk.serve()

    assert [m["id"] for m in talk.of_type("export_finished")] == [2, 3]
    assert step.stat().st_size > 0 and stl.stat().st_size > 0
    (error,) = talk.of_type("error")
    assert error["id"] == 4


def test_unknown_commands_and_bad_keys_report_errors(conversation):
    talk = conversation(
        {"cmd": "bogus", "id": 1}, {"cmd": "inspect", "id": 2, "key": "nope"}
    )
    talk.serve()
    assert [e["id"] for e in talk.of_type("error")] == [1, 2]


def test_imports(conversation, tmp_path):
    (tmp_path / "local_part.py").write_text("X = 1\n")
    script = tmp_path / "main.py"
    script.write_text("import local_part\nimport os\n")
    talk = conversation({"cmd": "imports", "id": 1, "path": str(script)})
    talk.serve()

    (reply,) = talk.of_type("imports")
    assert [p.replace("\\", "/").rsplit("/", 1)[-1] for p in reply["paths"]] == [
        "local_part.py"
    ]


@pytest.mark.skipif(not completion.is_available(), reason="jedi is not installed")
def test_completion(conversation):
    source = "result = cq.Workplane().bo"
    talk = conversation(
        {
            "cmd": "complete",
            "id": 1,
            "source": source,
            "line": 1,
            "column": len(source),
        },
        {"cmd": "complete", "id": 2, "source": "show_object(", "line": 1, "column": 12},
    )
    talk.serve()

    names, signatures = talk.of_type("completions")
    assert "box" in [item["name"] for item in names["items"]]
    assert signatures["items"] == [] and signatures["signatures"]


@pytest.mark.skipif(not completion.is_available(), reason="jedi is not installed")
def test_description_of_the_name_under_the_pointer(conversation):
    source = "result = cq.Workplane().box(1, 2, 3)"
    talk = conversation(
        {
            "cmd": "describe",
            "id": 1,
            "source": source,
            "line": 1,
            "column": source.index("box"),
        },
    )
    talk.serve()

    (reply,) = talk.of_type("description")
    assert reply["id"] == 1
    assert reply["signatures"][0].startswith("box(")
    assert reply["text"]


def test_stack_descriptions_are_readable():
    from cq_worker.inspector import describe

    box = cq.Workplane().box(2, 2, 2)
    assert describe(cq.Vector(1, 2.5, 0)) == "(1, 2.5, 0)"
    assert describe(box.val()) == "1 solid(s), volume 8"
    assert describe(box.faces(">Z").val()) == "plane, area 4"
    assert describe(box.edges().val()).startswith("line, length 2")
    assert describe(cq.Location(cq.Vector(1, 0, 0))) == "at (1, 0, 0)"
    assert "object at 0x" not in describe(box.val())


ASSEMBLY_SCRIPT = """\
assy = cq.Assembly()
assy.add(cq.Workplane().box(1, 1, 1), name="a")
assy.add(cq.Workplane().sphere({radius}), name="b", loc=cq.Location((5, 0, 0)))
assy.add(cq.Workplane().cylinder(2, 0.5), name="c", loc=cq.Location((-5, 0, 0)))
show_object(assy)
"""


def _meshes(nodes: list[dict]) -> dict[str, dict]:
    """Mesh messages by node name, through the assembly tree."""

    found = {}
    for node in nodes:
        for part in node["parts"]:
            found[node["name"]] = part["mesh"]
        found.update(_meshes(node["children"]))
    return found


def _assembly_run(request_id: int, radius: float, known=()) -> dict:
    return {
        "cmd": "run",
        "id": request_id,
        "script": ASSEMBLY_SCRIPT.format(radius=radius),
        "known_geometry": list(known),
    }


def test_run_reports_where_the_time_went(conversation):
    talk = conversation({"cmd": "run", "id": 1, "script": BOX_SCRIPT})
    talk.serve()

    (finished,) = talk.of_type("run_finished")
    timings = finished["timings"]
    assert timings["script"] == finished["duration"]
    assert timings["meshed"] == 1
    assert timings["cached"] == timings["referenced"] == 0
    assert timings["mesh_bytes"] > 0
    assert timings["tessellate"] > 0 and timings["identify"] > 0
    assert timings["encode"] > 0
    assert abs(timings["sent_at"] - time.time()) < 60


def test_editing_one_assembly_part_meshes_and_sends_only_that_part(conversation):
    first = conversation(_assembly_run(1, radius=1))
    first.serve()
    (finished,) = first.of_type("run_finished")
    before = _meshes(finished["objects"])
    assert all("bbox" in mesh for mesh in before.values())
    assert finished["timings"]["meshed"] == 3

    # The GUI advertises what it holds; only the edited sphere is new.
    known = [mesh["id"] for mesh in before.values()]
    edited = conversation(_assembly_run(2, radius=2, known=known))
    edited.serve()
    (finished,) = edited.of_type("run_finished")
    after = _meshes(finished["objects"])
    assert after["a"] == {"id": before["a"]["id"]}
    assert after["c"] == {"id": before["c"]["id"]}
    assert "bbox" in after["b"] and after["b"]["id"] != before["b"]["id"]
    timings = finished["timings"]
    assert (timings["meshed"], timings["cached"], timings["referenced"]) == (1, 0, 2)


def test_a_worker_remeshes_only_edited_parts_even_without_gui_geometry(
    conversation,
):
    # A GUI that holds nothing (e.g. after a restart) still saves meshing
    # time through the worker's cache.
    talk = conversation(_assembly_run(1, radius=1), _assembly_run(2, radius=2))
    talk.serve()

    first, second = talk.of_type("run_finished")
    assert first["timings"]["meshed"] == 3
    timings = second["timings"]
    assert (timings["meshed"], timings["cached"], timings["referenced"]) == (1, 2, 0)


def test_objects_and_inspector_steps_report_their_script_lines(conversation):
    script = (
        "import cadquery as cq\n"
        "plate = (\n"
        "    cq.Workplane()\n"
        "    .box(4, 4, 1)\n"
        "    .faces('>Z')\n"
        ")\n"
        "show_object(plate)\n"
    )
    talk = conversation(
        {"cmd": "run", "id": 1, "script": script},
        {"cmd": "inspect", "id": 2, "key": "n1"},
        {"cmd": "console", "id": 3, "source": "show_object(cq.Workplane().sphere(1))"},
    )
    talk.serve()

    (finished,) = talk.of_type("run_finished")
    assert finished["objects"][0]["line"] == 5
    (inspection,) = talk.of_type("inspection")
    # Newest first; the root Workplane() is not a step.
    assert [frame["line"] for frame in inspection["frames"]] == [5, 4]
    assert inspection["frames"][1]["items"][0]["line"] == 4
    # Objects made in the console have no script line.
    (added,) = talk.of_type("scene_add")
    assert added["objects"][0]["line"] is None
