# Compatibility with the original CQ-editor

This document is the compatibility contract for the Rust rewrite. It prevents
"implemented" from being used as a synonym for "equivalent" and makes every
known removal or changed behavior explicit.

## Baseline and rules

The baseline is commit [`71903d5`](https://github.com/igors93/CQ-editor/tree/71903d5),
the last PyQt/OCCT revision immediately before the Rust rewrite. The primary
legacy surfaces are
[`viewer.py`](https://github.com/igors93/CQ-editor/blob/71903d5/cq_editor/widgets/viewer.py),
[`occt_widget.py`](https://github.com/igors93/CQ-editor/blob/71903d5/cq_editor/widgets/occt_widget.py),
[`editor.py`](https://github.com/igors93/CQ-editor/blob/71903d5/cq_editor/widgets/editor.py)
and
[`main_window.py`](https://github.com/igors93/CQ-editor/blob/71903d5/cq_editor/main_window.py).
The eight scripts `01` to `08` in `examples/` are byte-for-byte unchanged from
that commit and are executable compatibility fixtures; later examples are
additions.

Compatibility means the same user-visible result or script contract. It does
not require the same toolkit, process model or rendering implementation. A
legacy defect is not preserved merely for fidelity; a corrected behavior is
marked **Enhanced** and protected by a regression test.

The states below have precise meanings:

* **Verified**: an automated test exercises the compatible result.
* **Enhanced**: the original result remains available and a tested superset or
  deliberate defect correction is provided.
* **Changed**: a deliberate, documented product decision replaces the old
  behavior.
* **Gap**: the original capability is not currently delivered.
* **Unverified**: it is implemented, but no automated test proves the complete
  user interaction yet. It must not be described as equivalent.

## Compatibility matrix

The first column is a stable contract id. `compatibility_matrix_is_complete_and_evidenced`
checks that no row disappears, every Verified/Enhanced row points to a real
test, and every Changed/Gap/Unverified row points to an explicit decision or
follow-up. A passing matrix test therefore proves the inventory is maintained;
it does not turn the three open gaps into supported features.

| Contract id | Original capability and current result | State | Evidence or decision |
| --- | --- | --- | --- |
| `file.lifecycle` | New, Open, Save and Save As preserve the editable Python file workflow. Saves are now atomic. | Enhanced | `tests/rust/editor/test_document.rs::preserves_line_endings_and_tracks_modification` |
| `file.autoreload` | Automatic reload and imported-file watching remain; local edits now require an explicit conflict decision. | Enhanced | `tests/rust/editor/test_document.rs::reload_refuses_to_discard_unsaved_changes` |
| `file.recovery` | Unsaved buffers gain crash recovery and local history. | Enhanced | `tests/rust/editor/test_recovery.rs::checkpoint_restores_unsaved_work_after_a_simulated_crash` |
| `editor.syntax` | Python syntax highlighting and the injected CQ-editor names remain recognized. | Verified | `tests/rust/editor/test_highlight.rs::recognizes_python_constructs` |
| `editor.editing` | Line numbers, breakpoints, indentation and Toggle Comment remain; markers now follow edits. | Enhanced | `tests/rust/editor/test_text_ops.rs::comments_blocks_at_the_common_indentation` |
| `editor.search` | Find and repeated match navigation remain available. | Verified | `tests/rust/editor/test_editor.rs::finds_matches_with_and_without_case` |
| `editor.completion` | Jedi completion and call signatures remain optional and use the live namespace. | Verified | `tests/worker/execution/test_completion.py::test_completion_uses_live_namespace_values` |
| `script.examples` | All eight projects shipped before the rewrite still execute and produce a visible model. | Verified | `tests/worker/execution/test_examples.py::test_legacy_example_executes_and_produces_a_model` |
| `script.show-object` | `show_object(obj, name, options, **options)` and variable-derived names remain supported. | Verified | `tests/worker/execution/test_execution.py::test_show_object_names_from_variables` |
| `script.discovery` | CadQuery values are still displayed when a script omits `show_object`. | Verified | `tests/worker/geometry/test_scene.py::test_find_cq_objects_ignores_private_and_plain_values` |
| `script.logging` | `print` output and `log()` messages still reach their panels. | Verified | `tests/worker/protocol/test_server.py::test_script_log_reaches_the_gui` |
| `script.execution-settings` | Reload CadQuery, script directory on `sys.path`, script working directory and imported-module reload remain configurable. | Verified | `tests/worker/execution/test_environment.py::test_script_environment_respects_disabled_path_and_cwd_changes` |
| `debugger.controls` | Debug, Step, Step in and Continue remain; Step out and Stop were added. | Enhanced | `tests/worker/execution/test_debugger.py::test_step_in_and_out` |
| `debugger.breakpoints` | Gutter breakpoints remain and now follow inserted or deleted lines. | Enhanced | `tests/rust/editor/test_editor.rs::breakpoints_and_the_error_line_follow_inserted_and_deleted_lines` |
| `debugger.errors` | Tracebacks retain script lines and nested call frames; variables remain described separately. | Verified | `tests/worker/execution/test_execution.py::test_errors_inside_script_functions_keep_the_call_chain` |
| `console.state` | The Python console remains interactive, multiline and persistent across commands. | Verified | `tests/worker/execution/test_console.py::test_console_keeps_variables_between_commands` |
| `console.rich-output` | The old in-process IPython/Jupyter rich display is not reproduced by the text console. | Gap | Decision: [Rich console](#rich-console) |
| `inspector.workplane` | The CQ Inspector still traverses Workplane parents and can display a stack item, plane and grid. | Verified | `tests/worker/protocol/test_server.py::test_inspection` |
| `objects.tree` | Objects and assembly children retain visibility, selection, removal and clearing semantics. | Verified | `tests/rust/scene/test_scene.rs::hiding_a_parent_hides_the_subtree` |
| `objects.properties` | Visibility and colors can be preserved by name; color and opacity editing are a tested superset. | Enhanced | `tests/rust/scene/test_scene.rs::properties_survive_a_reload_by_name` |
| `objects.assemblies` | Assemblies can remain hierarchical or be merged, retaining per-part transforms and colors. | Verified | `tests/worker/geometry/test_scene.py::test_builder_explodes_assemblies` |
| `export.step-stl` | Selected objects still export to STEP and STL; BREP was added. | Enhanced | `tests/worker/protocol/test_server.py::test_export` |
| `tools.upload` | Python or exported model bytes can still be uploaded to the configured service. | Verified | `tests/worker/protocol/test_server_commands.py::test_step_upload_sends_actual_model_bytes` |
| `tools.update` | CadQuery release checks remain available. | Verified | `tests/worker/protocol/test_server_commands.py::test_update_check_returns_current_and_newer_versions` |
| `viewer.standard-views` | Fit, isometric, top, bottom, front, back, left and right camera results remain. | Verified | `tests/rust/viewer/test_camera.rs::standard_views_look_where_expected` |
| `viewer.display-modes` | Shaded and wireframe commands are implemented, but their final GPU images are not compared automatically. | Unverified | Follow-up: [GUI-only validation](#gui-only-validation) |
| `viewer.projection` | Orthographic and perspective cameras remain and produce consistent pick rays. | Verified | `tests/rust/viewer/test_camera.rs::projection_round_trips_through_rays` |
| `viewer.mouse` | Left orbit/select, middle or Ctrl+left pan, right or Shift+left zoom, and wheel zoom retain their bindings. | Verified | `tests/rust/viewer/test_navigation.rs::legacy_mouse_gestures_keep_their_navigation_modes` |
| `viewer.navigation-cube` | Face/edge/corner views and 45-degree roll remain, with native hover feedback and animation. | Enhanced | `tests/rust/viewer/test_overlays.rs::arrows_are_hit_on_both_sides` |
| `viewer.selection` | Object selection remains and now resolves stable OCCT face, edge and vertex identities. | Enhanced | `tests/rust/viewer/test_picking.rs::prioritizes_vertices_then_edges_then_faces` |
| `viewer.screenshot` | PNG screenshots are implemented, but capture and save-dialog integration are not automated. | Unverified | Follow-up: [GUI-only validation](#gui-only-validation) |
| `viewer.stereo` | Stereo, mono-eye and OCCT stereo-composition modes are not implemented by the wgpu renderer. | Gap | Decision: [Stereo projection](#stereo-projection) |
| `panels.docking` | All legacy panels are dockable and resizable, but full drag/detach/layout equivalence has no automated UI test. | Unverified | Follow-up: [GUI-only validation](#gui-only-validation) |
| `preferences.persistence` | Supported preferences and the dock layout persist across sessions. | Verified | `tests/rust/app/test_settings.rs::preferences_round_trip_through_json` |
| `preferences.defaults` | Configurable font and tessellation settings remain, but three numeric defaults differ from the PyQt values. | Changed | Decision: [Preference defaults](#preference-defaults) |
| `shortcuts.legacy` | Every legacy file, editor, debugger and view shortcut is retained; additions do not replace them. | Verified | `tests/rust/app/test_commands.rs::legacy_shortcuts_are_preserved` |
| `behavior.rand-color` | `rand_color()` now defaults to visible opacity instead of reproducing the original invisible-color defect. | Enhanced | `tests/worker/execution/test_execution.py::test_rand_color_is_reproducible` |
| `ui.theme` | Separate application/editor schemes became one System/Light/Dark application theme. | Changed | Decision: [Theme settings](#theme-settings) |
| `packaging.distribution` | Release jobs build native executables, but do not yet bundle Python, CadQuery and installers like the PyInstaller/conda distribution did. | Gap | Decision: [Distribution](#distribution) |
| `worker.isolation` | Script execution moved out of process; crashes no longer take down the editor and stale scene ids cannot collide after restart. | Enhanced | `tests/worker/protocol/test_worker_process.py::test_worker_can_be_restarted_after_a_native_process_exit` |

## Exact interaction inventory

### Legacy shortcuts

The compatibility test asserts the complete set below. Ctrl means the
platform command modifier where appropriate.

| Area | Action | Original | Current |
| --- | --- | --- | --- |
| File | New / Open / Save / Save As / Quit | Ctrl+N / Ctrl+O / Ctrl+S / Ctrl+Shift+S / Ctrl+Q | Preserved |
| Edit | Toggle Comment / Auto-Complete / Search | Ctrl+/ / Alt+/ / Ctrl+F | Preserved; Ctrl+Space also completes |
| Run | Render / Debug / Step / Step in / Continue | F5 / Ctrl+F5 / Ctrl+F10 / Ctrl+F11 / Ctrl+F12 | Preserved; Step out and Stop added |
| View | Fit / Iso / Top / Bottom / Front / Back / Left / Right | Shift+F1 through Shift+F8 | Preserved |
| View | Wireframe / Shaded | Shift+F9 / Shift+F10 | Preserved |

### Legacy mouse behavior

| Gesture | Original result | Current result |
| --- | --- | --- |
| Left drag | Orbit using Turntable or Trackball | Preserved |
| Left click without a drag | Select | Preserved; exact topology is also returned |
| Middle drag or Ctrl+left drag | Pan | Preserved |
| Right drag or Shift+left drag | Zoom | Preserved |
| Wheel | Zoom | Preserved; anchored at the pointer |
| Navigation cube or roll arrow click | Reorient or roll by 45 degrees | Preserved |

### Legacy preferences

| Original group | Original setting | Current mapping |
| --- | --- | --- |
| General | Light/Dark Theme | System/Light/Dark application theme; see the decision below |
| Viewer | Fit automatically, navigation cube, gradient and two background colors | Preserved |
| Viewer | Deviation and angular deviation | Preserved as tessellation tolerances; the defaults changed as documented below |
| Viewer | Projection Type | Orthographic and Perspective preserved; stereo variants are an open gap |
| Viewer | Stereo Mode | Not applicable until stereo composition exists |
| Viewer | Orbit Method | Turntable and Trackball preserved |
| Editor | Font size, autoreload, reload delay, imported-file watch, line wrap and maximum line length | Preserved; the font-size default changed as documented below |
| Editor | Color scheme | Merged into the application theme |
| Debugger | Reload CQ, add script directory, change working directory and reload imported modules | Preserved as Execution settings |
| Objects | Preserve properties, clear before run, merge assemblies and STL precision | Preserved |

Default shape/edge colors, edge visibility/width, axes, animation and STL
angular tolerance are additions rather than legacy settings.

## Known differences and decisions

### Stereo projection

The legacy modes were thin bindings to OCCT's `Graphic3d_Camera` and
`Graphic3d_StereoMode`. The Rust viewer is a custom wgpu renderer, so adding
the old preference labels without two camera passes and a real composition
pipeline would create false compatibility. The modes remain an explicit gap
(`FEAT-C8`) until each supported mode has a render test and platform fallback.
Orthographic and perspective must not regress while that work is pending.

### Rich console

The old console embedded an in-process Jupyter kernel and could render rich
IPython output. The isolated worker intentionally provides a persistent text
console so user code cannot freeze the GUI. Restoring rich display requires a
serialized MIME-bundle protocol, output limits and untrusted HTML handling;
it is tracked as `FEAT-E6`. Plain Python evaluation, multiline input,
completion, history, `show_object`, `debug`, stdout and errors remain covered.

### Theme settings

The editor and application no longer have independent Light/Dark choices.
One System/Light/Dark theme prevents unreadable mixed combinations and follows
the operating system by default. This is an intentional settings migration,
not a missing editor feature.

### Preference defaults

The PyQt baseline used font size `12`, deviation `1e-5` and angular deviation
`0.1`. The current defaults are `13`, `1e-3` and `0.2`. Font units and raster
metrics differ between Qt and egui, so equal numbers do not imply equal visual
size. Tessellation now occurs in a separate process and its mesh crosses the
worker protocol; the looser defaults bound routine render cost. The legacy
values remain configurable, and worker tests verify that a finer tolerance
produces a finer mesh and that large structural dimensions retain small
cross-sections. These defaults must not be described as identical.

### Distribution

The old PyInstaller and conda-constructor paths bundled the Python runtime and
dependencies. Current release artifacts are standalone Rust executables that
locate a user-managed Python with CadQuery. Bare executables must not be called
equivalent installers. Bundled, signed installers remain `FEAT-E4`; until they
exist, installation documentation must state the external Python requirement.

### GUI-only validation

Screenshot capture, final shaded/wireframe pixels and free-form dock dragging
cross native window/GPU boundaries. They remain **Unverified**, not presumed
equivalent. `ENG-03` tracks a headless UI harness; render-image tests should use
a deterministic software adapter or golden images with a documented tolerance.

## Change policy

Any change to a legacy capability must do one of the following in the same
commit:

1. preserve or enhance it and add/update automated evidence;
2. mark it Changed with the product decision and migration behavior; or
3. mark it as a Gap with a tracked roadmap item.

A feature may move to Verified only when its named test exercises the stated
result. Passing unrelated unit tests or merely retaining a menu label is not
equivalence evidence.
