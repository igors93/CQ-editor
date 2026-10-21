# Changelog

## Unreleased (0.8.0)

The interface was rebuilt in Rust (egui + wgpu); CadQuery now runs in a
separate Python worker process (`worker/cq_worker`). Scripts, `show_object`
options, menus, shortcuts and panels work as before.

* New: a running or crashing script no longer freezes or closes the editor;
  Run > Stop (Ctrl+Shift+F5) aborts it and a crashed worker restarts itself
* New: GPU renderer with MSAA, smooth camera animations, zoom at the mouse
  pointer, hover highlighting and double-click to zoom to an object
* New: precise OCCT face, edge and vertex selection with subshape highlighting
  and stable topology identifiers retained by the Python worker
* New: navigation cube redrawn natively, with hover feedback and the 45° roll
  arrows; view icons follow the theme
* New: debugger "Step out" (Ctrl+Shift+F11) and a cleaner stepping model
* New: editor auto-indentation, current-line highlight, match-case search,
  completion popup filtered while typing and call signature tips
* New: object colors and opacity can be changed in the Objects panel, which
  also shows sizes and triangle counts
* New: Open Recent, Reset Layout, Restart Python, BREP export; single
  assemblies export to STEP with their names and colors
* New: Log panel with level filters; console with history
* New: atomic saves that preserve line endings, the UTF-8 BOM, permissions
  and file metadata; files that are not valid UTF-8 are refused instead of
  being rewritten
* New: crash recovery (checkpoints every 5 seconds while a buffer is
  modified, restore on the next start) and File > Local History with the
  newest 20 versions per document
* New: automatic reload never discards unsaved edits; a dialog offers to
  keep them, reload, save a copy or compare
* New: content-addressed mesh cache and assembly instancing; unchanged
  meshes are not sent or uploaded again
* New: documentation in `docs/` (user guide, development guide, roadmap)
* Changed: theme follows the system by default; the separate editor color
  scheme preference was merged into the application theme
* Changed: stereo projection modes were dropped (orthographic and
  perspective remain)
* Fixed: `rand_color()` defaulted to a fully transparent color
* Fixed: after Stop, Restart Python or a worker crash, the first run could
  free its own objects, so the CQ Inspector, export and upload failed; scene
  keys are now unique per worker process
* Fixed: the CI format check (`black`) failed on three worker files
* Fixed: in a short Objects pane, the property sheet grew over the object
  tree; it now scrolls and leaves the tree its room
* Removed: the PyQt GUI, PyInstaller and conda constructor packaging

## Release 0.5.0

* Implemented a dark theme and tweaked it to work better with icons (#490)
* Fixed bug causing the cursor to jump when autoreload was enabled (#496)
* Added a max line length indicator (#495)
* Mentioned work-around for Linux-Wayland users in the readme (#497)
* Fixed bug where dragging over an object while rotating would select the object (#498)
* Changed alpha setting in `show_object` to be consistent with other CadQuery alpha settings (#499)
* Added ability to clear the Log View and Console (#500)
* Started preserving undo history across saves (#501)
* Updated run.sh script to find the real path to the executable (#505)

## Release 0.4.0

* Updated version pins in order to fix some issues, including segfaults on Python 3.12
* Changed to forcing UTF-8 when saving (#480)
* Added `Toggle Comment` Edit menu item with a `Ctrl+/` hotkey (#481)
* Fixed the case where long exceptions would force the window to expand (#481)
* Add stdout redirect so that print statements could be used and passed to log viewer (#481)
* Improvements in stdout redirect method (#483 and #485)
* Fixed preferences drop downs not populating (#484)
* Fixed double-render calls on saves in some cases (#486)
* Added a lint check to CI and linted codebase (#487)
