"""Per-line profiling of script runs."""

import sys
import textwrap

from cq_worker.execution import SCRIPT_FILENAME, Execution, RunSettings
from cq_worker.profiling import OPERATIONS_PER_LINE, LineProfiler


def profile(source: str) -> tuple[Execution, dict]:
    execution = Execution(textwrap.dedent(source), None, RunSettings())
    profiler = LineProfiler(SCRIPT_FILENAME)
    execution.run(profiler.run)
    return execution, profiler.result()


def by_line(result: dict) -> dict[int, dict]:
    return {entry["line"]: entry for entry in result["lines"]}


def test_each_line_gets_the_time_it_took_with_everything_it_called():
    execution, result = profile("""\
        import time

        def slow():
            time.sleep(0.12)
            return 1

        total = 0
        for _ in range(3):
            total += slow()
        time.sleep(0.05)
        """)

    assert execution.error is None
    lines = by_line(result)
    # The function's own line, three times.
    assert lines[4]["hits"] == 3 and lines[4]["seconds"] >= 0.3
    # The line calling it includes what it called.
    assert lines[9]["hits"] == 3 and lines[9]["seconds"] >= lines[4]["seconds"]
    assert lines[8]["hits"] == 4, "the loop's own line, once more at the end"
    assert 0.04 <= lines[10]["seconds"] < 0.3
    assert lines[7]["seconds"] < 0.01
    assert [entry["line"] for entry in result["lines"]] == sorted(lines)


def test_calls_into_cadquery_are_timed_per_line_and_operation():
    _, result = profile("""\
        box = cq.Workplane().box(4, 4, 4)
        for size in (0.3, 0.4):
            rounded = box.edges("|Z").fillet(size)
        """)

    operations = {(op["line"], op["name"]): op for op in result["operations"]}
    assert operations[(1, "Workplane.box")]["calls"] == 1
    fillet = operations[(3, "Workplane.fillet")]
    assert fillet["calls"] == 2 and fillet["seconds"] > 0
    assert operations[(3, "Workplane.edges")]["calls"] == 2
    # Operations are the script's calls, not CadQuery's calls to itself.
    assert all(op["line"] in (1, 3) for op in result["operations"])
    # Each is included in its line's time.
    assert by_line(result)[3]["seconds"] >= fillet["seconds"]


def test_only_the_slowest_operations_of_a_line_are_kept():
    _, result = profile("""        w = cq.Workplane()
        made = [w.box(1, 1, 1), w.sphere(1), w.cylinder(1, 1), w.rect(1, 1), w.circle(1), w.polygon(5, 1), w.ellipse(1, 0.5), w.slot2D(2, 1), w.text("a", 1, 1), w.wedge(1, 1, 1, 0, 0, 1, 1)]
        """)
    line = [op for op in result["operations"] if op["line"] == 2]
    assert len(line) == OPERATIONS_PER_LINE, "ten operations, the slowest kept"
    seconds = [op["seconds"] for op in line]
    assert seconds == sorted(seconds, reverse=True)
    assert "Workplane.text" in {op["name"] for op in line}, "text is the slowest"


def test_a_failing_script_keeps_the_profile_up_to_the_error():
    tracer = sys.gettrace()
    execution, result = profile("""\
        import time
        time.sleep(0.05)
        1 / 0
        never = 1
        """)

    assert execution.error["type_name"] == "ZeroDivisionError"
    lines = by_line(result)
    assert lines[2]["seconds"] >= 0.04 and 3 in lines and 4 not in lines
    assert sys.gettrace() is tracer, "the previous trace function is restored"


def test_profiling_is_part_of_a_run_on_request(conversation):
    script = "box = cq.Workplane().box(1, 2, 3)\nshow_object(box)\n"
    talk = conversation(
        {"cmd": "run", "id": 1, "script": script, "profile": True},
        {"cmd": "run", "id": 2, "script": script},
        {"cmd": "run", "id": 3, "script": script, "profile": True, "debug": True},
    )
    talk.serve()

    profiled, plain, debugged = talk.of_type("run_finished")
    assert [entry["line"] for entry in profiled["profile"]["lines"]] == [1, 2]
    assert profiled["profile"]["operations"][0]["name"] == "Workplane.box"
    assert plain["profile"] is None
    # The debugger traces the script itself: it is not profiled too.
    assert debugged["profile"] is None
