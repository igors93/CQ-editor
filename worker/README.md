# cq_worker

The CadQuery execution backend of CQ-editor. The editor starts it as
`python -m cq_worker` and talks to it over stdin/stdout, one JSON object per
line (see `cq_worker/protocol.py`).

| Module | Role |
| --- | --- |
| `protocol.py` | Message transport and stdout/stderr capture |
| `server.py` | Request dispatch and session state |
| `execution.py` | Running scripts: injected API, tracebacks, module reloading |
| `debugger.py` | Line stepping debugger (`sys.settrace`) |
| `scene.py` | Shown objects to scene nodes: colors, assemblies, registry |
| `tessellation.py` | Meshing shapes into compact arrays |
| `inspector.py` | CadQuery workplane stack inspection |
| `console.py` | The interactive console |
| `completion.py` | jedi based completion |
| `exporting.py` | STEP / STL / BREP export |
| `network.py` | Model upload and CadQuery release checks |

Run the tests with CadQuery installed:

```sh
PYTHONPATH=worker python -m pytest worker/tests
```
