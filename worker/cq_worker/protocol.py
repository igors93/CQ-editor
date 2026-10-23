"""Line-delimited JSON transport between the CQ-editor GUI and the worker.

Requests arrive on stdin, one JSON object per line. Replies and events leave
through a private duplicate of the original stdout descriptor, also one JSON
object per line.

The worker handles one request at a time, but a ``cancel`` request must reach
it while it is busy: :meth:`Channel.cancel_requested` reads, without waiting,
whatever the GUI sent meanwhile, records cancellations and keeps the other
requests, in order, for :meth:`Channel.receive`.

User scripts and native libraries are free to print: descriptor 1 is pointed
at stderr (which the GUI shows in its log), and the Python-level
``sys.stdout`` / ``sys.stderr`` are replaced by :class:`OutputStream` objects
that forward text as ``output`` messages tagged with the request that
produced them.
"""

from __future__ import annotations

import collections
import io
import json
import os
import sys
import threading
from typing import Any, BinaryIO

Message = dict[str, Any]

#: Bumped whenever the message format changes incompatibly.
PROTOCOL_VERSION = 5

#: Captured output is sent regularly even when a script never writes ``\n``.
OUTPUT_CHUNK_CHARS = 64 * 1024
#: A runaway script cannot fill the GUI and its protocol queue indefinitely.
OUTPUT_CAPTURE_CHARS = 1024 * 1024
#: One Python logging record may include a traceback, but not arbitrary data.
LOG_MESSAGE_CHARS = 256 * 1024
#: Repeated logging calls share a per-request ceiling as well.
LOG_CAPTURE_CHARS = 1024 * 1024

_UNSET = object()


def truncate_text(text: str, limit: int, kind: str) -> str:
    """Keep a text protocol field bounded and make the data loss explicit."""

    if len(text) <= limit:
        return text
    return text[:limit] + f"\n[{kind} truncated after {limit} characters]\n"


class Channel:
    """Bidirectional message pipe to the GUI."""

    def __init__(self, reader: BinaryIO, writer: BinaryIO) -> None:
        self._reader = reader
        self._writer = writer
        self._lock = threading.Lock()
        #: Id of the request being handled; attached to output and log messages.
        self.current_id: int | None = None
        #: Requests the GUI asked to cancel.
        self._cancelled: set[Any] = set()
        #: Requests (or the errors parsing them) read ahead while polling
        #: for cancellations, in arrival order.
        self._pending: collections.deque[Message | Exception] = collections.deque()
        self._partial = b""
        self._ended = False
        # Streams with a descriptor are read with os.read, so that what was
        # read is never hidden in a buffered reader's buffer; others (tests)
        # are read line by line and never polled.
        try:
            self._fd: int | None = reader.fileno()
        except (AttributeError, OSError, io.UnsupportedOperation):
            self._fd = None

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
            if self._pending:
                message = self._pending.popleft()
                if isinstance(message, Exception):
                    raise message
                return message
            if self._ended:
                return None
            if self._fd is None:
                line = self._reader.readline()
                if line:
                    self._take_line(line)
                else:
                    self._ended = True
            else:
                self._take(os.read(self._fd, 65536))

    def cancel_requested(self, request_id: Any) -> bool:
        """Whether the GUI cancelled ``request_id``, reading without waiting
        whatever it has sent meanwhile."""

        self._poll()
        return request_id in self._cancelled

    def forget_cancel(self, request_id: Any) -> None:
        self._cancelled.discard(request_id)

    def _poll(self) -> None:
        while not self._ended and self._fd is not None:
            available = _bytes_available(self._fd)
            if available == 0:
                return
            self._take(os.read(self._fd, max(available, 1)))

    def _take(self, data: bytes) -> None:
        if not data:
            self._ended = True
            if self._partial.strip():
                self._take_line(self._partial)
            self._partial = b""
            return
        *lines, self._partial = (self._partial + data).split(b"\n")
        for line in lines:
            self._take_line(line)

    def _take_line(self, line: bytes) -> None:
        """Records a cancellation at once; queues anything else."""

        line = line.strip()
        if not line:
            return
        try:
            message = json.loads(line)
        except ValueError as error:
            self._pending.append(error)
            return
        if isinstance(message, dict) and message.get("cmd") == "cancel":
            self._cancelled.add(message.get("target"))
        else:
            self._pending.append(message)


def _bytes_available(fd: int) -> int:
    """How much can be read from ``fd`` right away (1 at the end of the
    stream, which reads as empty); 0 when that cannot be told. A thread
    blocked reading stdin is not an option on Windows: loading a DLL then
    waits for that read to finish."""

    if os.name == "nt":
        try:
            import _winapi
            import msvcrt

            available, _ = _winapi.PeekNamedPipe(msvcrt.get_osfhandle(fd), 0)
            return available
        except OSError as error:
            # The other end is closed: the next read reports the end.
            return 1 if getattr(error, "winerror", None) == 109 else 0
    import select

    try:
        readable, _, _ = select.select([fd], [], [], 0)
    except (OSError, ValueError):
        return 0
    return 1 if readable else 0


class OutputStream(io.TextIOBase):
    """Text stream that forwards writes to the GUI as ``output`` messages.

    Text is buffered until a newline, an explicit flush or the chunk limit.
    Each request also has a total capture limit so runaway output cannot grow
    the worker, protocol queue or GUI indefinitely.
    """

    def __init__(self, channel: Channel, name: str) -> None:
        super().__init__()
        self._channel = channel
        self._name = name
        self._pending: list[str] = []
        self._pending_chars = 0
        self._captured_chars = 0
        self._request_id: int | None | object = _UNSET
        self._truncated = False
        self._lock = threading.RLock()

    @property
    def encoding(self) -> str:
        return "utf-8"

    def writable(self) -> bool:
        return True

    def write(self, text: str) -> int:
        if not isinstance(text, str):
            raise TypeError(f"write() argument must be str, not {type(text).__name__}")
        if not text:
            return 0
        with self._lock:
            self._sync_request()
            if self._truncated:
                return len(text)

            remaining = OUTPUT_CAPTURE_CHARS - self._captured_chars
            accepted = text[:remaining]
            was_truncated = len(text) > len(accepted)
            self._captured_chars += len(accepted)
            while accepted:
                room = OUTPUT_CHUNK_CHARS - self._pending_chars
                part, accepted = accepted[:room], accepted[room:]
                self._pending.append(part)
                self._pending_chars += len(part)
                if "\n" in part or self._pending_chars == OUTPUT_CHUNK_CHARS:
                    self._flush_pending()

            if was_truncated:
                self._truncate()
        return len(text)

    def flush(self) -> None:
        with self._lock:
            self._sync_request()
            self._flush_pending()

    def _sync_request(self) -> None:
        request_id = self._channel.current_id
        if self._request_id is _UNSET:
            self._request_id = request_id
        elif self._request_id != request_id:
            self._flush_pending()
            self._request_id = request_id
            self._captured_chars = 0
            self._truncated = False

    def _truncate(self) -> None:
        self._flush_pending()
        self._send(
            f"\n[output truncated after {OUTPUT_CAPTURE_CHARS} characters; "
            "further characters from this request were discarded]\n"
        )
        self._truncated = True

    def _flush_pending(self) -> None:
        if not self._pending:
            return
        text = "".join(self._pending)
        self._pending.clear()
        self._pending_chars = 0
        self._send(text)

    def _send(self, text: str) -> None:
        try:
            self._channel.send(
                {
                    "type": "output",
                    "id": self._request_id,
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
