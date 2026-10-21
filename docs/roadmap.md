# Roadmap

This page is the plan for CQ-editor after the Rust rewrite. It lists the
**bugs and problems** found in the current code, the **gaps** in the
experience, and the **features** that would make CQ-editor clearly more
capable than the original PyQt editor, without losing what made it useful.

Every item is based on the current source tree (`0.8.0-dev`). Bugs say how
they were found:

* **seen**: reproduced in the running application (several are visible in
  the screenshots of these docs);
* **code**: found by reading the code; the reasoning is in the item;
* **CI**: reported by the continuous integration checks.

Priorities: **P1** fix before the next release · **P2** should fix soon ·
**P3** maintenance or polish.

Status: **✅** resolved · blank: still open.

Want to help? Pick an item, open an issue that mentions its id (for example
`BUG-03`), and see [Contributing](development/contributing.md).

## Guiding principles

CQ-editor is, first of all, **an IDE for running Python CadQuery code that
builds 3D models**. Everything on this roadmap has to respect that:

1. **The code is the model.** The script stays the single source of truth.
   GUI features may *read* the model (measure, inspect, select) and may
   *write code* (insert a selector, change a parameter), but they never keep
   design intent in a hidden GUI-only state.
2. **One tool for beginners and experts.** Beginners need a guided path,
   clear errors and things they can click. Experts need speed, keyboard
   control, multi-file projects and automation. Advanced features are
   available but stay out of the way until needed (progressive disclosure).
3. **Compatibility.** Scripts written for the original CQ-editor, and the
   `show_object` conventions shared with other CadQuery tools, keep working.
4. **Never block, never lose work.** Python runs in its own process. Saves
   are atomic. Unsaved work is recoverable.

## Where we are

The rewrite is functionally complete. Scripts, `show_object` options, menus,
shortcuts and panels match the original, and several features are new
(topology picking, assembly instancing, crash recovery, local history,
conflict-safe autoreload; see the [changelog](changelog.md)).

| Check | Status |
| --- | --- |
| Rust tests (`cargo test`) | 108 passing |
| Worker tests (`pytest tests/worker`) | 135 passing |
| `cargo fmt --check`, `cargo clippy --all-targets` | clean |
| `black --check worker tests/worker` | clean (BUG-03 ✅) |

---

## 1. Bugs and problems

| Status | Id | Problem | Priority |
| :---: | --- | --- | --- |
| ✅ | [BUG-01](#bug-01--object-keys-collide-after-python-restarts) | Object keys collide after Python restarts | P1 |
| ✅ | [BUG-02](#bug-02--objects-panel-the-property-sheet-overlaps-the-tree) | Objects panel: the property sheet overlaps the tree | P1 |
| ✅ | [BUG-03](#bug-03--ci-format-check-fails) | CI format check fails | P1 |
| ✅ | [BUG-04](#bug-04--breakpoints-do-not-follow-edits) | Breakpoints do not follow edits | P1 |
| | [BUG-05](#bug-05--worker-events-wait-for-the-window-to-repaint) | Worker events wait for the window to repaint | P2 |
| | [BUG-06](#bug-06--icons-in-dialog-titles-show-as-empty-boxes) | Icons in dialog titles show as empty boxes | P2 |
| | [BUG-07](#bug-07--wrong-shortcut-in-the-search-bar-tooltip) | Wrong shortcut in the search bar tooltip | P2 |
| | [BUG-08](#bug-08--help--documentation-opens-the-old-wiki) | Help > Documentation opens the old wiki | P2 |
| | [BUG-09](#bug-09--imported-modules-are-watched-only-once) | Imported modules are watched only once | P2 |
| | [BUG-10](#bug-10--variables-values-are-unreadable) | Variables: values are unreadable | P2 |
| | [BUG-11](#bug-11--misleading-hints-after-a-failed-run) | Misleading hints after a failed run | P2 |
| | [BUG-12](#bug-12--the-view-keeps-the-previous-files-objects) | The view keeps the previous file's objects | P2 |
| | [BUG-13](#bug-13--debug-in-the-console-loses-the-variable-name) | `debug()` in the console loses the variable name | P2 |
| | [BUG-14 … BUG-20](#p3-maintenance-and-polish) | Maintenance and polish | P3 |
| | [PERF-01 … PERF-04](#performance) | Performance | |

### P1: fix before the next release

#### BUG-01 · Object keys collide after Python restarts

✅ **Resolved.** *Evidence: code; the fix was checked in the app.*

**Where:** `src/app.rs` (`start_worker`, `reset_execution_state`,
`finish_run`), `src/scene.rs` (`replace`, `append`),
`worker/cq_worker/scene.py` (`Registry`).

After **Run > Stop**, **Run > Restart Python** or a worker crash, the scene
keeps its objects. Their keys (`n1`, `n2`, …) belong to the old process. The
new worker numbers its objects from `n1` again. On the first run after the
restart:

1. the worker registers the new objects as `n1`, …;
2. `finish_run` replaces the scene and sends `release` for the *old* keys,
   which are the same strings;
3. the new worker frees its *new* objects.

As a result, the CQ Inspector, Export and Upload fail for that run with
*Unknown object 'n1'; it may have been cleared*. Objects added from the
console after a restart can also duplicate keys already in the scene.

**Resolution:** keys are now unique per worker process. The GUI passes its
worker generation in the `CQ_EDITOR_SESSION` environment variable
(`src/worker/mod.rs`), and the worker prefixes its keys with it (`s1-n1`,
then `s2-n1` after a restart; `Registry` in `worker/cq_worker/scene.py`).
Releasing the keys of a previous process is now a harmless no-op. The CQ
Inspector state is also reset when Python restarts (`start_worker` in
`src/app.rs`). Objects of the old process stay visible. Exporting them
before rendering again reports that they *come from a previous Python
session (render again)*.

Regression tests: `test_prefixed_registries_never_share_keys`,
`test_restarted_worker_keeps_objects_when_stale_keys_are_released` and
`test_worker_namespaces_scene_keys_with_the_gui_session`. Checked in the
app: render, *Run > Restart Python*, render, select the object; the CQ
Inspector shows its construction stack.

#### BUG-02 · Objects panel: the property sheet overlaps the tree

✅ **Resolved.** *Evidence: seen; the fix was checked in the app.*

**Where:** `src/panels/objects.rs`.

When the Objects pane is short, the property sheet (Type, Visible, Color,
Opacity, Size, Triangles) grows over the *CQ models* tree. The texts overlap
and nothing scrolls.

**Resolution:** the sheet is laid out by `property_sheet` in
`src/panels/objects.rs`. Its contents scroll instead of growing the panel,
and its height is capped so the tree keeps at least 80 points (or 60 % of a
very short pane). The tree's scroll area no longer insists on its default
64-point minimum, which made it spill over the sheet.

Regression tests (`tests/rust/panels/test_objects.rs`) lay out a sheet much
taller than the pane with a headless `egui::Context` and check that the tree
keeps its room above the sheet. Both fail with the previous layout. Checked
in the app: with a short Objects pane, the sheet shows a scroll bar and the
tree stays readable above it.

#### BUG-03 · CI format check fails

✅ **Resolved.** *Evidence: CI.*

**Where:** `worker/cq_worker/tessellation.py`,
`tests/worker/geometry/test_scene.py`,
`tests/worker/geometry/test_tessellation.py`.

`black --check worker tests/worker` reports these three files, so the
*worker / Format* step of `.github/workflows/ci.yml` fails on every push.

**Resolution:** the three files were reformatted with black 26.10.1, the
version CI installs; `black --check worker tests/worker` now passes. A
pre-commit hook would keep it that way.

#### BUG-04 · Breakpoints do not follow edits

✅ **Resolved.** *Evidence: code.*

**Where:** `src/editor/mod.rs` (`breakpoints: BTreeSet<usize>`).

Breakpoints are plain line numbers. Inserting or deleting lines above a
breakpoint moves the code but not the dot. The debugger then stops on a
different statement than the one marked.

**Resolution:** the editor remembers the text its line markers refer to
and, whenever the text changes (typing, editor commands, reloads, restored
versions), moves breakpoints, the error line and the debug line with
`text_ops::LineMap` (`CodeEditor::sync_markers` in `src/editor/mod.rs`).
Lines before the edited block keep their number, lines after it shift,
lines inside it map by position, and a breakpoint on a deleted line is
removed. Runs and debugger steps sync first, so Python always receives the
current line numbers.

Regression tests in `tests/rust/editor/test_text_ops.rs` (inserting,
splitting, merging, deleting and indenting lines) and
`tests/rust/editor/test_editor.rs` (breakpoints and the error line through
edits, and a fresh document after a reset).

### P2: should fix soon

#### BUG-05 · Worker events wait for the window to repaint

*Evidence: code*

**Where:** `src/app.rs`, where `begin_frame` (called from `ui()`) runs
`process_worker_events`, `poll_watcher` and `poll_recovery`.

All worker traffic, automatic reload and recovery checkpoints run as part
of drawing a frame. When the window is minimized and the platform stops
drawing it, runs appear to stall and replies pile up until it is restored.

**Fix:** drain the event channel and the timers in a hook that runs without
painting, or on a background thread that wakes the UI only for visible
changes. Verify on Windows, Linux (X11/Wayland) and macOS.

#### BUG-06 · Icons in dialog titles show as empty boxes

*Evidence: seen*

**Where:** `src/theme.rs:184-197`.

The semibold font family is cloned from the proportional family *before*
the Phosphor icon font is added. Text in that family (window titles use the
heading style) therefore has no icon glyphs. *Preferences* shows `□` before
its title (see the Preferences screenshot). *Upload Model* uses the same
code path.

**Fix:** call `egui_phosphor::add_to_fonts` before cloning, or append the
Phosphor font to the semibold family.

#### BUG-07 · Wrong shortcut in the search bar tooltip

*Evidence: code*

**Where:** `src/editor/mod.rs:592`.

The *Previous match* button says *Shift+F3*. The actual shortcut is
**Alt+F3** (`src/commands.rs`), and **Shift+F3 is the Top view**.

**Fix:** derive tooltip texts from `commands::shortcut()` instead of
hard-coding them.

#### BUG-08 · Help > Documentation opens the old wiki

*Evidence: code*

**Where:** `src/app.rs:367`.

The menu opens `https://github.com/CadQuery/CQ-editor/wiki`, which describes
the PyQt editor (installation with conda, PyQt preferences).

**Fix:** open these docs (`docs/README.md` on GitHub), and add a note on the
wiki pointing here.

#### BUG-09 · Imported modules are watched only once

*Evidence: code*

**Where:** `src/app.rs` (`sync_watcher`, handling of `Message::Imports`).

With *Autoreload: watch imported modules*, the list of local imports is
computed only when autoreload, that option or the file path changes. A
module imported later is not watched until one of those changes. The merged
list is also de-duplicated with `Vec::dedup` without sorting, so a path can
be watched twice.

**Fix:** refresh the import list after each successful run (or each save),
and de-duplicate with a set.

#### BUG-10 · Variables: values are unreadable

*Evidence: seen*

**Where:** `src/panels/variables.rs`.

The *Value* column gets only the space left after *Name* and *Type*, so in
the default layout every value shows as `…` (see the debugger screenshot).

**Fix:** let the value column take the remaining width, make columns
resizable, and offer a detail view (or copy) for long values.

#### BUG-11 · Misleading hints after a failed run

*Evidence: seen*

**Where:** `src/viewer/mod.rs` (empty-view hint), `src/panels/mod.rs:137`
(`empty_state`).

* After a failed run with nothing on screen, the view still says *Press F5
  to render the script*, although F5 was just pressed. It should point to
  the error, for example *The last run failed, see Traceback*.
* Empty-state texts are laid out justified, so wrapped lines get uneven word
  spacing (*Select  a  workplane  in  Objects…* in the CQ Inspector).

#### BUG-12 · The view keeps the previous file's objects

*Evidence: seen*

**Where:** `src/app.rs` (`set_document`).

Opening another file resets the editor and the traceback, but the 3D view,
Objects panel, selection and CQ Inspector still show the previous file's
model until the next successful run. With a failing new script, the stale
model looks like the result of that script.

**Fix:** clear the scene when the document changes (or grey it out with a
"from previous file" notice), and consider rendering on open (an option).

#### BUG-13 · `debug()` in the console loses the variable name

*Evidence: code*

**Where:** `worker/cq_worker/console.py` (`"debug": self.shown.debug`),
`worker/cq_worker/execution.py` (`ShownObjects.debug`).

`ShownObjects.debug` finds the variable name two frames up, which is right
for the script wrapper. The console binds the method directly, so the
lookup lands in the interpreter's internals. The object gets a generic name
(*Workplane 1*) instead of its variable name.

**Fix:** wrap it in the console the way `show_object` is wrapped, or pass the
caller frame explicitly.

### P3: maintenance and polish

| Status | Id | Problem | Where | Suggested fix |
| :---: | --- | --- | --- | --- |
| | BUG-14 *(code)* | `--help` and command-line errors print nothing in Windows release builds, which have no console | `src/main.rs` | Attach to the parent console, or show a message box |
| | BUG-15 *(code)* | Every worker version extracts a new `worker-<hash>/` folder in the data directory; old ones are never removed | `src/embedded.rs` | Delete other `worker-*` folders after a successful extraction |
| | BUG-16 *(code)* | Each recovery checkpoint (every 5 s while modified) reads and parses **every** history file of **every** document. History of deleted or renamed files is never pruned, and all untitled buffers share one bucket | `src/recovery.rs` (`history_files`, `prune_history`) | One sub-folder per document (hash of the path) with a small index; prune by age and total size |
| | BUG-17 *(code)* | Files that are not UTF-8 cannot be opened at all | `src/document.rs` | Offer to open as Windows-1252 / Latin-1 and convert to UTF-8 on save, with a clear notice |
| | BUG-18 *(code)* | The Windows `.exe` has no embedded icon or version information, so Explorer shows a generic icon for the file (the window icon is set at run time) | `build.rs` | Compile a resource (for example with `winresource`). The GNU toolchain needs `windres` |
| | BUG-19 *(code)* | Building with the Windows GNU toolchain needs MinGW binutils and a hand-made `libshlwapi.a` ([Building](development/building.md#gnu-no-visual-studio)) | dependency tree (`webbrowser` via `egui-winit`) | Investigate disabling the link-opening feature, since links already go through the `open` crate |
| | BUG-20 *(code)* | The Log panel lives only in memory: after a GUI panic or crash, the messages that explain it are gone | `src/logging.rs` | Also write a rotating log file in the data folder, and mention it in bug reports |

### Performance

| Status | Id | Problem | Where | Suggested fix |
| :---: | --- | --- | --- | --- |
| | PERF-01 | Pick topology walks every triangulation node and triangle **in Python** (`_topology_faces`), which is the slow path the glTF writer was introduced to avoid. The pick triangles also duplicate the display triangles in memory and over the pipe | `worker/cq_worker/tessellation.py:449` | Build a per-triangle face id array for the display mesh (glTF primitives map to faces) and pick on the display triangles; vectorize the rest with numpy |
| | PERF-02 | Each tessellation writes and reads a temporary `.glb` file on disk | `tessellation.py:306` | Reuse one temporary directory, or use an in-memory stream if OCCT allows it; measure first |
| | PERF-03 | Hovering re-runs edge and vertex picking for every segment of every object on each pointer move. Faces use a BVH, edges do not | `src/viewer/picking.rs` | Spatial index (screen grid or BVH) for segments; skip hover picking while a drag is in progress or the pointer is still |
| | PERF-04 | Topology selection exists only in the GUI. The worker keeps the live subshapes (`RegistryEntry.subshape`) but no request uses them | `worker/cq_worker/scene.py`, `protocol.rs` | Expose selected subshapes to the worker (see FEAT-B1/B2). This turns existing work into features |

---

## 2. Gaps in the everyday experience

Small, concrete improvements to what already exists:

| Status | Improvement |
| :---: | --- |
| | **Undo/Redo in the Edit menu.** The text field keeps its own undo history, but there are no menu entries and no documented shortcuts. Check that indent, comment and completion edits are undone as one step |
| | **Completion while typing names.** The popup already opens after `.` and on Ctrl+Space; also offer it after a few letters of a name (configurable) |
| | **Go to line** (Ctrl+G), **Replace** (Ctrl+H) and **whole word** search |
| | **Hover documentation:** CadQuery docstrings and signatures in a tooltip (jedi already provides them) |
| | **Remember breakpoints** per file across sessions |
| | **Object property sheet:** show the selected face's area or edge's length and center, not just its id |
| | **Better first-run experience:** when Python or CadQuery is missing, offer to pick an interpreter or open the installation guide from the dialog |

---

## 3. Features to implement

Labels: 🟢 mostly for beginners · 🔵 mostly for advanced users · ⚪ both.

### A. Learn by doing (beginner path)

| Status | Id | Feature | Why it keeps the original idea |
| :---: | --- | --- | --- |
| | FEAT-A1 🟢 | **Start page** with *New from template*, recent files and an examples gallery with thumbnails | Every template is a commented script; you always start from code |
| | FEAT-A2 🟢 | **Guided lessons**: a track of examples where each step explains one CadQuery concept and checks the result (for example the expected volume) | Learning happens by editing and running real code |
| | FEAT-A3 🟢 | **Friendly errors**: translate common OpenCascade failures (`StdFail_NotDone`, `BRep_API: command not done`) into hints such as *the fillet radius may be larger than the edge allows*, with a link to the line | Same traceback, plus advice |
| | FEAT-A4 🟢 | **Selector preview**: put the cursor on `.faces(">Z")` or `.edges("\|Z")` and the matching entities light up in the view | Makes the hardest concept for beginners visible, without leaving the code |
| | FEAT-A5 🟢 | **Snippets** for frequent patterns (box with fillets, hole pattern, revolve, assembly) available from completion | Inserts ordinary code |
| | FEAT-A6 ⚪ | **Translations** of the interface (starting with Portuguese and Spanish) | Lowers the entry barrier; the API stays English |

### B. Code ↔ model link (the core differentiator)

| Status | Id | Feature | Notes |
| :---: | --- | --- | --- |
| | FEAT-B1 ⚪ | **Selection to code**: after picking faces or edges, *Copy selector* / *Insert selector* generates a CadQuery expression such as `.faces(">Z")` or `.edges("\|Z and >Y")`, with an index-based fallback when no simple selector matches | The worker already holds the subshapes (PERF-04) |
| | FEAT-B2 🔵 | **Picked entities in the console**: `selected()` returns the selected `cq.Face` / `cq.Edge` objects for measurement and experiments | Uses `Registry.subshape` |
| | FEAT-B3 ⚪ | **Parameter panel**: top-level numeric assignments (`width = 40`) appear as spin boxes and sliders. Changing one rewrites that line and re-renders | The script stays the source of truth; the panel only edits literals |
| | FEAT-B4 ⚪ | **Jump to source**: from an object or a CQ Inspector step to the line that created it | Record line numbers while the script runs |
| | FEAT-B5 ⚪ | **Measure tool**: distance, angle, area, length, radius and bounding box from picked entities, with *copy as code* | Reads the model; never edits it |

### C. 3D view

| Status | Id | Feature |
| :---: | --- | --- |
| | FEAT-C1 ⚪ | Section / clipping planes, with the cut face filled |
| | FEAT-C2 ⚪ | Exploded view and isolate / hide others for assemblies |
| | FEAT-C3 ⚪ | X-ray and hidden-line display modes; per-object edge visibility |
| | FEAT-C4 🔵 | Named views (saved cameras) and view presets per file |
| | FEAT-C5 ⚪ | High-resolution and transparent-background screenshots, turntable animation export |
| | FEAT-C6 🔵 | Large-model mode: frustum culling, level of detail, progress and cancellation for tessellation |
| | FEAT-C7 ⚪ | Ground grid with units and a scale bar |

### D. Projects and files (advanced path)

| Status | Id | Feature |
| :---: | --- | --- |
| | FEAT-D1 🔵 | **Multiple documents in tabs**, plus a project sidebar with the folder's file tree, for multi-module projects |
| | FEAT-D2 🔵 | **Headless command line**: `cq-editor --export part.step script.py` (and `--check`) for batch jobs and CI, reusing the worker |
| | FEAT-D3 ⚪ | **More export formats** in the menu: glTF/GLB (the worker already uses OCCT's glTF writer), 3MF, AMF, VRML, and DXF/SVG of sections, with an options dialog (units, assembly mode) |
| | FEAT-D4 ⚪ | **Import by drag and drop**: dropping a STEP, STL or DXF file inserts the matching `cq.importers…` line into the script |
| | FEAT-D5 🔵 | **Git awareness**: changed-lines markers in the gutter and a diff view against the last commit (the conflict dialog's comparison view is a start) |
| | FEAT-D6 🔵 | **Run selection**: execute the selected lines in the console |

### E. Running code

| Status | Id | Feature |
| :---: | --- | --- |
| | FEAT-E1 ⚪ | **Gentle stop**: interrupt pure-Python loops with `KeyboardInterrupt` first, so the console and the cache survive. Restart the process only if the script does not respond |
| | FEAT-E2 🔵 | **Per-line profiling**: time spent per line and per CadQuery operation, shown in the gutter |
| | FEAT-E3 🟢 | **Environment manager**: create or choose a virtual environment and install CadQuery, jedi and add-on packages from Preferences |
| | FEAT-E4 🟢 | **Installers with a bundled Python and CadQuery** (Windows installer or winget, macOS `.app` / `.dmg`, Linux AppImage or Flatpak), with "Open with CQ-editor" for `.py` files |
| | FEAT-E5 🔵 | **Self-update check** for CQ-editor (today only CadQuery releases are checked) |

### F. Extensibility (advanced path)

| Status | Id | Feature |
| :---: | --- | --- |
| | FEAT-F1 🔵 | **Other OCP-based libraries**: display any object that wraps a `TopoDS_Shape` (`.wrapped`), so [build123d](https://github.com/gumyr/build123d) and `cq_warehouse` parts can be shown with `show_object` |
| | FEAT-F2 🔵 | **Plugin API**: Python plugins loaded by the worker that register exporters, `show_object` handlers or commands; GUI menu entries that run Python snippets |
| | FEAT-F3 🔵 | **User themes**: custom syntax and viewer colors |

### G. Engineering quality

| Status | Id | Item |
| :---: | --- | --- |
| | ENG-01 | Keep CI green (the format check was fixed with BUG-03 ✅). Add `ruff` and a type checker for the worker, and coverage reports for both suites |
| | ENG-02 | Shared protocol fixtures: golden JSON messages tested by both the Rust and the Python suites, so the two sides cannot drift |
| | ENG-03 | Headless UI tests of panels and dialogs (for example with `egui_kittest`) |
| | ENG-04 | Benchmarks for tessellation, decoding and picking on large assemblies, tracked over time |
| | ENG-05 | Accessibility review (keyboard-only use, screen readers through AccessKit, contrast of both themes) |

---

## 4. Milestones

| Milestone | Goal | Contents |
| --- | --- | --- |
| **0.8: stable rewrite** | Ready to replace the PyQt release | BUG-01…BUG-13, BUG-18, ENG-01, basic installers, docs linked from Help |
| **0.9: code ↔ model** | The GUI helps write code | FEAT-A3, A4, B1, B2, B3, B5; completion while typing names, Go to line, Replace, hover docs; PERF-01…04 |
| **1.0: projects** | Comfortable for large designs | FEAT-D1, D2, D3, C1, C2, E1, E3, E4, F1 |
| **Later** | Exploratory | FEAT-A2, A6, B4, C3–C7, D4–D6, E2, E5, F2, F3, ENG-02…05 |

The order inside a milestone is flexible. Bugs come before features.
