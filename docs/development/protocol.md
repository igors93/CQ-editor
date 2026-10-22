# Worker protocol (version 4)

The GUI and the Python worker exchange **one JSON object per line**, encoded
as UTF-8:

* **requests** go from the GUI to the worker's **stdin** and carry a `cmd`
  field;
* **messages** go from the worker to the GUI on a private duplicate of the
  original **stdout** and carry a `type` field;
* the worker's **stderr** is free text, shown in the Log panel line by line.

Definitions: `src/worker/protocol.rs` (Rust, authoritative for the GUI) and
`worker/cq_worker/server.py` (Python).

## Process start-up

The GUI runs:

```
<python> -u -m cq_worker     with PYTHONPATH=<worker dir>[:<existing PYTHONPATH>]
                                  PYTHONIOENCODING=utf-8
                                  CQ_EDITOR_SESSION=<worker generation: 1, 2, …>
```

`CQ_EDITOR_SESSION` numbers the GUI's successive worker processes. The worker
uses it to namespace its scene keys (see [Scene nodes](#scene-nodes)).

The worker then:

1. duplicates descriptor 1 for the protocol, and points descriptor 1 at
   stderr. C++ code printing to stdout (OpenCascade) can then never corrupt
   the message stream;
2. replaces `sys.stdout`/`sys.stderr` with streams that send `output`
   messages, and `sys.stdin` with an empty stream (scripts must not consume
   requests);
3. removes a stale `CASROOT` variable and imports CadQuery;
4. sends **`ready`**, or **`fatal`** if the import failed.

```json
{"type":"ready","protocol":4,"python":"3.12.10","cadquery":"2.8.0","ocp":"7.9.3.1.1","executable":"C:\\…\\python.exe","completion":true,"pid":1234}
{"type":"fatal","message":"Cannot load CadQuery: ModuleNotFoundError: …","detail":"Traceback …","executable":"…"}
```

The GUI refuses a worker whose `protocol` differs from its own
`PROTOCOL_VERSION`. **Bump the version on both sides** whenever a change is
not backward compatible.

## Ids and replies

Most requests carry an `id` chosen by the GUI. The reply carries the same
`id`. `output` and `log` messages produced while a request is handled are
tagged with that request's id. This is how console output ends up in the
console and script output in the Log panel.

A failing request is answered with:

```json
{"type":"error","id":7,"message":"KeyError: \"Unknown object 's1-n3'; it may have been cleared or come from a previous Python session (render again)\""}
```

The worker handles one request at a time, in arrival order.

## Requests

| `cmd` | Fields | Reply |
| --- | --- | --- |
| `run` | `id`, `script`, `path` (or null), `debug`, `breakpoints` (line numbers), `settings` (RunSettings), `scene` (SceneSettings), `known_geometry` (mesh ids) | `run_finished`, preceded by `debug_paused` messages when debugging |
| `debug` | `action` (`continue`, `step`, `step_in`, `step_out`, `stop`), `breakpoints` | Only while paused. Resumes the session |
| `release` | `keys` | None. Frees registry entries |
| `complete` | `id`, `source`, `line` (1-based), `column` (0-based, characters), `path`, `target` (`editor` or `console`) | `completions` |
| `inspect` | `id`, `key` | `inspection` |
| `inspect_geometry` | `id`, `key`, `frame`, `item` (or null for the whole frame), `scene` | `inspection_geometry` |
| `measure` | `id`, one or two `selections` (`key`, zero-based `part`, `kind`, one-based `identifier`) | `measurement` |
| `console` | `id`, `source`, `scene`, `known_geometry` | `scene_add` (if objects were shown), then `console_result` |
| `console_reset` | `id` | `console_result` |
| `export` | `id`, `keys`, `format` (`step`, `stl`, `brep`), `path`, `tolerance`, `angular_tolerance` | `export_finished` |
| `upload` | `id`, `url`, `file_type` (`step`, `stl`, `py`), `keys`, `source` | `upload_finished` |
| `check_updates` | `id` | `updates` |
| `imports` | `id`, `path` | `imports` (local module files imported by the script) |
| `shutdown` | | The worker exits |

**RunSettings:** `reload_cq`, `add_script_dir_to_path`, `change_working_dir`,
`reload_imported_modules` (booleans, see [Preferences](../user-guide/preferences.md#execution)).

**SceneSettings:** `linear_tolerance`, `angular_tolerance` (display mesh),
`merge_assemblies`.

## Messages

| `type` | Fields | Meaning |
| --- | --- | --- |
| `ready` | `protocol`, `python`, `cadquery`, `ocp`, `executable`, `completion`, `pid` | Worker is ready; the version fields identify the exact CAD runtime |
| `fatal` | `message`, `detail`, `executable` | Worker cannot start |
| `output` | `id`, `stream` (`stdout`/`stderr`), `text` | Captured `print` / `sys.stderr` text |
| `log` | `id`, `level`, `source`, `message` | A Python logging record (including the script's `log()`) |
| `run_finished` | `id`, `ok`, `debug`, `stopped`, `objects`, `variables`, `error`, `duration`, `timings` | End of a run. `objects` is null when a failed plain run should keep the current scene. `timings` is described below |
| `debug_paused` | `id`, `line`, `variables`, `objects` | The debugger stopped at `line`. `objects` are the CadQuery locals |
| `completions` | `id`, `available`, `items` (`name`, `insert`, `kind`), `signatures` | |
| `inspection` | `id`, `key`, `frames` (`type`, `value`, `items` (`type`, `value`, `displayable`)) | Workplane stack, newest first |
| `inspection_geometry` | `id`, `meshes`, `plane` (`origin`, `x_dir`, `z_dir`, `size`) | Overlay geometry |
| `measurement` | `id`, `identity`, `entities`, `pair` | Exact properties and optional two-entity relationship calculated by OCCT |
| `scene_add` | `id`, `objects` | Objects shown from the console |
| `console_result` | `id`, `more` | `more` is true when the statement is incomplete |
| `export_finished` | `id`, `path` | |
| `upload_finished` | `id`, `status`, `body` | HTTP status and response body |
| `updates` | `id`, `current`, `newer` | CadQuery release tags newer than `current` |
| `imports` | `id`, `paths` | |
| `error` | `id`, `message` | A request failed |

`error` (in `run_finished`) is a ScriptError: `type_name`, `message`,
`frames` (`file`, `line`, `function`, `code`), `line` (the last line of the
script involved) and `text` (Python's formatted traceback). The editor
buffer appears as the file `<cq-editor>`.

## Scene nodes

```json
{
  "key": "s1-n4", "name": "bracket", "kind": "Workplane", "inspectable": true,
  "color": [0.27, 0.51, 0.71], "alpha": null, "edge_color": null,
  "parts": [{ "mesh": { … }, "transform": [16 floats, column-major], "color": null, "alpha": null }],
  "children": []
}
```

* `key` identifies the object in the worker's registry. It is used for
  `export`, `inspect`, `upload` and `release`. Keys are opaque to the GUI.
  They have the form `s<session>-n<counter>`, or `n<counter>` when
  `CQ_EDITOR_SESSION` is not set. A restarted worker therefore never reuses
  the keys of objects that are still on screen, and releasing such a stale
  key is a no-op.
* An assembly is a node whose `children` mirror the assembly tree. Each
  child carries its own parts and placement.
* Colors are sRGB floats in 0–1. `null` means "use the default".

### Meshes

A full mesh:

| Field | Encoding |
| --- | --- |
| `id` | Content id: BLAKE2b of the shape's binary BREP and the tolerances |
| `positions`, `normals` | base64 of little-endian `f32` triples |
| `indices` | base64 of little-endian `u32`, three per triangle |
| `edges` | base64 `f32` triples, two points per segment |
| `points` | base64 `f32` triples (free vertices) |
| `bbox` | `[[min x, y, z], [max x, y, z]]` |
| `topology` | `triangle_faces` (OCCT face id per pick triangle), `segment_edges` (OCCT edge id per segment), `vertices`, and `face_positions` / `face_indices`: separate pick triangles, normally empty so that `triangle_faces` follows the display `indices` |

A **reference** is just `{"id": "…"}`. The worker sends it instead of the
arrays when the id is in the request's `known_geometry` or was already sent
earlier for the same request, which includes the previous `debug_paused`
messages of a debugging session. The GUI resolves it from its mesh library
and therefore pins every advertised and received mesh until the request
ends (`run_finished`, `console_result` or a worker restart), even when a
paused view no longer shows it. `scene_add` carries its request `id` for the
same reason.

`known_geometry` lists every mesh the GUI still holds: the shown ones and
up to 128 MB of recently used ones, so a render after Clear or a debugging
session can also receive references.

Binary arrays are decoded on the GUI's reader thread, so large models never
stall the interface. Base64 inside JSON is kept on purpose: for a message
with 5 MB of mesh arrays, encoding and serializing take about 35 ms in the
worker and parsing about 6 ms in the GUI, against more than a second of
meshing. Unchanged geometry is not sent at all (see references above).

`timings` in `run_finished` reports where the time went, in seconds for the
whole request (a debugging session included): `script` (equal to
`duration`), `identify` (content ids), `tessellate` (meshing of geometry not
sent as a reference), `encode` (base64), the mesh counts `meshed`, `cached`
(served from the worker's cache) and `referenced`, `mesh_bytes` (raw arrays
sent in full) and `sent_at` (worker wall clock, seconds since the Unix
epoch). The GUI adds the transfer time, its own parsing and scene decoding,
and writes one *Run timings* line to the Log panel; GPU uploads are logged
at the debug level.

### Topology identity and measurements

The topology arrays associate display primitives with the 1-based OCCT face,
edge and vertex order of one live registry entry. A `measure` request resolves
those references with `RegistryEntry.subshape`; it never measures the display
mesh. Registry shapes are already placed in world coordinates, including
assembly locations.

`measurement.identity` is always
`{"scope":"scene_entry","persistent":false}`. This is an explicit contract:
an id remains usable only while its scene key is alive. OCCT subshape order is
not a persistent naming scheme across rebuilds or remodeling.

Each entity returns `geometry_type`, `center` and applicable `position`,
`length`, `area`, `radius` and `diameter` values as JSON doubles. A two-entity
response adds exact minimum `distance`, the two closest points and, when the
relationship is well-defined, `angle_degrees`/`angle_kind`. Angles currently
cover straight edge/edge, planar face/face and straight edge/planar face.

## Debugging sessions

```
GUI                                   worker
 │  run {debug: true, breakpoints}      │
 │ ───────────────────────────────────► │ runs under sys.settrace
 │            debug_paused {line: 9}    │ pauses, waits for a command
 │ ◄─────────────────────────────────── │
 │  complete / inspect / measure / export │ served while paused
 │ ───────────────────────────────────► │
 │  debug {action: "step"}              │
 │ ───────────────────────────────────► │ resumes
 │            debug_paused {line: 10}   │
 │ ◄─────────────────────────────────── │
 │  debug {action: "continue"}          │
 │ ───────────────────────────────────► │
 │            run_finished              │
 │ ◄─────────────────────────────────── │
```

While paused, only `complete`, `inspect`, `inspect_geometry`, `measure`, `export`,
`imports`, `release` and `check_updates` are served. Anything else is
answered with an `error` (*Not available while debugging; continue or stop
first*).

## Stopping

There is no "cancel" request: Python cannot interrupt arbitrary C++ code
safely. **Stop** kills the worker process and starts a new one. Events from
the old process are discarded by their generation number.
