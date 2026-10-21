import textwrap

from cq_worker.debugger import Action, Debugger
from cq_worker.execution import SCRIPT_FILENAME, compile_script

SOURCE = textwrap.dedent("""\
    def helper(x):
        y = x * 2
        return y

    a = 1
    b = helper(a)
    c = b + 1
    """)


def trace(breakpoints, actions):
    """Run SOURCE, answering each pause with the next action; return pause lines."""

    lines = []
    script_actions = iter(actions)

    def on_pause(frame):
        lines.append(frame.f_lineno)
        return next(script_actions, Action.CONTINUE)

    namespace = {}
    Debugger(SCRIPT_FILENAME, breakpoints, on_pause).run(
        compile_script(SOURCE), namespace
    )
    return lines, namespace


def test_runs_to_breakpoints_only():
    lines, namespace = trace({6}, [])
    assert lines == [6]
    assert namespace["c"] == 3


def test_step_over_skips_function_bodies():
    lines, _ = trace({5}, [Action.STEP, Action.STEP, Action.CONTINUE])
    assert lines == [5, 6, 7]


def test_step_in_and_out():
    lines, _ = trace({6}, [Action.STEP_IN, Action.STEP_OUT, Action.CONTINUE])
    # Step in lands on the first line of helper, step out back in the caller.
    assert lines[:2] == [6, 2]
    assert lines[2] in (6, 7)


def test_stop_aborts_the_script():
    from cq_worker.debugger import DebugStop

    try:
        trace({5}, [Action.STOP])
    except DebugStop:
        pass
    else:
        raise AssertionError("DebugStop expected")
