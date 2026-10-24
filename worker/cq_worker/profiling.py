"""Per-line profiling of a script run (FEAT-E2).

``sys.settrace`` follows the frames of the editor buffer line by line: the
time between two events of a frame goes to the line that was running, with
everything that line called, so a line's time is all the time it cost. Calls
from the script straight into CadQuery are also timed, per function, as the
line's *operations*. Code outside the script is not traced line by line,
which keeps the overhead to one Python call per function call.
"""

from __future__ import annotations

import os
import sys
import time
import types
from collections import defaultdict
from typing import Any

import cadquery

#: CadQuery's own files: calls into them are operations.
_CADQUERY = os.path.dirname(os.path.abspath(cadquery.__file__)) + os.sep
#: Operations reported per line, the slowest first.
OPERATIONS_PER_LINE = 8


class LineProfiler:
    """Runs a script and measures it; :meth:`result` is the profile."""

    def __init__(self, filename: str) -> None:
        self.filename = filename
        #: line -> [seconds, times it started]
        self._lines: defaultdict[int, list] = defaultdict(lambda: [0.0, 0])
        #: (line, function) -> [seconds, calls]
        self._operations: defaultdict[tuple[int, str], list] = defaultdict(
            lambda: [0.0, 0]
        )
        #: Script frames being run -> [line, since]
        self._running: dict[types.FrameType, list] = {}

    def run(self, code: types.CodeType, namespace: dict) -> None:
        """Executes ``code`` like ``exec``, measuring it."""

        previous = sys.gettrace()
        sys.settrace(self._call)
        try:
            exec(code, namespace)
        finally:
            sys.settrace(previous)
            # Frames left without a return event (none in practice).
            now = time.perf_counter()
            for line, since in self._running.values():
                if line is not None:
                    self._lines[line][0] += now - since
            self._running.clear()

    def result(self) -> dict[str, Any]:
        """``lines`` (line, seconds, hits) in line order and, per line, its
        slowest ``operations`` (line, name, seconds, calls)."""

        operations: defaultdict[int, list] = defaultdict(list)
        for (line, name), (seconds, calls) in self._operations.items():
            operations[line].append(
                {"line": line, "name": name, "seconds": seconds, "calls": calls}
            )
        slowest = []
        for line in sorted(operations):
            ranked = sorted(
                operations[line], key=lambda op: op["seconds"], reverse=True
            )
            slowest.extend(ranked[:OPERATIONS_PER_LINE])
        return {
            "lines": [
                {"line": line, "seconds": seconds, "hits": hits}
                for line, (seconds, hits) in sorted(self._lines.items())
                if hits
            ],
            "operations": slowest,
        }

    def _call(self, frame: types.FrameType, event: str, arg: Any):
        """The global trace function: picks the frames worth following."""

        if event != "call":
            return None
        code = frame.f_code
        if code.co_filename == self.filename:
            self._running[frame] = [None, time.perf_counter()]
            return self._line
        caller = frame.f_back
        if (
            caller is not None
            and caller.f_code.co_filename == self.filename
            and code.co_filename.startswith(_CADQUERY)
        ):
            return self._operation(frame, caller.f_lineno, code)
        return None

    def _line(self, frame: types.FrameType, event: str, arg: Any):
        """A script frame's trace function: time goes to the running line."""

        now = time.perf_counter()
        running = self._running.get(frame)
        if running is None:
            return self._line
        line, since = running
        if line is not None:
            self._lines[line][0] += now - since
        running[1] = now
        if event == "line":
            running[0] = frame.f_lineno
            self._lines[frame.f_lineno][1] += 1
        elif event == "return":
            del self._running[frame]
        return self._line

    def _operation(self, frame: types.FrameType, line: int, code: types.CodeType):
        # Its own lines are not followed, only its return.
        frame.f_trace_lines = False
        name = getattr(code, "co_qualname", None)
        if name is None:  # Python 3.10: the method's class from its self
            owner = frame.f_locals.get("self")
            name = code.co_name
            if owner is not None:
                name = f"{type(owner).__name__}.{name}"
        started = time.perf_counter()

        def returned(frame: types.FrameType, event: str, arg: Any):
            if event == "return":
                stats = self._operations[(line, name)]
                stats[0] += time.perf_counter() - started
                stats[1] += 1
            return returned

        return returned
