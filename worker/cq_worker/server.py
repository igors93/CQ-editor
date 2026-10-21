"""Request dispatch and session state.

The worker is single threaded: requests are handled one at a time, in
arrival order. While a debugging session is paused, :meth:`Server.wait_for_debug_action`
keeps serving read-only requests (completion, inspection, export) so the GUI
stays useful, and returns as soon as a debugger action arrives.
"""

from __future__ import annotations

import logging
import os
import platform
import sys
import tempfile
import time
import traceback
import types
from modulefinder import ModuleFinder

import cadquery as cq

from . import completion, exporting, inspector, network
from .console import ConsoleSession
from .debugger import Action, Debugger
from .execution import (
    SCRIPT_FILENAME,
    Execution,
    RunSettings,
    ShownObjects,
    describe_variables,
    script_api,
)
from .protocol import PROTOCOL_VERSION, Channel, Message
from .scene import Registry, SceneBuilder, find_cq_objects, to_compound
from .tessellation import GeometryCache, Tolerance

log = logging.getLogger(__name__)

#: Requests that are safe to serve while a debugging session is paused.
_ALLOWED_WHILE_PAUSED = {
    "complete",
    "inspect",
    "inspect_geometry",
    "export",
    "imports",
    "release",
    "check_updates",
}


class RequestError(Exception):
    """A request that cannot be served; reported to the GUI as an error."""


class ChannelLogHandler(logging.Handler):
    """Forwards Python log records (including the script's ``log()``) to the GUI."""

    def __init__(self, channel: Channel) -> None:
        super().__init__(logging.INFO)
        self._channel = channel

    def emit(self, record: logging.LogRecord) -> None:
        try:
            message = record.getMessage()
            if record.exc_info:
                message += "\n" + "".join(traceback.format_exception(*record.exc_info))
            self._channel.send(
                {
                    "type": "log",
                    "id": self._channel.current_id,
                    "level": record.levelname.lower(),
                    "source": record.name,
                    "message": message,
                }
            )
        except Exception:
            self.handleError(record)


class Server:
    def __init__(self, channel: Channel, session: str | None = None) -> None:
        """``session`` identifies this worker process among the GUI's
        successive workers; it makes scene keys unique across restarts."""

        self.channel = channel
        self.registry = Registry(f"s{session}-n" if session else "n")
        self.console = ConsoleSession()
        self.geometry_cache = GeometryCache()
        #: Stack frames of inspected workplanes, by scene key.
        self.inspections: dict[str, list[cq.Workplane]] = {}
        self._debugger: Debugger | None = None
        self._handlers = {
            "run": self.run,
            "release": self.release,
            "complete": self.complete,
            "inspect": self.inspect,
            "inspect_geometry": self.inspect_geometry,
            "console": self.run_console,
            "console_reset": self.reset_console,
            "export": self.export,
            "upload": self.upload,
            "check_updates": self.check_updates,
            "imports": self.imports,
        }

    # -- main loop ---------------------------------------------------------

    def serve_forever(self) -> None:
        self.channel.send(
            {
                "type": "ready",
                "protocol": PROTOCOL_VERSION,
                "python": platform.python_version(),
                "cadquery": cq.__version__,
                "executable": sys.executable,
                "completion": completion.is_available(),
                "pid": os.getpid(),
            }
        )
        while True:
            request = self.channel.receive()
            if request is None or request.get("cmd") == "shutdown":
                return
            self.dispatch(request)

    def dispatch(self, request: Message) -> None:
        request_id = request.get("id")
        previous_id, self.channel.current_id = self.channel.current_id, request_id
        try:
            handler = self._handlers.get(request.get("cmd"))
            if handler is None:
                raise RequestError(f"Unknown command {request.get('cmd')!r}")
            reply = handler(request)
            if reply is not None:
                self.channel.send({"id": request_id, **reply})
        except RequestError as error:
            self.channel.send(
                {"type": "error", "id": request_id, "message": str(error)}
            )
        except Exception as error:
            log.debug("Request %r failed", request.get("cmd"), exc_info=True)
            message = f"{type(error).__name__}: {error}"
            self.channel.send({"type": "error", "id": request_id, "message": message})
        finally:
            sys.stdout.flush()
            sys.stderr.flush()
            self.channel.current_id = previous_id

    # -- running scripts ---------------------------------------------------

    def run(self, request: Message) -> Message:
        self.registry.release(request.get("release", []))
        builder = self._scene_builder(request)
        execution = Execution(
            request.get("script", ""),
            request.get("path"),
            RunSettings.from_message(request.get("settings")),
        )
        debug = bool(request.get("debug"))
        executor = None
        if debug:
            self._debugger = Debugger(
                SCRIPT_FILENAME,
                request.get("breakpoints", []),
                lambda frame: self._pause(frame, execution, builder),
            )
            executor = self._debugger.run

        started = time.perf_counter()
        try:
            ok = execution.run(executor)
        finally:
            self._debugger = None
        duration = time.perf_counter() - started

        hidden = execution.hidden_names()
        objects = variables = None
        if ok or debug:
            # A failed plain run keeps the previous model on screen, like the
            # original editor; a debugging session always shows where it ended.
            shown = execution.shown or find_cq_objects(execution.namespace)
            objects = builder.build(shown)
            variables = describe_variables(execution.namespace, hidden)
            self.console.update_variables(
                {k: v for k, v in execution.namespace.items() if k not in hidden}
            )
        return {
            "type": "run_finished",
            "ok": ok,
            "debug": debug,
            "stopped": execution.stopped,
            "objects": objects,
            "variables": variables,
            "error": execution.error,
            "duration": duration,
        }

    def _pause(
        self, frame: types.FrameType, execution: Execution, builder: SceneBuilder
    ) -> Action:
        hidden = execution.hidden_names()
        local_names = frame.f_locals
        self.channel.send(
            {
                "type": "debug_paused",
                "id": self.channel.current_id,
                "line": frame.f_lineno,
                "variables": describe_variables(local_names, hidden),
                "objects": builder.build(find_cq_objects(local_names)),
            }
        )
        return self.wait_for_debug_action()

    def wait_for_debug_action(self) -> Action:
        while True:
            request = self.channel.receive()
            if request is None:
                return Action.STOP
            command = request.get("cmd")
            if command == "debug":
                if "breakpoints" in request and self._debugger is not None:
                    self._debugger.breakpoints = set(request["breakpoints"])
                return Action(request.get("action", "continue"))
            if command == "shutdown":
                sys.exit(0)
            if command in _ALLOWED_WHILE_PAUSED:
                self.dispatch(request)
            else:
                self.channel.send(
                    {
                        "type": "error",
                        "id": request.get("id"),
                        "message": "Not available while debugging; continue or stop first",
                    }
                )

    def release(self, request: Message) -> None:
        keys = request.get("keys", [])
        self.registry.release(keys)
        for key in keys:
            self.inspections.pop(key, None)

    # -- console -----------------------------------------------------------

    def run_console(self, request: Message) -> Message:
        more = self.console.execute(request.get("source", ""))
        shown = self.console.take_shown()
        if shown:
            self.channel.send(
                {
                    "type": "scene_add",
                    "id": request.get("id"),
                    "objects": self._scene_builder(request).build(shown),
                }
            )
        return {"type": "console_result", "more": more}

    def reset_console(self, request: Message) -> Message:
        self.console.reset()
        return {"type": "console_result", "more": False}

    # -- editor services ---------------------------------------------------

    def complete(self, request: Message) -> Message:
        if request.get("target") == "console":
            namespace = self.console.namespace
        else:
            namespace = {**script_api(ShownObjects()), "__name__": "__cq_main__"}
        result = completion.complete(
            request.get("source", ""),
            int(request.get("line", 1)),
            int(request.get("column", 0)),
            request.get("path"),
            namespace,
        )
        return {"type": "completions", **result}

    def imports(self, request: Message) -> Message:
        """Local modules imported by a script, so the GUI can watch them too."""

        path = request["path"]
        finder = ModuleFinder([os.path.dirname(os.path.abspath(path))])
        try:
            finder.run_script(path)
        except SyntaxError as error:
            log.warning("Syntax error in %s: %s", path, error)
        except Exception as error:
            log.warning("Cannot determine imports of %s: %s", path, error)
        paths = [
            module.__file__
            for name, module in finder.modules.items()
            if name != "__main__"
            and module.__file__
            and os.path.isfile(module.__file__)
        ]
        return {"type": "imports", "paths": sorted(set(paths))}

    # -- inspection --------------------------------------------------------

    def inspect(self, request: Message) -> Message:
        key = request["key"]
        obj = self.registry.get(key).obj
        if not isinstance(obj, cq.Workplane):
            raise RequestError("Only workplanes can be inspected")
        frames = inspector.stack_frames(obj)
        self.inspections[key] = frames
        return {
            "type": "inspection",
            "key": key,
            "frames": inspector.describe_stack(frames),
        }

    def inspect_geometry(self, request: Message) -> Message:
        frames = self.inspections.get(request["key"])
        if frames is None:
            raise RequestError("Object is no longer inspected")
        frame = frames[int(request["frame"])]
        item = request.get("item")
        geometry = inspector.frame_geometry(
            frame, None if item is None else int(item), self._tolerance(request)
        )
        return {"type": "inspection_geometry", **geometry}

    # -- files and network -------------------------------------------------

    def export(self, request: Message) -> Message:
        exporting.export(
            self.registry,
            request.get("keys", []),
            request["format"],
            request["path"],
            float(request.get("tolerance", 0.1)),
            float(request.get("angular_tolerance", 0.1)),
        )
        return {"type": "export_finished", "path": request["path"]}

    def upload(self, request: Message) -> Message:
        file_type = request.get("file_type", "step").lower()
        if file_type == "py":
            data = request.get("source", "").encode("utf-8")
        else:
            shapes = self.registry.subtree_shapes(request.get("keys", []))
            if not shapes:
                raise RequestError("Please select a model to upload")
            data = _export_to_bytes(to_compound(shapes), file_type)
        result = network.upload(request["url"], data, f"model.{file_type}")
        return {"type": "upload_finished", **result}

    def check_updates(self, request: Message) -> Message:
        return {
            "type": "updates",
            "current": cq.__version__,
            "newer": network.newer_releases(cq.__version__),
        }

    # -- helpers -----------------------------------------------------------

    @staticmethod
    def _tolerance(request: Message) -> Tolerance:
        scene = request.get("scene") or {}
        return Tolerance(
            linear=float(scene.get("linear_tolerance", Tolerance.linear)),
            angular=float(scene.get("angular_tolerance", Tolerance.angular)),
        )

    def _scene_builder(self, request: Message) -> SceneBuilder:
        scene = request.get("scene") or {}
        return SceneBuilder(
            self.registry,
            self._tolerance(request),
            merge_assemblies=bool(scene.get("merge_assemblies", False)),
            geometry_cache=self.geometry_cache,
            known_geometry=request.get("known_geometry", ()),
        )


def _export_to_bytes(compound: cq.Compound, file_type: str) -> bytes:
    fd, path = tempfile.mkstemp(suffix=f".{file_type}")
    os.close(fd)
    try:
        if file_type == "stl":
            compound.exportStl(path)
        elif file_type == "step":
            compound.exportStep(path)
        else:
            raise RequestError(f"Unsupported upload type {file_type!r}")
        with open(path, "rb") as file:
            return file.read()
    finally:
        os.unlink(path)
