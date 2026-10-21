"""Editor completion behavior with and without the optional Jedi package."""

from types import SimpleNamespace

import pytest

from cq_worker import completion


def test_completion_reports_unavailable_without_jedi(monkeypatch):
    monkeypatch.setattr(completion, "jedi", None)
    assert completion.complete("value.", 1, 6, None, {}) == {
        "available": False,
        "items": [],
        "signatures": [],
    }


@pytest.mark.skipif(not completion.is_available(), reason="jedi is optional")
def test_completion_uses_live_namespace_values():
    result = completion.complete("value.upp", 1, 9, None, {"value": "hello"})
    assert result["available"]
    assert "upper" in {item["name"] for item in result["items"]}


def test_completion_limits_results_and_filters_private_names(monkeypatch):
    class FakeInterpreter:
        def __init__(self, *_args, **_kwargs):
            pass

        def get_signatures(self, *_args):
            return []

        def complete(self, *_args):
            return [SimpleNamespace(name="__private", complete="", type="function")] + [
                SimpleNamespace(name=f"item_{n}", complete=str(n), type="instance")
                for n in range(205)
            ]

    monkeypatch.setattr(
        completion, "jedi", SimpleNamespace(Interpreter=FakeInterpreter)
    )
    result = completion.complete("object.", 1, 7, None, {})
    assert len(result["items"]) == completion.MAX_COMPLETIONS - 1
    assert result["items"][0] == {"name": "item_0", "insert": "0", "kind": "instance"}


def test_call_arguments_return_signatures_instead_of_names(monkeypatch):
    class FakeInterpreter:
        def __init__(self, *_args, **_kwargs):
            pass

        def get_signatures(self, *_args):
            return [SimpleNamespace(to_string=lambda: "make(size)")]

        def complete(self, *_args):
            raise AssertionError("name completion should not run for arguments")

    monkeypatch.setattr(
        completion, "jedi", SimpleNamespace(Interpreter=FakeInterpreter)
    )
    assert completion.complete("make(", 1, 5, None, {}) == {
        "available": True,
        "items": [],
        "signatures": ["make(size)"],
    }
