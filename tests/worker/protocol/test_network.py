"""Network requests are checked without contacting external services."""

import io
import json

import pytest

from cq_worker import network


class Response(io.BytesIO):
    status = 201


def test_upload_sends_file_as_binary_multipart(monkeypatch):
    captured = {}

    def urlopen(request, timeout):
        captured.update(request=request, timeout=timeout)
        return Response(b"received")

    monkeypatch.setattr(network.urllib.request, "urlopen", urlopen)
    result = network.upload(
        "https://example.test/upload", b"\x00\xffshape", "part.step"
    )
    request = captured["request"]
    content_type = request.get_header("Content-type")
    boundary = content_type.split("boundary=", 1)[1].encode()

    assert result == {"status": 201, "body": "received"}
    assert request.full_url == "https://example.test/upload"
    assert request.get_method() == "POST"
    assert request.get_header("User-agent") == network.USER_AGENT
    assert captured["timeout"] == network.TIMEOUT
    assert b'name="file"; filename="part.step"' in request.data
    assert b"\x00\xffshape" in request.data
    assert request.data.startswith(b"--" + boundary + b"\r\n")
    assert request.data.endswith(b"--" + boundary + b"--\r\n")


def test_upload_replaces_invalid_response_bytes(monkeypatch):
    monkeypatch.setattr(
        network.urllib.request,
        "urlopen",
        lambda *_args, **_kwargs: Response(b"bad \xff"),
    )
    assert network.upload("https://example.test", b"x", "x") == {
        "status": 201,
        "body": "bad \ufffd",
    }


def test_newer_releases_ignores_drafts_and_older_versions(monkeypatch):
    captured = {}
    releases = [
        {"tag_name": "v2.3.0", "draft": False},
        {"tag_name": "v2.2.1", "draft": True},
        {"tag_name": "v2.2.0"},
        {"tag_name": "v1.99.0"},
    ]

    def urlopen(request, timeout):
        captured.update(request=request, timeout=timeout)
        return Response(json.dumps(releases).encode())

    monkeypatch.setattr(network.urllib.request, "urlopen", urlopen)
    assert network.newer_releases("2.2", "acme", "modeler") == ["v2.3.0"]
    assert captured["request"].full_url.endswith("/acme/modeler/releases")
    assert captured["timeout"] == network.TIMEOUT


@pytest.mark.parametrize(
    ("version", "expected"),
    [
        ("v2.4.0", (2, 4, 0)),
        ("2.4", (2, 4, 0)),
        ("v2.4.1-rc1", (2, 4, 1)),
        ("release-10.0.12", (10, 0, 12)),
    ],
)
def test_version_key_orders_numeric_components(version, expected):
    assert network._version_key(version) == expected
