# Architecture

CQ-editor is two programs that talk over a pipe:

```
┌─────────────────────────────────────┐   requests (JSON lines, stdin)   ┌──────────────────────────────────┐
│ cq-editor  (Rust)                   │ ───────────────────────────────► │ python -m cq_worker              │
│                                     │                                  │                                  │
│ egui + egui_dock   window, panels   │   messages (JSON lines, stdout)  │ runs scripts with CadQuery       │
│ wgpu               3D renderer      │ ◄─────────────────────────────── │ tessellates shapes (OCCT)        │
│ scene, picking, camera, documents   │                                  │ debugger, inspector, console     │
│ recovery, file watching, settings   │   stderr → Log panel             │ export, upload, completion (jedi)│
└─────────────────────────────────────┘ ◄─────────────────────────────── └──────────────────────────────────┘
```

**Why two processes?** CadQuery and OpenCascade are Python and C++ code that
can block for a long time or crash on bad geometry. Running them in a child
process means:

* the interface never freezes, even during a long boolean operation;
* *Stop* is reliable: the GUI kills the process and starts a fresh one;
* a segmentation fault inside OpenCascade costs a restart, not your
  unsaved work;
* the GUI does not depend on Python at build time. Any environment with
  CadQuery can be used at run time.

The worker package (`worker/cq_worker`) and the example scripts are compiled
into the executable by `build.rs`. At run time `embedded::worker_dir`
extracts the package into the data folder (`worker-<content hash>/`) and puts
it on `PYTHONPATH`. Debug builds use `worker/` from the source tree directly,
and `CQ_EDITOR_WORKER_DIR` overrides both.

## Rust side (`src/`)

| Module | Responsibility |
| --- | --- |
| `main.rs` | Command line, window options, start-up |
| `app.rs` | Application state (`AppState`), the frame loop, action execution, worker message handling. `CqEditorApp` wraps it with the dock layout and implements `eframe::App` |
| `commands.rs` | `Action`, everything the user can ask for, and the keyboard shortcut table |
| `document.rs` | The open script: UTF-8 decoding, BOM and line-ending preservation, atomic saves, external change classification |
| `recovery.rs` | Crash-recovery session and bounded local history |
| `file_watcher.rs` | Polling watcher behind *Automatic reload* |
| `settings.rs` | Preferences, persisted by eframe |
| `scene.rs` | What the view shows: nodes, parts, meshes, visibility, colors, selection, topology selection, inspector overlay |
| `spatial.rs` | Bounding-volume hierarchies for picking: triangles, and generic boxes for scene parts, edge segments and vertices |
| `viewer/` | 3D view: `camera.rs` (orbit camera, standard views, animation), `renderer.rs` (wgpu pipelines), `shaders.wgsl`, `picking.rs`, `overlays.rs` (navigation cube, axes indicator) |
| `editor/` | Code editor: `highlight.rs` (Python tokenizer), `text_ops.rs` (pure text edits: indent, comment, auto-indent) |
| `panels/` | Objects, Measurements, Variables, CQ Inspector, Console, Traceback, Log, and the default dock layout |
| `dialogs/` | Preferences, Upload, About, unsaved changes, external change, recovery, local history, messages |
| `ui/` | Menu bar, toolbar, status bar |
| `worker/` | Child process management (`mod.rs`), interpreter discovery (`launch.rs`), protocol types (`protocol.rs`) |
| `embedded.rs` | Access to the files embedded by `build.rs` |
| `logging.rs` | The bounded, application-wide log behind the Log panel |
| `theme.rs` | Light/dark visuals, fonts, editor and viewer colors |

### The frame loop and actions

egui is an immediate-mode toolkit: the whole interface is rebuilt every
frame. To keep that manageable, widgets **never change application state
directly**. They push an `Action` (see `commands.rs`), and `app.rs` runs the
queued actions after the UI of the frame has been built:

```
begin_frame   apply theme/scale · process worker events · poll file watcher · recovery checkpoint · collect shortcuts
build UI      menu bar · toolbar · dock (editor, view, panels) · status bar · dialogs   → push Actions
execute       for each Action: open/save/run/select/export/…  → may send worker Requests
end_frame     handle window close requests (unsaved changes guard)
```

Menus, toolbar buttons, shortcuts and context menus therefore share one code
path per command, and the shortcut shown in a menu comes from the same table
that handles the key press.

### Talking to the worker

`worker::Worker` owns the child process and three helper threads:

* a **writer** thread that serializes requests to stdin;
* a **reader** thread that parses stdout lines into `Message`s and decodes
  the base64 mesh arrays, so the UI thread never parses large payloads;
* a **stderr** thread that forwards lines to the Log panel.

Events reach the UI through a channel and wake egui up with
`request_repaint`. Each process gets a *generation* number, so late events
of a killed process are ignored. Requests that expect an answer get an id,
which `AppState::pending` maps to what the answer is for (run, completion,
inspection, export, …).

### Rendering

`viewer/renderer.rs` draws the scene off screen into an sRGB texture with 4×
MSAA when available. egui then shows it like an image, through a non-sRGB
view of the same texture so colors are not encoded twice. Draw order:
background (flat or gradient), opaque faces, edges and points, transparent
faces back to front, then the inspector overlay on top.

`shaders.wgsl` lights faces with a camera-attached three-point setup (key,
fill, rim) and lights both sides of a face, so open shells look right.
Edges and points are camera-facing quads expanded in screen space, which
gives constant pixel widths at any zoom. Faces get a depth bias so edges
stay visible on top of them.

Assembly parts are **instances**: one mesh plus a transform per part.
Identical geometry is uploaded to the GPU once.

### Picking

`viewer/picking.rs` hit-tests faces exactly with a ray, accelerated by the
BVH in `spatial.rs`. Edges and vertices are matched in screen space within 6
pixels, and a face in front of them occludes them. The worker sends, with
every mesh, which triangle belongs to which OCCT face and which segment to
which edge (`MeshTopology`). A click can therefore name *Face 6* rather than
"triangle 1234".

Picking never scans the whole scene. It runs in two phases:

* **Broad phase:** the viewer keeps a `SceneIndex`, a BVH over the
  world-space bounds of every visible part. A part is kept when the pointer
  ray hits its box or, when edges or vertices may be picked, when its
  projected box lies within the pixel tolerance of the pointer. The index is
  rebuilt only when `Scene::pick_revision` changes (parts added, removed or
  shown/hidden), never on camera moves or selection changes.
* **Narrow phase:** each remaining mesh tests its triangles, edge segments
  and vertices through its own lazily built BVHs, shared by every instance
  of the mesh. Edge and vertex boxes are pruned in screen space.

Both phases return candidates in scene and index order, so ties between
coincident entities resolve exactly as a linear scan would. The camera
matrices are computed once per pick (`camera::project_with`).
`pick_latency_on_growing_assemblies` measures hover latency as the part
count grows (see [Testing](testing.md#benchmarks)).

GPU picking (rendering entity ids and reading a pixel back) was considered
and not adopted for now: the readback adds at least a frame of latency and
a GPU-to-CPU synchronization, while the CPU path above stays near 0.1 ms per
hover in the benchmark's assembly of 16,384 parts. Revisit it only if a
measured scene makes the CPU path the bottleneck again.

## Python side (`worker/cq_worker/`)

| Module | Responsibility |
| --- | --- |
| `__main__.py` | Entry point: takes over stdout, imports CadQuery, reports `ready` or `fatal` |
| `protocol.py` | JSON-lines transport. Captures `print` output and native writes to descriptor 1 |
| `server.py` | Request dispatch, the run loop, debug pause loop, session state |
| `execution.py` | Running the editor buffer as `__cq_main__`: injected API, environment, module reloading, tracebacks |
| `debugger.py` | Line stepping with `sys.settrace`, tracing only the script's own frames |
| `scene.py` | Shown objects → scene nodes: colors, assemblies, the `Registry` of live objects by key |
| `tessellation.py` | Meshing, the content-addressed `GeometryCache`, `TopologyIndex` |
| `inspector.py` | Workplane stack description and per-step geometry |
| `console.py` | The persistent interactive console |
| `completion.py` | jedi completions and call signatures |
| `exporting.py` | STEP / STL / BREP export |
| `network.py` | Model upload and CadQuery release checks, standard library only |

The worker is **single threaded**: it handles one request at a time. While a
debugging session is paused, `Server.wait_for_debug_action` keeps serving
read-only requests (completion, inspection, export, …), so the GUI stays
useful at a breakpoint.

### The registry

The GUI never holds CadQuery objects. It holds **keys** (`s1-n1`, `s1-n2`,
…). The worker's `Registry` maps each key to the live object, its
world-space shape and its topology index. Export, inspection, measurement and upload
refer to objects by key. When the GUI drops objects it sends `release`, so
the worker can free them.

Keys carry the worker's session number (`CQ_EDITOR_SESSION`, the GUI's
worker generation). Objects stay on screen when Python is restarted, and the
next run releases their keys. With per-session keys, that release can never
free the new worker's objects.

### Tessellation and the geometry cache

1. BRepMesh triangulates the faces in parallel.
2. OCCT's glTF writer (`RWGltf_CafWriter`) emits positions, exact normals and
   indices from C++ in one pass. numpy reads the binary back, which is much
   faster than walking `Poly_Triangulation` from Python.
3. Edges are discretized with `GCPnts_TangentialDeflection`. Free vertices
   are sent as points.
4. Each mesh gets a **content id**: a BLAKE2b hash of the shape's binary BREP
   and the tolerances.

`GeometryCache` is an LRU of 256 MB keyed by that id. On top of it, the GUI
sends the ids of meshes it already has (`known_geometry`), and the worker
answers with a bare reference instead of the arrays. Re-running an unchanged
model, or showing the same part many times in an assembly, sends each mesh
only once: after editing one part of an assembly, only that part is meshed,
sent and uploaded again.

On the GUI side, `MeshLibrary` (`scene.rs`) holds the meshes by content id.
It pins the meshes a running request may reference and retains up to
128 MB of recently used ones beyond the shown scene. A reference, or a full
mesh for geometry it still holds, resolves to the same `Arc<Mesh>`, and the
renderer keeps GPU buffers per mesh for as long as it lives, so a render
after Clear, a debugging session and repeated runs reuse both the CPU mesh
and its GPU buffers.

## Persistence

| Data | Where | How |
| --- | --- | --- |
| Preferences, layout, window | `app.ron` | eframe persistence (`App::save`) |
| Scripts | wherever you save them | `document.rs`: temp file + atomic replace (`ReplaceFileW`/`MoveFileExW` on Windows, `rename` elsewhere), synchronized to disk |
| Recovery session | `recovery/active-session.json` | Written every 5 s while the buffer is modified |
| Local history | `recovery/history/version-*.json` | 20 newest versions per document path |

## Design principles

* **The script is the source of truth.** The GUI shows and explains what
  the code produced. Changes made in the GUI (colors, visibility) never
  rewrite the script.
* **Compatibility.** Scripts written for the original PyQt CQ-editor run
  unchanged: same `show_object` options, same namespace, same execution
  settings. User-interface equivalence and intentional differences are
  recorded separately in the [compatibility matrix](compatibility.md).
* **The UI thread never blocks** on Python, on the disk for long, or on the
  network.
* **No silent data loss.** Atomic saves, recovery checkpoints, conflict
  dialogs.
* **One code path per command** (`Action`), shared by menus, toolbar and
  shortcuts.
