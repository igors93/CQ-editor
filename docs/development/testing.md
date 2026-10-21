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

> **Current status:** all checks pass: 108 Rust tests, 135 worker tests,
> `cargo fmt`, clippy and `black` (CI installs the latest black, currently
> 26.10.1; use the same version locally to avoid formatting differences).

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
