"""Persistent console state and its connection to the scene."""

import cadquery as cq

from cq_worker.console import ConsoleSession


def test_console_keeps_variables_between_commands():
    console = ConsoleSession()
    assert console.execute("size = 3") is False
    assert console.execute("volume = size ** 3") is False
    assert console.namespace["volume"] == 27


def test_console_reports_incomplete_multiline_statement():
    console = ConsoleSession()
    assert console.execute("for n in range(3):") is True
    assert console.execute("    total = n") is True
    assert console.execute("") is False
    assert console.namespace["total"] == 2


def test_reset_discards_user_state_and_restores_script_api():
    console = ConsoleSession()
    console.execute("custom = 42")
    console.reset()
    assert "custom" not in console.namespace
    assert console.namespace["cq"] is cq
    assert callable(console.namespace["show_object"])
    assert callable(console.namespace["debug"])


def test_update_variables_ignores_dunder_names():
    console = ConsoleSession()
    console.update_variables({"part_count": 4, "__name__": "overwritten"})
    assert console.namespace["part_count"] == 4
    assert console.namespace["__name__"] == "__console__"


def test_take_shown_returns_objects_once():
    console = ConsoleSession()
    part = cq.Workplane().box(1, 2, 3)
    console.namespace["show_object"](part, name="box", color="red")
    shown = console.take_shown()
    assert list(shown) == ["box"]
    assert shown["box"].obj is part
    assert shown["box"].options["color"] == "red"
    assert not console.take_shown()


def test_debug_api_uses_current_scene_after_take_shown():
    console = ConsoleSession()
    console.take_shown()
    console.namespace["debug"](cq.Workplane().box(1, 1, 1), name="debug-box")
    assert list(console.take_shown()) == ["debug-box"]
