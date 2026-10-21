"""Wire format and stream isolation regressions."""

import io
import json

import pytest

from cq_worker.protocol import Channel, OutputStream


def messages(writer):
    return [json.loads(line) for line in writer.getvalue().splitlines()]


def test_receive_skips_empty_lines_and_returns_none_at_eof():
    channel = Channel(io.BytesIO(b'\n  \n{"cmd":"run","id":4}\n'), io.BytesIO())
    assert channel.receive() == {"cmd": "run", "id": 4}
    assert channel.receive() is None


def test_receive_rejects_malformed_json():
    channel = Channel(io.BytesIO(b"{broken}\n"), io.BytesIO())
    with pytest.raises(json.JSONDecodeError):
        channel.receive()


def test_send_writes_one_utf8_message_per_line():
    writer = io.BytesIO()
    channel = Channel(io.BytesIO(), writer)
    channel.send({"type": "output", "text": "ação\n新"})
    channel.send({"type": "ready", "id": 3})
    assert messages(writer) == [
        {"type": "output", "text": "ação\n新"},
        {"type": "ready", "id": 3},
    ]
    assert writer.getvalue().count(b"\n") == 2


def test_send_rejects_non_json_floats_without_writing():
    writer = io.BytesIO()
    channel = Channel(io.BytesIO(), writer)
    with pytest.raises(ValueError):
        channel.send({"value": float("nan")})
    assert writer.getvalue() == b""


def test_output_stream_buffers_fragments_until_newline():
    writer = io.BytesIO()
    channel = Channel(io.BytesIO(), writer)
    channel.current_id = 17
    stream = OutputStream(channel, "stdout")
    assert stream.writable() and stream.encoding == "utf-8"
    assert stream.write("first ") == 6
    assert writer.getvalue() == b""
    stream.write("line\n")
    assert messages(writer) == [
        {"type": "output", "id": 17, "stream": "stdout", "text": "first line\n"}
    ]


def test_output_stream_flushes_partial_line_and_ignores_empty_write():
    writer = io.BytesIO()
    stream = OutputStream(Channel(io.BytesIO(), writer), "stderr")
    assert stream.write("") == 0
    stream.flush()
    assert writer.getvalue() == b""
    stream.write("partial")
    stream.flush()
    stream.flush()
    assert messages(writer) == [
        {"type": "output", "id": None, "stream": "stderr", "text": "partial"}
    ]


def test_output_stream_requires_text():
    stream = OutputStream(Channel(io.BytesIO(), io.BytesIO()), "stdout")
    with pytest.raises(TypeError):
        stream.write(b"bytes")


@pytest.mark.parametrize("error", [BrokenPipeError, ValueError])
def test_output_stream_survives_disconnected_gui(error):
    class BrokenWriter:
        def write(self, _data):
            raise error("gone")

    stream = OutputStream(Channel(io.BytesIO(), BrokenWriter()), "stdout")
    stream.write("done\n")
    stream.flush()
