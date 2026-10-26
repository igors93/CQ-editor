"""Headless build and geometry check through the same worker handlers as the GUI."""

from __future__ import annotations

import argparse
import ast
from contextlib import contextmanager, redirect_stdout
import json
import math
import os
from pathlib import Path
import sys
from typing import Iterator


def replace_parameters(source: str, assignments: list[str]) -> str:
    """Replace top-level literal parameters without moving traceback lines."""
    overrides: dict[str, str] = {}
    for assignment in assignments:
        name, separator, value = assignment.partition("=")
        if not separator or not name.isidentifier() or name.startswith("__"):
            raise ValueError(
                f"Invalid parameter assignment {assignment!r}; use name=value"
            )
        try:
            literal = ast.literal_eval(value)
        except (SyntaxError, ValueError) as error:
            raise ValueError(f"Parameter {name} needs a Python literal") from error
        if type(literal) not in (int, float, bool, str) or (
            isinstance(literal, float) and not math.isfinite(literal)
        ):
            raise ValueError(f"Parameter {name} needs a finite scalar literal")
        overrides[name] = repr(literal)

    if not overrides:
        return source
    tree = ast.parse(source)
    replacements: list[tuple[int, int, str]] = []
    matched: set[str] = set()
    lines = source.splitlines(keepends=True)
    byte_offsets = [0]
    for line in lines:
        byte_offsets.append(byte_offsets[-1] + len(line.encode("utf-8")))
    for node in tree.body:
        name = None
        value = None
        if (
            isinstance(node, ast.Assign)
            and len(node.targets) == 1
            and isinstance(node.targets[0], ast.Name)
        ):
            name, value = node.targets[0].id, node.value
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            name, value = node.target.id, node.value
        if name not in overrides or value is None:
            continue
        try:
            ast.literal_eval(value)
        except (SyntaxError, ValueError):
            continue
        start = byte_offsets[value.lineno - 1] + value.col_offset
        end = byte_offsets[value.end_lineno - 1] + value.end_col_offset
        replacements.append((start, end, overrides[name]))
        matched.add(name)

    if matched != set(overrides):
        missing = sorted(set(overrides) - matched)
        raise ValueError(f"Unknown or non-literal parameter: {', '.join(missing)}")
    encoded = source.encode("utf-8")
    for start, end, value in reversed(replacements):
        encoded = encoded[:start] + value.encode("utf-8") + encoded[end:]
    return encoded.decode("utf-8")


class CliChannel:
    """Only terminal replies are retained; mesh parts never accumulate."""

    def __init__(self) -> None:
        self.current_id: int | None = None
        self.replies: list[dict] = []

    def send(self, message: dict) -> None:
        kind = message.get("type")
        if kind in (
            "error",
            "run_finished",
            "geometry_checked",
            "geometry_invalid",
            "export_finished",
        ):
            self.replies.append(message)
        elif kind == "log":
            print(f"{message['source']}: {message['message']}", file=sys.stderr)

    @contextmanager
    def interruptible(self, _request_id: int) -> Iterator[None]:
        yield

    def cancel_requested(self, _request_id: int) -> bool:
        return False

    def forget_cancel(self, _request_id: int) -> None:
        pass


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="cq-editor")
    commands = parser.add_subparsers(dest="command", required=True)
    for command in ("build", "check"):
        sub = commands.add_parser(command)
        sub.add_argument("script", type=Path)
        sub.add_argument(
            "--set",
            dest="parameters",
            action="append",
            default=[],
            metavar="NAME=VALUE",
        )
        sub.add_argument("--json", action="store_true", help="Print the result as JSON")
        if command == "build":
            sub.add_argument("--export", required=True, type=Path)
            sub.add_argument(
                "--unit",
                choices=("millimeter", "centimeter", "meter", "inch"),
                default="millimeter",
            )
    return parser


def _report(args: argparse.Namespace, payload: dict, status: int) -> int:
    if args.json:
        print(json.dumps(payload, ensure_ascii=False, allow_nan=False))
    elif status:
        if payload.get("reports"):
            for item in payload["reports"]:
                if not item["valid"]:
                    print(
                        f"{item['name']}: {', '.join(item['issues'])}", file=sys.stderr
                    )
        else:
            print(
                payload.get("traceback") or payload.get("error") or payload,
                file=sys.stderr,
            )
    elif args.command == "check":
        for item in payload["reports"]:
            print(f"{item['name']}: valid ({item['solids']} solids)")
    else:
        print(payload["path"])
    return status


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        os.environ.pop("CASROOT", None)
        from .server import Server

        path = args.script.resolve(strict=True)
        source = replace_parameters(
            path.read_text(encoding="utf-8-sig"), args.parameters
        )
        channel = CliChannel()
        server = Server(channel)
        with redirect_stdout(sys.stderr):
            server.dispatch(
                {"cmd": "run", "id": 1, "script": source, "path": str(path)}
            )
        result = channel.replies[-1]
        if result["type"] == "error":
            return _report(args, {"ok": False, "error": result["message"]}, 1)
        if not result["ok"]:
            failure = result.get("error") or {}
            return _report(
                args,
                {
                    "ok": False,
                    "error": failure.get("message", "Script failed"),
                    "traceback": failure.get("text"),
                },
                1,
            )
        keys = [node["key"] for node in result.get("objects") or []]
        if not keys:
            return _report(
                args,
                {"ok": False, "error": "Script produced no displayable objects"},
                1,
            )

        channel.replies.clear()
        if args.command == "check":
            with redirect_stdout(sys.stderr):
                server.dispatch({"cmd": "check", "id": 2, "keys": keys})
            checked = channel.replies[-1]
            if checked["type"] == "error":
                return _report(args, {"ok": False, "error": checked["message"]}, 1)
            reports = checked["reports"]
            valid = bool(reports) and all(report["valid"] for report in reports)
            return _report(args, {"ok": valid, "reports": reports}, 0 if valid else 1)

        output = args.export.resolve()
        file_format = output.suffix.lstrip(".").lower()
        with redirect_stdout(sys.stderr):
            server.dispatch(
                {
                    "cmd": "export",
                    "id": 2,
                    "keys": keys,
                    "format": file_format,
                    "path": str(output),
                    "unit": args.unit,
                }
            )
        exported = channel.replies[-1]
        if exported["type"] == "export_finished":
            return _report(
                args, {"ok": True, "path": str(output), "format": file_format}, 0
            )
        if exported["type"] == "geometry_invalid":
            return _report(
                args,
                {
                    "ok": False,
                    "error": "Invalid geometry",
                    "reports": exported["reports"],
                },
                1,
            )
        return _report(args, {"ok": False, "error": exported["message"]}, 1)
    except (OSError, ValueError, SyntaxError, ImportError) as error:
        return _report(
            args, {"ok": False, "error": f"{type(error).__name__}: {error}"}, 1
        )


if __name__ == "__main__":
    raise SystemExit(main())
