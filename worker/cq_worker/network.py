"""Model upload and release checks (CadQuery's and CQ-editor's), using only
the standard library."""

from __future__ import annotations

import json
import re
import urllib.request
import uuid

USER_AGENT = "CQ-editor"
TIMEOUT = 60
RELEASES_URL = "https://api.github.com/repos/{org}/{project}/releases"
#: Where CQ-editor is published.
EDITOR_ORG, EDITOR_PROJECT = "igors93", "CQ-editor"


def upload(url: str, data: bytes, file_name: str) -> dict:
    """POST ``data`` as the multipart form field ``file``."""

    boundary = uuid.uuid4().hex
    body = b"".join(
        (
            f"--{boundary}\r\n".encode(),
            f'Content-Disposition: form-data; name="file"; filename="{file_name}"\r\n'.encode(),
            b"Content-Type: application/octet-stream\r\n\r\n",
            data,
            f"\r\n--{boundary}--\r\n".encode(),
        )
    )
    request = urllib.request.Request(
        url,
        data=body,
        method="POST",
        headers={
            "Content-Type": f"multipart/form-data; boundary={boundary}",
            "User-Agent": USER_AGENT,
        },
    )
    with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
        return {
            "status": response.status,
            "body": response.read().decode("utf-8", errors="replace"),
        }


def newer_releases(
    current: str, org: str = "cadquery", project: str = "cadquery"
) -> list[str]:
    """Published release tags newer than ``current``."""

    return [
        release["tag_name"]
        for release in _releases(org, project)
        if not release.get("draft")
        and _version_key(release["tag_name"]) > _version_key(current)
    ]


def editor_releases(current: str) -> list[dict]:
    """CQ-editor's releases newer than ``current``, newest first, each with
    its ``tag``, ``name`` and ``url`` (the page with the installers). Drafts
    and pre-releases are left out."""

    newer = [
        release
        for release in _releases(EDITOR_ORG, EDITOR_PROJECT)
        if not release.get("draft")
        and not release.get("prerelease")
        and _version_key(release["tag_name"]) > _version_key(current)
    ]
    newer.sort(key=lambda release: _version_key(release["tag_name"]), reverse=True)
    page = f"https://github.com/{EDITOR_ORG}/{EDITOR_PROJECT}/releases/tag/"
    return [
        {
            "tag": release["tag_name"],
            "name": release.get("name") or release["tag_name"],
            "url": release.get("html_url") or page + release["tag_name"],
        }
        for release in newer
    ]


def _releases(org: str, project: str) -> list[dict]:
    request = urllib.request.Request(
        RELEASES_URL.format(org=org, project=project),
        headers={"User-Agent": USER_AGENT},
    )
    with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
        return json.load(response)


def _version_key(version: str) -> tuple[int, int, int]:
    """``"v2.4.0"`` -> ``(2, 4, 0)``; pre-release suffixes are ignored."""

    numbers = [int(n) for n in re.findall(r"\d+", version)[:3]]
    return tuple(numbers + [0] * (3 - len(numbers)))
