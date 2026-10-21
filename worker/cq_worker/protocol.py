"""Line-delimited JSON transport between the CQ-editor GUI and the worker.

Requests arrive on stdin, one JSON object per line. Replies and events leave
through a private duplicate of the original stdout descriptor, also one JSON
object per line.

User scripts and native libraries are free to print: descriptor 1 is pointed
at stderr (which the GUI shows in its log), and the Python-level
``sys.stdout`` / ``sys.stderr`` are replaced by :class:`OutputStream` objects
that forward text as ``output`` messages tagged with the request that
produced them.
"""

from __future__ import annotations

import io
import json
import os
import sys
import threading
from typing import Any, BinaryIO

Message = dict[str, Any]

#: Bumped whenever the message format changes incompatibly.
PROTOCOL_VERSION = 3


class Channel:
    """Bidirectional message pipe to the GUI."""

    def __init__(self, reader: BinaryIO, writer: BinaryIO) -> None:
        self._reader = reader
        self._writer = writer
        self._lock = threading.Lock()
        #: Id of the request being handled; attached to output and log messages.
        self.current_id: int | None = None

    @classmethod
    def from_std_streams(cls) -> Channel:
        """Take over the process' standard streams for the protocol."""

        sys.stdout.flush()
        protocol_fd = os.dup(1)
        # Anything written to descriptor 1 from now on (OpenCascade warnings,
        # C extensions, child processes) lands in stderr instead of
        # corrupting the message stream.
        os.dup2(2, 1)
        return cls(sys.stdin.buffer, os.fdopen(protocol_fd, "wb"))

    def send(self, message: Message) -> None:
        data = json.dumps(
            message, ensure_ascii=False, separators=(",", ":"), allow_nan=False
        ).encode("utf-8")
        with self._lock:
            self._writer.write(data + b"\n")
            self._writer.flush()

    def receive(self) -> Message | None:
        """Block until the next request arrives; ``None`` once the GUI is gone."""

        while True:
            line = self._reader.readline()
            if not line:
                return None
            line = line.strip()
            if line:
                return json.loads(line)


class OutputStream(io.TextIOBase):
    """Text stream that forwards writes to the GUI as ``output`` messages.

    Text is buffered until a newline or an explicit flush so that a single
    ``print`` call produces a single message.
    """

    def __init__(self, channel: Channel, name: str) -> None:
        super().__init__()
        self._channel = channel
        self._name = name
        self._pending: list[str] = []

    @property
    def encoding(self) -> str:
        return "utf-8"

    def writable(self) -> bool:
        return True

    def write(self, text: str) -> int:
        if not isinstance(text, str):
            raise TypeError(f"write() argument must be str, not {type(text).__name__}")
        if text:
            self._pending.append(text)
            if "\n" in text:
                self.flush()
        return len(text)

    def flush(self) -> None:
        if not self._pending:
            return
        text = "".join(self._pending)
        self._pending.clear()
        try:
            self._channel.send(
                {
                    "type": "output",
                    "id": self._channel.current_id,
                    "stream": self._name,
                    "text": text,
                }
            )
        except (OSError, ValueError):
            # The GUI side is gone (typically during interpreter shutdown).
            pass


def redirect_std_streams(channel: Channel) -> None:
    """Route Python-level standard streams through the protocol."""

    sys.stdout = OutputStream(channel, "stdout")
    sys.stderr = OutputStream(channel, "stderr")
    # stdin belongs to the protocol: a script calling input() must not eat
    # the next request, so it gets an empty stream (input() raises EOFError).
    sys.stdin = io.StringIO()
