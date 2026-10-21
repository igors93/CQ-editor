# CQ-editor documentation

CQ-editor is a code editor and 3D viewer for [CadQuery](https://github.com/CadQuery/cadquery).
You write ordinary Python, press **F5**, and the model appears. The editor
adds the tools you need around that loop: a debugger, a stack inspector, an
interactive console and exporters.

![CQ-editor main window](images/main-window-light.png)

## Getting started

| Page | What you will learn |
| --- | --- |
| [Installation](getting-started/installation.md) | Install CadQuery, build or download CQ-editor, point it at the right Python |
| [Your first model](getting-started/first-model.md) | A 10-minute tour: model a bracket, change parameters, select faces, debug and export |

## User guide

| Page | Topics |
| --- | --- |
| [The interface](user-guide/interface.md) | Window layout, panels, menus, toolbar, status bar, themes |
| [Writing scripts](user-guide/scripting.md) | `show_object`, colors, assemblies, `debug`, `log`, the script environment |
| [3D view](user-guide/viewer.md) | Mouse controls, navigation cube, standard views, selecting faces, edges and vertices |
| [Code editor](user-guide/editor.md) | Editing, completion, search, files, automatic reload, crash recovery, local history |
| [Debugging and inspection](user-guide/debugging.md) | Breakpoints, stepping, the CQ Inspector |
| [Console, variables and logs](user-guide/console.md) | Interactive Python, Variables, Traceback and Log panels |
| [Exporting and sharing](user-guide/exporting.md) | STEP, STL, BREP, screenshots, model upload |
| [Preferences](user-guide/preferences.md) | Every option with its default value |
| [Keyboard and mouse reference](user-guide/shortcuts.md) | All shortcuts on one page |
| [Troubleshooting](user-guide/troubleshooting.md) | Python not found, slow models, display problems, where files are stored |

## Development

| Page | Topics |
| --- | --- |
| [Architecture](development/architecture.md) | Rust interface, Python worker, frame loop, renderer, geometry cache |
| [Worker protocol](development/protocol.md) | The JSON-lines protocol (version 3) between the two processes |
| [Building](development/building.md) | Toolchains, platform notes, release builds |
| [Testing](development/testing.md) | Test layout, running the suites, continuous integration |
| [Original compatibility](development/compatibility.md) | Verified legacy behavior, exact shortcuts and mouse controls, deliberate differences and open gaps |
| [Contributing](development/contributing.md) | Workflow, code style, where to make common changes |

## Project

* [Roadmap](roadmap.md): known bugs, planned features and the direction of the project
* [Changelog](changelog.md): what changed in each release

> These pages describe the current source tree (version `0.8.0-dev`). If
> something here does not match what the program does, please
> [open an issue](https://github.com/CadQuery/CQ-editor/issues/new).
