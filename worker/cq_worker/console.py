"""The interactive Python console behind the GUI's console panel."""

from __future__ import annotations

import builtins
import code
from collections.abc import Callable
from typing import Any

import cadquery as cq

from .execution import ShownObjects, rand_color, script_log


class ConsoleSession:
    """A persistent interpreter.

    Its namespace offers the script API (``show_object`` adds objects to the
    viewer), ``selected()`` for what is picked in the 3D view and, after each
    run, the variables of the last script run.
    """

    def __init__(self) -> None:
        self.shown = ShownObjects()
        self.namespace: dict[str, Any] = {}
        #: Resolves the GUI's selection when ``selected()`` is called.
        self._selection: Callable[[], list[cq.Shape]] = list
        self.reset()

    def reset(self) -> None:
        def selected() -> list[cq.Shape]:
            """The faces, edges and vertices picked in the 3D view, in the
            order they were picked, as ``cq.Face``, ``cq.Edge`` or
            ``cq.Vertex`` placed as shown. When no entity is picked, the
            shapes of the selected objects. An empty list when nothing is
            selected."""

            return list(self._selection())

        self.namespace.clear()
        self.namespace.update(
            {
                "__name__": "__console__",
                "__builtins__": builtins,
                "cq": cq,
                "show_object": self._show_object,
                "show": self._show_object,
                "debug": self._debug,
                "rand_color": rand_color,
                "log": lambda value: script_log.info(str(value)),
                "selected": selected,
            }
        )
        self._interpreter = code.InteractiveConsole(
            self.namespace, filename="<console>"
        )

    def set_selection(self, resolve: Callable[[], list[cq.Shape]]) -> None:
        """What ``selected()`` returns from now on; resolved when called."""

        self._selection = resolve

    def update_variables(self, variables: dict[str, Any]) -> None:
        self.namespace.update(
            (name, value)
            for name, value in variables.items()
            if not name.startswith("__")
        )

    def execute(self, source: str) -> bool:
        """Feed ``source`` line by line; ``True`` if a statement is incomplete."""

        more = False
        for line in source.split("\n"):
            more = self._interpreter.push(line)
        return more

    def take_shown(self) -> ShownObjects:
        shown, self.shown = self.shown, ShownObjects()
        return shown

    # Both wrappers put exactly one frame between the console line and
    # ShownObjects, which looks two frames up for the variable name.

    def _show_object(self, obj, name=None, options=None, **kwargs) -> None:
        self.shown.show(obj, name, options, **kwargs)

    def _debug(self, obj, name=None) -> None:
        self.shown.debug(obj, name)
