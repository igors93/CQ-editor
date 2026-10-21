# CQ-editor

[![DOI](https://img.shields.io/badge/DOI-10.5281%2Fzenodo.3955112-blue.svg)](https://doi.org/10.5281/zenodo.3955112)
[![Patreon](https://badgen.net/badge/icon/patreon?icon=patreon&label)](https://www.patreon.com/jmwright)
[![Liberapay](https://badgen.net/badge/icon/liberapay?icon=liberapay&label)](https://liberapay.com/jmwright)

CadQuery script editor and 3D viewer for Linux, Windows and macOS.

![CQ-editor screenshot](https://github.com/CadQuery/CQ-editor/raw/master/screenshots/screenshot4.png)
Additional screenshots are available in [the wiki](https://github.com/CadQuery/CQ-editor/wiki#screenshots).

## Notable features

* Automatic code reloading - you can use your favourite editor
* GPU accelerated 3D view with anti-aliasing, smooth camera animations and
  zoom at the mouse pointer
* Navigation cube with rotation arrows
  * Always shows the current camera orientation
  * Click faces, edges or corners for one-click animated reorientation
* Graphical debugger for CadQuery scripts
  * Step through the script (over, in, out) and watch how your model changes
* CadQuery object stack inspector
  * Visual inspection of the current workplane and selected items
  * Insight into the evolution of the model
* Export to STEP (assemblies keep names and colors), STL and BREP
* Python console sharing the variables of the last run
* Light and dark themes, dockable panels that remember their layout
* A long running or crashing script never freezes or takes down the editor:
  press Stop (Ctrl+Shift+F5) to abort it

## How it works

The interface is a native Rust application built with
[egui](https://github.com/emilk/egui) and [wgpu](https://wgpu.rs). CadQuery
runs in a separate Python process, the *worker* (`worker/cq_worker`), which
executes scripts, tessellates the shapes and sends the meshes to the
interface. The worker sources are embedded in the executable, so all CQ-editor
needs is a Python environment with CadQuery installed.

```
┌──────────────────────────────┐   JSON lines    ┌─────────────────────────────┐
│ cq-editor (Rust)             │ ──────────────► │ python -m cq_worker         │
│ editor, 3D view, panels      │   stdin/stdout  │ runs scripts with CadQuery, │
│                              │ ◄────────────── │ meshes, debugger, export    │
└──────────────────────────────┘                 └─────────────────────────────┘
```

## Installation

1. Install CadQuery in a Python environment, for example:

   ```sh
   python -m venv .venv
   .venv/bin/pip install cadquery jedi   # .venv\Scripts\pip on Windows
   ```

   `jedi` is optional and enables code completion.

2. Build CQ-editor with a recent stable Rust toolchain:

   ```sh
   cargo build --release
   ```

   The executable is `target/release/cq-editor` (`cq-editor.exe` on Windows).

3. Run it, optionally with a script to open:

   ```sh
   cq-editor my_part.py
   ```

CQ-editor finds Python in this order: the `--python PATH` option, the
interpreter set in *Edit > Preferences > General*, the `CQ_EDITOR_PYTHON`
environment variable, the active virtual or conda environment, a `.venv`
folder next to the executable or in the working directory, and finally
`python` (`python3`) on `PATH`.

### Building on Windows with the GNU toolchain

The `x86_64-pc-windows-gnu` toolchain does not need Visual Studio, but some
dependencies need `as` and `dlltool` from MinGW-w64 binutils on `PATH`
(for example from [WinLibs](https://winlibs.com) or MSYS2). With the MSVC
toolchain nothing extra is needed.

## Usage

Scripts are regular CadQuery Python files. CQ-editor provides these names in
the script namespace:

| Name | Purpose |
| --- | --- |
| `show_object(obj, name=None, options=None, **options)` | Show `obj`. Options: `color`, `alpha` (opacity 0-1), `rgba`, `edgecolor`. Colors can be names (`"steelblue"`), `"#rrggbb"`, `cq.Color` or tuples. |
| `debug(obj, name=None)` | Show `obj` as a translucent red overlay. |
| `log(value)` | Write to the Log panel. |
| `rand_color(alpha=1.0, cfloat=False)` | A random dark color for `show_object` options. |
| `cq` | The `cadquery` module. |

When a script never calls `show_object`, every CadQuery object bound to a
variable is shown. `print` output goes to the Log panel.

### Mouse

| Action | Control |
| --- | --- |
| Orbit | Left drag (turntable or trackball, see Preferences) |
| Pan | Middle drag or Ctrl + left drag |
| Zoom | Mouse wheel (at the pointer), right drag or Shift + left drag |
| Select | Click a face, edge or vertex; Ctrl + click adds to the selection |
| Zoom to object | Double-click |

### Keyboard

| Command | Shortcut |
| --- | --- |
| Render | F5 |
| Debug / stop debugging | Ctrl+F5 |
| Step, step in, step out, continue | Ctrl+F10, Ctrl+F11, Ctrl+Shift+F11, Ctrl+F12 |
| Stop a running script | Ctrl+Shift+F5 |
| Fit, Iso, Top, Bottom, Front, Back, Left, Right | Shift+F1 … Shift+F8 |
| Wireframe, Shaded | Shift+F9, Shift+F10 |
| Toggle comment | Ctrl+/ |
| Auto-complete | Ctrl+Space or Alt+/ |
| Search, next, previous | Ctrl+F, F3, Alt+F3 |
| New, open, save, save as | Ctrl+N, Ctrl+O, Ctrl+S, Ctrl+Shift+S |
| Preferences | Ctrl+, |

Click the line numbers to toggle breakpoints.

## Development

```
src/            the Rust application
  app.rs        application state, frame loop and actions
  editor/       code editor: highlighting, text operations
  viewer/       3D view: camera, wgpu renderer, picking, navigation cube
  panels/       objects, variables, inspector, console, traceback, log
  dialogs/      preferences, upload and message windows
  ui/           menu bar, toolbar, status bar
  worker/       worker process management and protocol types
worker/         the Python worker package (cq_worker)
tests/rust/      Rust tests, grouped by application area
tests/worker/    Python worker tests, grouped by execution, geometry and protocol
examples/       example scripts (File > Examples)
```

* `cargo run -- examples/01_hello_box.py` starts a debug build; debug builds
  load the worker straight from `worker/`, so Python changes apply on restart.
* `cargo test` runs the Rust tests, `pytest tests/worker` (with `worker` on
  `PYTHONPATH` and CadQuery installed) the worker tests.
* `CQ_EDITOR_WORKER_DIR` points any build at another worker directory.

## Getting Help

For general questions and discussion about CQ-editor, please create a [GitHub Discussion](https://github.com/CadQuery/CQ-editor/discussions).

## Reporting a Bug

If you believe that you have found a bug in CQ-editor, please ensure the following.

* You are not running a CQ-editor fork, as these are not always synchronized with the latest updates in this project.
* You have searched the [issue tracker](https://github.com/CadQuery/CQ-editor/issues) to make sure that the bug is not already known.

If you have already checked those things, please file a [new issue](https://github.com/CadQuery/CQ-editor/issues/new) with the following information.

* Operating System (type, version, etc) - If running Linux, please include the distribution and the version.
* How CQ-editor was installed.
* Python and CadQuery versions (shown in the status bar and in Help > About).
* Steps to reproduce the bug.
