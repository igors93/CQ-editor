# Roadmap

This page is the plan for CQ-editor after the Rust rewrite. It lists the
**bugs and problems** found in the current code, the **gaps** in the
experience, and the **features** that would make CQ-editor clearly more
capable than the original PyQt editor, without losing what made it useful.

Every item is based on the current source tree (`0.8.0`). Bugs say how
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
   The [compatibility matrix](development/compatibility.md) distinguishes
   tested equivalence, intentional changes and open gaps.
4. **Never block, never lose work.** Python runs in its own process. Saves
   are atomic. Unsaved work is recoverable.

## Where we are

The core modeling workflow is implemented and substantially tested, but full
functional equivalence is not claimed. Scripts, the `show_object` contract,
legacy shortcuts and mouse bindings have automated compatibility coverage.
Stereo rendering, the rich Jupyter console and bundled distribution remain
explicit gaps; some native GUI interactions still require an end-to-end test.
Several areas are enhanced (topology picking, assembly instancing, crash
recovery, local history and conflict-safe autoreload; see the
[changelog](changelog.md)).

| Check | Status |
| --- | --- |
| Compatibility inventory | Enforced by `compatibility_matrix_is_complete_and_evidenced` |
| Legacy projects | All eight pre-rewrite examples run in the worker suite |
| Rust and worker suites | See the latest CI run; counts are not hand-maintained |
| Format and lint | `cargo fmt`, clippy and `black` are required by CI |

---

## 1. Bugs and problems

| Status | Id | Problem | Priority |
| :---: | --- | --- | --- |
| ✅ | [BUG-01](#bug-01--object-keys-collide-after-python-restarts) | Object keys collide after Python restarts | P1 |
| ✅ | [BUG-02](#bug-02--objects-panel-the-property-sheet-overlaps-the-tree) | Objects panel: the property sheet overlaps the tree | P1 |
| ✅ | [BUG-03](#bug-03--ci-format-check-fails) | CI format check fails | P1 |
| ✅ | [BUG-04](#bug-04--breakpoints-do-not-follow-edits) | Breakpoints do not follow edits | P1 |
| ✅ | [BUG-05](#bug-05--worker-events-wait-for-the-window-to-repaint) | Worker events wait for the window to repaint | P2 |
| ✅ | [BUG-06](#bug-06--icons-in-dialog-titles-show-as-empty-boxes) | Icons in dialog titles show as empty boxes | P2 |
| ✅ | [BUG-07](#bug-07--wrong-shortcut-in-the-search-bar-tooltip) | Wrong shortcut in the search bar tooltip | P2 |
| ✅ | [BUG-08](#bug-08--help--documentation-opens-the-old-wiki) | Help > Documentation opens the old wiki | P2 |
| ✅ | [BUG-09](#bug-09--imported-modules-are-watched-only-once) | Imported modules are watched only once | P2 |
| ✅ | [BUG-10](#bug-10--variables-values-are-unreadable) | Variables: values are unreadable | P2 |
| ✅ | [BUG-11](#bug-11--misleading-hints-after-a-failed-run) | Misleading hints after a failed run | P2 |
| ✅ | [BUG-12](#bug-12--the-view-keeps-the-previous-files-objects) | The view keeps the previous file's objects | P2 |
| ✅ | [BUG-13](#bug-13--debug-in-the-console-loses-the-variable-name) | `debug()` in the console loses the variable name | P2 |
| ✅ | [BUG-14 … BUG-20](#p3-maintenance-and-polish) | Maintenance and polish | P3 |
| ✅ | [BUG-21 … BUG-32](#found-in-the-october-2026-review) | Found in the October 2026 reviews | P1–P3 |
| ✅ | [PERF-01 … PERF-04](#performance) | Performance | |

### P1: fix before the next release

#### BUG-01 · Object keys collide after Python restarts

✅ **Resolved.** *Evidence: code; the fix was checked in the app.*

**Where:** `src/app/worker_events.rs` (`start_worker`,
`reset_execution_state`), `src/app/runs.rs` (`finish_run`),
`src/scene.rs` (`replace`, `append`),
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
`src/app/worker_events.rs`). Objects of the old process stay visible. Exporting them
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

✅ **Resolved.** *Evidence: code.*

**Where:** `src/app/mod.rs`, where `begin_frame` (called from `ui()`) ran
`process_worker_events`, `poll_watcher` and `poll_recovery`.

All worker traffic, automatic reload and recovery checkpoints ran as part
of drawing a frame. When the window was minimized and the platform stopped
drawing it, runs appeared to stall and replies piled up until it was
restored.

**Resolution:** these steps moved to `AppState::logic`, called from
eframe's `App::logic` (eframe 0.36). eframe calls it before every frame and,
while the window is minimized or hidden, whenever a repaint is requested,
without running a UI pass. The worker's reader and stderr threads request a
repaint for every message, and the watcher and recovery timers schedule
theirs, so all of them keep running. Only theming and shortcuts stay in
`begin_frame`. eframe throttles repaints of hidden windows to 100 ms, which
bounds the latency. Checked in the app: a three-second script rendered and
then minimized at once was finished, logged and shown at the time it ended,
not when the window was restored.

#### BUG-06 · Icons in dialog titles show as empty boxes

✅ **Resolved.** *Evidence: seen; regression test.*

**Where:** `src/theme.rs` (`fonts`).

The semibold font family was cloned from the proportional family *before*
the Phosphor icon font was added. Text in that family (window titles use the
heading style) therefore had no icon glyphs: *Preferences* showed `□` before
its title, and *Upload Model* used the same code path.

**Resolution:** the semibold family is built after
`egui_phosphor::add_to_fonts`, so it has the icons while its letters still
come from Inter SemiBold. `headings_render_icons_like_body_text`
(`tests/rust/app/test_theme.rs`) loads the fonts into a headless context and
checks that headings find the icon glyphs. Checked in the app: the
Preferences title shows its gear icon.

#### BUG-07 · Wrong shortcut in the search bar tooltip

✅ **Resolved.** *Evidence: code and regression test.*

**Where:** `src/editor/mod.rs` (`CodeEditor::search_bar`).

The *Previous match* button says *Shift+F3*. The actual shortcut is
**Alt+F3** (`src/commands.rs`), and **Shift+F3 is the Top view**.

**Resolution:** both search-button tooltips now derive their platform-formatted
text from `commands::shortcut()` instead of duplicating the binding. The
command test protects Alt+F3 for Previous and F3 for Next, so the tooltip and
command cannot drift independently.

#### BUG-08 · Help > Documentation opens the old wiki

✅ **Resolved.** *Evidence: code; regression test.*

**Where:** `src/app/mod.rs` (`Action::Documentation`).

The menu opened `https://github.com/CadQuery/CQ-editor/wiki`, which
describes the PyQt editor (installation with conda, PyQt preferences).

**Resolution:** it opens `docs/README.md` in the repository
(`DOCUMENTATION_URL`, built from `CARGO_PKG_REPOSITORY`), checked by
`the_documentation_link_points_to_these_docs`. The wiki itself is outside
the repository: a maintainer still needs to add a note there pointing to
these docs.

#### BUG-09 · Imported modules are watched only once

✅ **Resolved.** *Evidence: code.*

**Where:** `src/app/watching.rs` (`sync_watcher`), `src/app/worker_events.rs`
(handling of `Message::Imports`).

With *Autoreload: watch imported modules*, the list of local imports was
computed only when autoreload, that option or the file path changed. A
module imported later was not watched until one of those changed. The
merged list was also de-duplicated with `Vec::dedup` without sorting, so a
path could be watched twice, and modules no longer imported stayed watched.

**Resolution:** `request_imports` asks the worker again after every save and
every successful run, besides when watching starts. The answer is tagged
with the script it is for (`Pending::Imports(path)`) and ignored if the
document or the watch settings changed meanwhile. The watched set is then
replaced by the script plus its current imports, collected in a `BTreeSet`.

#### BUG-10 · Variables: values are unreadable

✅ **Resolved.** *Evidence: seen; regression test.*

**Where:** `src/panels/variables.rs`.

The *Value* column got only the space left after the fixed 120-point *Name*
and 90-point *Type* columns, so in the default layout every value showed as
`…`.

**Resolution:** the name and type columns take a share of the panel width
(`column_widths`), the value column keeps at least 80 points, and all
columns stay resizable (under a new table id, so the widths remembered from
the old layout do not apply). Clicking a row opens a resizable detail view
below the table with the whole value, wrapped, selectable and with a copy
button; × closes it, and a context menu copies a row's value or name. The
worker now sends up to 2000 characters per value instead of 300. In a short
pane the table and the detail view both scroll instead of growing over each
other, and the cell labels are not selectable, so a click on their text
selects the row (see BUG-28). Tests: `the_value_column_keeps_room_in_narrow_panes`,
`clicking_the_text_of_a_row_selects_it` and
`the_rows_stay_above_the_detail_view_in_a_short_pane`. Checked in the app with
the default layout.

#### BUG-11 · Misleading hints after a failed run

✅ **Resolved.** *Evidence: seen; regression test.*

**Where:** `src/app/mod.rs` (`empty_view_hint`), `src/panels/mod.rs`
(`empty_state`).

* After a failed run with nothing on screen, the view said *Press F5 to
  render the script*, although F5 had just been pressed. The view and the
  Objects panel now say *The last run failed at line N; see the Traceback
  panel*.
* Empty-state texts were laid out justified, so wrapped lines got uneven
  word spacing (*Select  a  workplane  in  Objects…*). They are now laid out
  as centered rows of natural width;
  `empty_state_rows_are_centered_with_natural_spacing` compares them with
  plain and justified layouts. Both checked in the app.

#### BUG-12 · The view keeps the previous file's objects

✅ **Resolved.** *Evidence: seen.*

**Where:** `src/app/documents.rs` (`set_document`).

Opening another file reset the editor and the traceback, but the 3D view,
Objects panel, selection and CQ Inspector still showed the previous file's
model until the next successful run. With a failing new script, the stale
model looked like the result of that script.

**Resolution:** New, Open, Open Recent, examples and a restored session now
clear the scene (releasing its worker objects), measurements, the inspector
and the variables. A run still going for the previous file is marked stale:
its result is discarded and its objects released, and a paused or later
pausing debugging session of that file is stopped. Restoring a local-history
version of the *same* file keeps the model, like a reload. Rendering on open
was not added: a script may be slow or have side effects, and F5 is one key
away. Checked in the app: opening an example after rendering another file
leaves an empty view, Objects tree and Variables panel.

#### BUG-13 · `debug()` in the console loses the variable name

✅ **Resolved.** *Evidence: code; regression test.*

**Where:** `worker/cq_worker/console.py`.

`ShownObjects.debug` finds the variable name two frames up, which is right
for the script wrapper. The console bound the method directly, so the lookup
landed in the interpreter's internals and the object got a generic name
(*Workplane 1*) instead of its variable name.

**Resolution:** the console wraps it (`ConsoleSession._debug`) the way it
wraps `show_object`, and no longer rebinds it after each command.
`test_console_names_shown_and_debugged_objects_after_their_variables` fails
with the former binding.

### P3: maintenance and polish

| Status | Id | Problem | Where | Resolution |
| :---: | --- | --- | --- | --- |
| ✅ | BUG-14 *(code)* | `--help` and command-line errors printed nothing in Windows release builds, which have no console | `src/main.rs` | `show_cli_message` writes to redirected output as before; otherwise it attaches to the console of the parent shell (`AttachConsole`) and writes to `CONOUT$`, or shows a message box when started without one. Checked with the release executable in a `cmd` console (usage and the error of an unknown option are printed) and started without a console (message box) |
| ✅ | BUG-15 *(code)* | Every worker version extracted a new `worker-<hash>/` folder in the data directory; old ones were never removed | `src/embedded.rs` | After a successful extraction, the folders of other versions (and interrupted extractions) are removed. A running older instance loses nothing: its worker has imported all modules, and a restart extracts them again. Test: `extracting_a_new_worker_removes_the_other_versions` |
| ✅ | BUG-16 *(code)* | Each recovery checkpoint (every 5 s while modified) read and parsed **every** history file of **every** document. History of deleted or renamed files was never pruned, and all untitled buffers shared one bucket | `src/recovery.rs` | One folder per document (stable FNV-1a hash of the path); version file names carry their timestamps, so a checkpoint lists one folder of at most 20 files and parses no JSON. Once per session, versions older than 90 days and the oldest beyond 64 MB are removed, and the flat history of earlier releases is migrated. Untitled buffers keep one shared folder on purpose: they have no identity across sessions, and their drafts stay findable. Tests in `tests/rust/editor/test_recovery.rs` |
| ✅ | BUG-17 *(code)* | Files that are not UTF-8 could not be opened at all | `src/document.rs`, `src/dialogs/mod.rs` | A *Not a UTF-8 file* dialog offers **Open as Windows-1252** (WHATWG mapping, never fails); the file is untouched until saved, saving writes UTF-8, and the status bar shows *Windows-1252 → UTF-8* meanwhile. The file watcher keeps reading the file in its encoding. Tests: `windows_1252_files_open_on_request_and_are_saved_as_utf8` and others |
| ✅ | BUG-18 *(code)* | The Windows `.exe` had no embedded icon or version information | `build.rs` | `winresource` compiles the icon and version strings with `rc.exe` (MSVC) or `windres` (GNU). Without a resource compiler the build warns; release CI sets `CQ_EDITOR_REQUIRE_RESOURCES=1` to make it an error. Checked with `windres` from MSYS2: Explorer's details show *CQ-editor 0.8.0* and the icon |
| ✅ | BUG-19 *(code)* | Building with the Windows GNU toolchain needed MinGW binutils and a hand-made `libshlwapi.a` | `Cargo.toml`, `src/app/mod.rs` | eframe is used without its `links` feature, which removes `webbrowser` (and `shlwapi`) from the dependency tree; links of egui widgets are taken from the platform output and opened with `open`. binutils (`dlltool`) are still required by the `windows` crates. Test: `links_clicked_in_widgets_are_taken_from_the_platform_output` |
| ✅ | BUG-20 *(code)* | The Log panel lived only in memory: after a GUI panic or crash, the messages that explained it were gone | `src/logging.rs` | Every entry is also appended to `logs/cq-editor.log` in the data folder (warnings and errors immediately, other lines within a second, by a flushing thread even when nothing else is logged), rotated at 2 MB to `cq-editor.1.log`. A panic hook records the panic and its backtrace without waiting for the log lock. Bug reports and Troubleshooting mention the file |

### Performance

| Status | Id | Problem | Where | Suggested fix |
| :---: | --- | --- | --- | --- |
| ✅ | PERF-01 | Resolved: per-face counts propose the display-triangle mapping, then OCCT triangulation bounds and sampled centroids validate every face group before it is picked directly; separate pick triangles remain the fallback for count or geometry mismatches. On the 324-hole benchmark the validated path takes 1.49 s total, including 65 ms for validation, versus 2.2 s for the former extraction, while retaining its 34% smaller message | `worker/cq_worker/tessellation.py` | Covered by independent face-centroid equivalence on ten shape kinds, explicit equal-count face-reordering fault injection, and the OCP 7.8/7.9/8.0 matrix |
| ✅ | PERF-02 | Resolved as far as OCCT allows: `RWGltf_CafWriter` (OCCT 7.8 through OCP) can only write files, so the glTF still passes through disk. Measured on the 324-hole plate, the file handling costs about 1 ms of the writer's 95 ms (and its parallel mode gains nothing), so the round trip now reuses one scratch folder per worker process. That also fixed a leak: a worker stopped while meshing left its `cq-editor-*.glb` files in the temporary folder, and the next worker now removes leftovers older than a day | `tessellation.py` (`_scratch_path`) | Tests: `test_glb_round_trips_leave_only_an_empty_scratch_directory`, `test_scratch_left_by_killed_workers_is_removed_once_stale` |
| ✅ | PERF-03 | Resolved: a retained scene-part BVH prefilters parts and per-mesh BVHs prune edges and vertices in screen space; hover picking already pauses while dragging or when the pointer is still | `src/viewer/picking.rs`, `src/spatial.rs` | Covered by linear-scan equivalence tests and the `pick_latency_on_growing_assemblies` benchmark |
| ✅ | PERF-04 | Resolved: the `measure` request resolves selected ids through `RegistryEntry.subshape` and calculates against live OCCT geometry | `worker/cq_worker/measurement.py`, `protocol.rs` | Covered by exact numeric worker and protocol tests |

### Found in the October 2026 review

A full review of the code for this roadmap, and a later architecture
review, found these problems, which were not listed before. All are
resolved, each with a regression test unless noted.

| Status | Id | Problem | Where | Resolution |
| :---: | --- | --- | --- | --- |
| ✅ | BUG-21 *(P1, code)* | Quitting with **Don't save** brought the discarded edits back: eframe's final `save` wrote a new recovery checkpoint, and the next start offered *Unsaved work can be recovered* | `src/app/documents.rs` (`checkpoint_recovery`) | No checkpoint once quitting was confirmed (`allow_close`) |
| ✅ | BUG-22 *(P2, code)* | **Stop debugging** raised `DebugStop`, an `Exception`, so a script's own `except Exception` swallowed it and the session went on | `worker/cq_worker/debugger.py` | `DebugStop` derives from `BaseException`, like `KeyboardInterrupt`. Test: `test_stop_is_not_swallowed_by_the_scripts_exception_handlers` |
| ✅ | BUG-23 *(P1, code)* | The editor could panic (and close) when an edit, such as a reload, left the completion popup's anchor inside a multi-byte character: `close_popups_if_left` sliced the text without checking character boundaries | `src/editor/mod.rs` | `typed_text` checks both boundaries. Test: `completion_survives_edits_that_move_its_anchor_inside_a_character` |
| ✅ | BUG-24 *(P2, code)* | A link in an upload service's JSON reply was opened with the system handler, which also opens local files and starts programs | `src/app/mod.rs` (`open_url`) | Only `http` and `https` URLs without control characters are opened. Test: `only_web_links_are_opened` |
| ✅ | BUG-25 *(P3, code)* | A long status message (typically an error) took the whole status bar and pushed the Python state, cursor position and encoding out of it | `src/ui/status_bar.rs` | The fixed indicators are laid out first; the message is truncated to what remains and shows its full text on hover. Checked in the app |
| ✅ | BUG-26 *(P3, code)* | The Log filter lowercased the text but not the source, so typing a source such as `OCP` matched nothing | `src/panels/log.rs` | Both are compared in lowercase. Test: `the_filter_ignores_case_in_sources_too` |
| ✅ | BUG-27 *(P3, code)* | When a run's scene could not be decoded, `finish_run` returned early and also dropped the run's variables and a queued render; a run request that failed in the worker left the debugger arrow on screen and dropped a queued render | `src/app/runs.rs` (`finish_run`, `run_failed`) | The scene error is reported and the rest of the result still applies; a failed run clears the debug line and starts the queued render. Test: `a_failed_run_still_starts_the_render_queued_behind_it` (the run state machine); the scene decoding path still needs a running eframe context |
| ✅ | BUG-28 *(P3, seen)* | Clicking the text of a Traceback row (the file name, line or code) did nothing: egui labels are selectable by default and took the click from the row, so only the empty part of a cell jumped to the line | `src/panels/traceback.rs` | Cell labels are not selectable; the Variables table has the same fix and test. Checked in the app: clicking the file name of a frame moves the cursor to its line |
| ✅ | BUG-29 *(P1, code)* | Worker dependency versions were unbounded and the current CadQuery development branch could not import because OCP 8 moved generated collection classes and removed `TopoDS.*_s` downcasts | `worker/pyproject.toml`, `tessellation.py`, `.github/workflows/ci.yml` | Explicit supported ranges, OCP 7/8 name adapters, blocking released-version lanes and an upstream-development early-warning lane. `test_compatibility.py` performs a real topology-aware tessellation in every environment |
| ✅ | BUG-30 *(P1, code)* | A script writing forever without a newline grew `OutputStream._pending` indefinitely, and Rust's JSON/stderr `lines()` readers had no maximum line size | `worker/cq_worker/protocol.py`, `src/worker/mod.rs` | Output flushes every 64 Ki characters and is truncated after 1 MiB per stream/request; logging records stop at 256 Ki characters each and 1 MiB per request. A bounded reader retains at most 2 MiB for control, 512 MiB for mesh-bearing replies and 256 KiB for native stderr. Regression tests inject newline-free output and oversized lines |
| ✅ | BUG-31 *(P2, code)* | A mesh id hashed only the shape's BREP and the tolerances, but the GUI keeps meshes across Python restarts: after switching to another environment, or upgrading OCP, the new worker could answer with a reference to a mesh made by the previous mesher. The worker also reported OCP as `unknown` for conda installs, which carry no pip metadata | `worker/cq_worker/tessellation.py` (`GeometryCache`), `environment.py` | Ids also hash the mesh generator: `MESH_FORMAT`, the OCP version and the installed OCP binary (`mesh_generator`); the OCP version falls back to the bindings' own `__version__`. Tests: `test_geometry_ids_name_the_mesher_that_made_them`, `test_ocp_version_is_known_without_pip_metadata` |
| ✅ | BUG-32 *(P3, code)* | The CQ inspector drew the construction geometry of a reply that arrived after another workplane, or another step, was selected | `src/app/worker_events.rs` (`Message::InspectionGeometry`) | The pending request remembers its workplane and entry, and replies for anything but the current selection are ignored. Covered by `every_reply_is_routed_once` for the routing; the overlay itself needs a running eframe context |

---

## 2. Gaps in the everyday experience

Small, concrete improvements to what already exists:

| Status | Improvement |
| :---: | --- |
| ✅ | **Undo/Redo in the Edit menu.** Done: Edit > Undo/Redo with Ctrl+Z, Ctrl+Y and Ctrl+Shift+Z. The editor keeps its own history (`src/editor/history.rs`): `TextEdit`'s grouped by time, counted caret moves and outlived the document, so undo after opening another file brought back the previous file's text. Typing is grouped while it continues at the caret; indent, comment, completion, *Replace all* and a reload are one step each. Tests: `indent_comment_and_completion_are_undone_as_one_step_each`, `undo_never_brings_back_another_document`, `tests/rust/editor/test_history.rs` |
| ✅ | **Completion while typing names.** Done: after 3 characters of a name by default (*Preferences > Editor > Complete names after*, 0 turns it off); these automatic requests never put a message in the status bar. Test: `completion_is_offered_once_a_name_reaches_the_threshold` |
| ✅ | **Go to line** (Ctrl+G, `line` or `line:column`), **Replace** (Ctrl+H; *Replace* and *Replace all*, one undo step) and **whole word** search. Done; the first Enter or F3 after typing a query also skipped the first match, and now goes to the first match after the caret. Tests: `whole_word_search_skips_parts_of_longer_names`, `the_first_search_step_goes_to_the_first_match_after_the_caret`, `replace_takes_the_selected_match_and_replace_all_is_one_step`, `go_to_line_accepts_a_line_and_an_optional_column` |
| ✅ | **Hover documentation.** Done: resting the pointer on a name shows its signatures and docstring, or the type of a variable (`describe` request, jedi `help`/`infer`). Tests: `test_documentation_gives_signatures_and_docstrings`, `test_description_of_the_name_under_the_pointer`, `documentation_is_shown_only_for_the_name_still_under_the_pointer` |
| ✅ | **Remember breakpoints** per file across sessions. Done for the last 100 files; each breakpoint keeps the text of its line and returns to it if the file changed meanwhile. Tests: `breakpoints_are_found_again_by_their_line_text`, `breakpoints_are_relocated_by_the_text_of_their_line`, `breakpoints_are_remembered_per_file_and_bounded` |
| ✅ | **Object property sheet.** Done: a selected face, edge or vertex shows its geometry type, area or length, radius and center (position for a vertex) from the exact measurement. Test: `the_property_sheet_finds_the_measurement_of_its_selection` |
| ✅ | **Better first-run experience.** Done: the dialog tells *Python was not found* (including the Windows Store placeholder) from *CadQuery is not available*, lists the interpreters found on this computer, and offers **Choose interpreter…** and **Installation guide** (also in the Help menu). Tests: `a_failed_start_says_whether_python_or_cadquery_is_missing`, `conda_environments_are_listed_after_the_base_interpreter` |

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
| ✅ | FEAT-B1 ⚪ | **Selection to code**: after picking faces or edges, *Copy selector* / *Insert selector* generates a CadQuery expression such as `.faces(">Z")` or `.edges("\|Z and >Y")`, with an index-based fallback when no simple selector matches | Done: direction, type, rank and inclined-direction selectors combined with `and`/`or`/`exc`, simplest first; every candidate is verified with CadQuery on the script's own object (assembly parts in their own coordinates). Ctrl+click gathers up to 64 entities. Tests: `test_every_entity_gets_a_selector_that_selects_exactly_it`, `test_simple_selectors_are_evaluated_exactly_as_cadquery_does` |
| ✅ | FEAT-B2 🔵 | **Picked entities in the console**: `selected()` returns the selected `cq.Face` / `cq.Edge` objects for measurement and experiments | Done: every console command carries the GUI's selection, resolved when `selected()` runs: picked entities in pick order and placed as shown, else the selected objects' shapes; stale picks raise a `LookupError` that says why. Tests: `test_console_selected_returns_the_picked_entities_and_objects`, `test_picked_entities_resolve_to_live_shapes_placed_as_shown` |
| ✅ | FEAT-B3 ⚪ | **Parameter panel**: top-level numeric assignments (`width = 40`) appear as spin boxes and sliders. Changing one rewrites that line and re-renders | Done: only plain top-level assignments of a number literal (skipping blocks, brackets, strings and continuations); literals keep their style, `# [min, max, step]` sets the slider; one Undo step per drag and a render once the value settles. Tests: `nested_continued_and_non_literal_lines_are_skipped`, `a_parameter_change_rewrites_its_literal_and_renders_once_it_settles` |
| ✅ | FEAT-B4 ⚪ | **Jump to source**: from an object or a CQ Inspector step to the line that created it | Done: while a script runs, new Workplanes, Shapes, Assemblies and Sketches record their innermost script line (about 1% of run time); the GUI keeps the run's text and maps the line onto later edits. Tests: `test_each_object_remembers_the_line_that_created_it`, `going_to_the_source_follows_the_edits_made_since_the_run` |
| | FEAT-B5 ⚪ | **Measure tool**: exact distance, closest points, angle, area, length, radius/diameter and geometry type are implemented from picked entities; bounding box and *copy as code* remain | Reads the live OCCT model; never edits it |

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
| | FEAT-C8 🔵 | **Stereo compatibility**: dual-camera rendering and tested composition for selected legacy modes, with a clear fallback when the GPU/platform cannot provide them |

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
| ✅ | FEAT-E4 🟢 | **Self-contained installers:** Windows `.exe`, macOS `.pkg` for Apple Silicon and Intel, and Linux `.sh` bundle the pinned Python/CadQuery runtime. CI installs and smoke-tests each artifact; native launchers are created on all systems and Windows/Linux register "Open with CQ-editor" for `.py` files |
| | FEAT-E5 🔵 | **Self-update check** for CQ-editor (today only CadQuery releases are checked) |
| | FEAT-E6 🔵 | **Rich console output**: transport bounded and sanitized Jupyter MIME bundles from the isolated worker without allowing user code to block the GUI |

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
| | ENG-04 | Benchmarks for tessellation, decoding and picking on large assemblies, tracked over time (picking has `pick_latency_on_growing_assemblies`; not yet tracked over time) |
| | ENG-05 | Accessibility review (keyboard-only use, screen readers through AccessKit, contrast of both themes) |
| ✅ | ENG-06 | `src/app.rs` had grown to about 2,000 lines of state, event routing and transitions. It is now `src/app/`, one module per concern around the central `Action` dispatch, and runs with their pending requests are a pure state machine (`Execution`) whose transitions are tested in `tests/rust/app/test_execution.rs`. Next: move the remaining egui-bound glue behind similar testable controllers as it changes |

---

## 4. Milestones

| Milestone | Goal | Contents |
| --- | --- | --- |
| **0.8: stable core rewrite** | Preserve the modeling workflow without hiding incompatibilities | BUG-01…BUG-27 ✅, ENG-01, compatibility matrix, docs linked from Help ✅ |
| **0.9: code ↔ model** | The GUI helps write code | FEAT-A3, A4, B1, B2, B3, B5; completion while typing names, Go to line, Replace, hover docs; PERF-01…04 |
| **1.0: projects** | Comfortable for large designs | FEAT-D1, D2, D3, C1, C2, E1, E3, E4, F1 |
| **Later** | Exploratory | FEAT-A2, A6, B4, C3–C8, D4–D6, E2, E5, E6, F2, F3, ENG-02…05 |

The order inside a milestone is flexible. Bugs come before features.
