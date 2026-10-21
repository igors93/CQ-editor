"""Code completion and call signatures, powered by jedi when installed."""

from __future__ import annotations

from typing import Any

try:
    import jedi
except ImportError:  # completion is optional
    jedi = None

MAX_COMPLETIONS = 200


def is_available() -> bool:
    return jedi is not None


def complete(
    source: str,
    line: int,
    column: int,
    path: str | None,
    namespace: dict[str, Any],
) -> dict:
    """Completions at ``line`` (1-based) / ``column`` (0-based, in characters).

    Right after ``(`` or ``,`` the user is typing arguments, so call
    signatures are returned instead of names.
    """

    if jedi is None:
        return {"available": False, "items": [], "signatures": []}

    interpreter = jedi.Interpreter(source, [namespace], path=path)
    current_line = source.split("\n")[line - 1] if source else ""
    before_cursor = current_line[:column].rstrip()

    signatures = [
        signature.to_string() for signature in interpreter.get_signatures(line, column)
    ]
    if before_cursor.endswith(("(", ",")):
        return {"available": True, "items": [], "signatures": signatures}

    items = [
        {
            "name": completion.name,
            "insert": completion.complete or "",
            "kind": completion.type,
        }
        for completion in interpreter.complete(line, column)[:MAX_COMPLETIONS]
        if not completion.name.startswith("__")
    ]
    return {"available": True, "items": items, "signatures": signatures}
