# Writing scripts

A CQ-editor script is a normal CadQuery Python file. It needs no special
imports and no special structure. The same file runs with plain `python`, in
CQ-editor, or in other CadQuery tools that support `show_object`. CQ-editor
adds a few names to the script's namespace so you can display results.

## The script API

| Name | Purpose |
| --- | --- |
| `show_object(obj, name=None, options=None, **options)` | Display `obj` in the 3D view and the Objects panel |
| `debug(obj, name=None)` | Display `obj` as a translucent red overlay, handy for checking a selection or a construction shape |
| `log(value)` | Write `str(value)` to the Log panel |
| `rand_color(alpha=1.0, cfloat=False)` | A random, not-too-bright color for `show_object` options |
| `cq` | The `cadquery` module, even if the script did not import it |

The interactive console offers the same names, plus `show` as a short alias
of `show_object`.

## `show_object`

```python
show_object(result)                                   # name taken from the variable: "result"
show_object(result, name="housing")                   # explicit name
show_object(result, options={"color": "steelblue"})   # options as a dict…
show_object(result, color="steelblue", alpha=0.5)     # …or as keyword arguments
```

**Naming.** When `name` is omitted, the editor uses the variable that holds
the object in the calling code. If there is none (for example
`show_object(box.faces(">Z"))`), it uses the type and a counter, such as
*Workplane 1*. Showing a second object with the same name replaces the first
one.

**What can be shown:**

* `cq.Workplane`: every shape on its stack (sketches included);
* any `cq.Shape` (`Solid`, `Compound`, `Face`, `Wire`, `Edge`, `Vertex`, …);
* `cq.Sketch`, with its placement applied;
* `cq.Assembly`, with its hierarchy, locations and colors;
* a raw OCP `TopoDS_Shape`;
* a list of any of the above, fused into one object.

Non-shape stack entries (vectors, locations) are skipped. If an object
cannot be displayed, the reason goes to the Log panel and the other objects
are still shown.

### Display options

| Option | Value | Effect |
| --- | --- | --- |
| `color` | a color (see below) | Face color. A color with an alpha below 1 also sets the opacity |
| `alpha` | float, 0–1 | Opacity: `1` is opaque, `0` invisible. Values are clamped to 0–1 |
| `rgba` | `(r, g, b, a)` | Color and opacity in one tuple |
| `edgecolor` | a color | Color of the edges |

Accepted color forms:

```python
"red", "steelblue", "lightgray"     # names known to OpenCascade
"#4682b4", "#48b", "#4682b480"      # hex: #rgb, #rgba, #rrggbb, #rrggbbaa
(70, 130, 180)                      # 0–255 integers
(0.27, 0.51, 0.71, 0.8)             # 0–1 floats, optional alpha
cq.Color("steelblue")               # CadQuery colors
```

An invalid option is ignored with a warning in the Log panel. It never stops
the script. Objects without a color use *Preferences > 3D View > Default
object color* (gold by default).

### Scripts without `show_object`

If a script never calls `show_object`, CQ-editor shows **every CadQuery
object bound to a public variable** (a name not starting with `_`). This
keeps quick experiments short:

```python
import cadquery as cq
plate = cq.Workplane().box(30, 20, 4)
boss = plate.faces(">Z").workplane().circle(5).extrude(6)
# plate and boss are both displayed
```

## Assemblies

```python
assy = (
    cq.Assembly()
    .add(plate, name="plate", color=cq.Color("lightgray"))
    .add(bolt, name="bolt", color=cq.Color("steelblue"),
         loc=cq.Location(cq.Vector(0, 0, 14)))
)
show_object(assy)
```

* The Objects panel mirrors the assembly tree. Each part can be hidden,
  recolored or exported on its own.
* Parts inherit the color of their parent when they have none.
* Repeated parts share one mesh, so large assemblies stay light.
* *Preferences > Objects > Merge assemblies* shows the whole assembly as one
  object instead.
* Exporting a single assembly to STEP keeps its names, structure and colors.

Example *08 Assembly* (**File > Examples**) is a complete bolted joint.

## `debug`

`debug(obj)` displays `obj` in translucent red (`color="red"`, `alpha=0.2`)
on top of the model:

```python
edges = part.edges("|Z")
debug(edges)            # are these the edges I meant?
part = edges.fillet(1)
```

## Output: `print` and `log`

* `print(...)` output from a script goes to the **Log** panel (source
  `print`). Text written to `stderr` appears there as a warning.
* `log(value)` writes an *INFO* entry (source `script`).
* Python `logging` records at level INFO and above are forwarded too.
* Messages printed by native libraries (OpenCascade warnings) are captured
  and shown as well.

In the **Console**, output appears in the console itself instead.

## The script environment

Each run starts from a fresh module called `__cq_main__`, so no variable
leaks from one run into the next. Some behavior is configurable in
*Preferences > Execution*:

| Behavior | Default | Notes |
| --- | --- | --- |
| The script's folder is added to `sys.path` | on | `import my_helpers` works for a file next to the script |
| The working directory is the script's folder | on | Relative paths such as `cq.importers.importStep("part.step")` resolve next to the script |
| Local modules are re-imported on every run | on | Edits to helper modules are picked up. Installed packages (standard library, site-packages) are never reloaded |
| CadQuery itself is re-imported before each run | off | Only useful when developing CadQuery |

More details:

* `__file__` is set to the script's path once the script has been saved.
* `rand_color()` is seeded the same way before every run, so "random"
  colors are stable from run to run.
* `input()` raises `EOFError`: scripts cannot read from the terminal.
* `sys.exit()` ends the script early without an error.
* Folder and environment settings apply only while the script runs.

## Errors

When a script raises an exception:

* the line is highlighted in the editor, and the status bar shows the
  exception and the line number;
* the **Traceback** panel lists the frames from your script onwards (frames
  inside CQ-editor are hidden). Click a row in your script to jump to it;
* the full traceback is also written to the Log panel;
* the model of the previous successful run stays on screen.

Syntax errors are reported the same way, with the line of the error.

## Patterns that work well

**Keep parameters at the top.** Dimensions as variables make the model easy
to tweak, and they show up in the Variables panel:

```python
length, width, thickness = 80, 60, 10
hole_d = 8.5
```

**Split big models into modules.** Put shared parts in `parts.py` next to the
script and `import parts`. With *Reload imported modules* on, edits to
`parts.py` apply on the next run. With *Autoreload: watch imported modules*,
saving `parts.py` even triggers the run.

**Show intermediate steps while you work** and remove them later:

```python
show_object(sketch_profile, name="profile", alpha=0.3)
show_object(result)
```

**Use the console to experiment** on the last run's objects without editing
the script: `result.faces(">Z").val().Area()`.
