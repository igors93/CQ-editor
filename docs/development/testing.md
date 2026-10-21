# Testing

## Running the tests

From the repository root, with CadQuery, pytest (and optionally jedi)
installed in the active Python environment:

```sh
cargo test                                   # Rust tests
PYTHONPATH=worker python -m pytest tests/worker   # worker tests
```

In PowerShell, set the path first:

```powershell
$env:PYTHONPATH = 'worker'
python -m pytest tests/worker
```

The network tests use local fakes. The process tests start a real worker but
never contact external services.

## Layout

The test folders follow the parts of the application they protect:

| Folder | Coverage |
| --- | --- |
| `tests/rust/app/` | Commands, preferences, file watching, logging, embedded files and the command line |
| `tests/rust/editor/` | Document persistence, recovery, text editing and highlighting |
| `tests/rust/panels/` | Console behavior and the Objects panel layout |
| `tests/rust/protocol/` | JSON messages, binary mesh decoding and interpreter discovery |
| `tests/rust/scene/` | Scene tree, visibility, selection, colors and bounds |
| `tests/rust/viewer/` | Camera, navigation overlays and picking |
| `tests/worker/execution/` | Script environment, debugger, console and completion |
| `tests/worker/geometry/` | Scene building, display options, colors, registry, glTF reading, tessellation and inspection |
| `tests/worker/protocol/` | Request handling, transport, uploads, releases and real worker processes |

Rust tests live in `tests/rust/` but are **included from the source module
they test** with `#[cfg(test)] #[path = "…"] mod tests;`. They can therefore
exercise private items without making them public. Python tests share the
protocol fixtures of `tests/worker/conftest.py`.

## Continuous integration

`.github/workflows/ci.yml` runs on every push to `master` and on pull
requests:

| Job | Steps |
| --- | --- |
| `rust` (Ubuntu 22.04, Windows, macOS) | `cargo fmt --check` · `cargo clippy --all-targets` · `cargo test` |
| `worker` (Ubuntu 22.04, Python 3.12) | `pip install cadquery jedi pytest black` · `black --check worker tests/worker` · `pytest tests/worker` |

Run the same checks locally before opening a pull request:

```sh
cargo fmt --check && cargo clippy --all-targets && cargo test
black --check worker tests/worker
PYTHONPATH=worker python -m pytest tests/worker
```

The CI run is the authoritative status. Do not hand-maintain test totals here:
parameterized compatibility cases make the number change often. CI installs
the latest `black`; use the version reported by the failing job if local and
CI formatting differ.

## Original compatibility gate

The [compatibility matrix](compatibility.md) is part of the test suite, not
just a checklist. `compatibility_matrix_is_complete_and_evidenced` contains
the stable legacy contract ids and fails when:

* an inventoried capability silently disappears from the matrix;
* a Verified or Enhanced row does not name an existing test function;
* a Changed or Gap row has no explicit decision; or
* an Unverified GUI interaction has no follow-up.

`legacy_shortcuts_are_preserved` checks every original shortcut,
`legacy_mouse_gestures_keep_their_navigation_modes` checks the original mouse
bindings, and `test_examples.py` executes the eight example projects that are
unchanged from the pre-rewrite revision. This gate checks inventory and
evidence; it deliberately does not claim that documented gaps are complete.

## Writing tests

* **Rust:** put the test next to its siblings in `tests/rust/<area>/` and
  include it from the module, as the existing files do. Keep tests free of
  a GPU or a window: test the camera math, picking, scene and text
  operations directly.
* **Worker:** prefer calling `Server.dispatch` or the module functions with
  small CadQuery shapes. Use the fixtures in `conftest.py` for protocol
  round trips. Tests that start a real `python -m cq_worker` process belong
  in `tests/worker/protocol/`.
* **Regression tests first:** a bug fix should come with a test that fails
  without it.
