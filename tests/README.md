# Test layout

The test folders follow the parts of the application they protect:

| Folder | Coverage |
| --- | --- |
| `rust/app/` | Commands, preferences, file watching, logging and startup |
| `rust/editor/` | Document persistence, text editing and highlighting |
| `rust/panels/` | Console behavior |
| `rust/protocol/` | JSON messages and binary mesh decoding |
| `rust/scene/` | Scene tree, visibility, selection, colors and bounds |
| `rust/viewer/` | Camera, navigation overlays and picking |
| `worker/execution/` | Script environment, debugger and console |
| `worker/geometry/` | Scene building, colors, registry, glTF and tessellation |
| `worker/protocol/` | Request handling, transport, uploads, releases and real worker processes |

Rust tests live in this directory and are included from the relevant source
module, so they can exercise internal behavior without making it public.
Python tests use `tests/worker/conftest.py` for protocol fixtures.

From the repository root, with CadQuery and pytest installed in the selected
Python environment:

```sh
cargo test
PYTHONPATH=worker python -m pytest tests/worker
```

In PowerShell, set the path with `$env:PYTHONPATH = 'worker'` before running
pytest. The network tests use local fakes; the process tests start a real
worker but do not contact external services.
