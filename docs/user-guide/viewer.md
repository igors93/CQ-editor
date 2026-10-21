# 3D view

The 3D view draws every visible object with GPU shading, 4× anti-aliasing and
constant-width edges. The camera is controlled with the mouse, the
navigation cube and the View menu.

## Mouse

| Action | Control |
| --- | --- |
| Orbit | Left drag |
| Pan | Middle drag, or Ctrl + left drag |
| Zoom | Mouse wheel or touchpad pinch (zooms toward the pointer), right drag, or Shift + left drag |
| Select | Left click on a face, edge or vertex |
| Add to / remove from the selection | Ctrl + click |
| Clear the selection | Click on empty space |
| Zoom to an object | Double-click it |

While the view is being orbited or panned, the pointer becomes a closed hand
to make it clear that the camera is being held and moved. A click without a
drag still selects the object under the pointer.

The floating zoom controls in the top-left corner offer precise adjustments
without a mouse wheel. Use the magnifying-glass buttons for small steps or drag
the logarithmic slider to a specific percentage. **100%** is the framing set by
the most recent **Fit** or **Zoom to Selection** command.

**Orbit method** (*Preferences > 3D View*):

* **Turntable** (default) rotates around the world Z axis and the screen's
  horizontal axis, so Z always stays up. This is the most comfortable mode
  for mechanical parts.
* **Trackball** rotates freely, like rolling a ball under the pointer.

## Navigation cube

![Navigation cube](../images/navigation-cube.png)

The cube in the top-right corner always shows the current orientation.

* Click one of its **6 faces** to look straight at that side (TOP, FRONT,
  RIGHT, …).
* Click one of its **12 edges** or **8 corners** to look from that diagonal.
* Click the **two curved arrows** above it to roll the view by 45°.
* Hovering highlights the part that will be clicked.

The axes indicator in the bottom-right corner shows the X (red), Y (green)
and Z (blue) directions. Both can be hidden in *Preferences > 3D View*.

## Standard views and framing

| Command | Shortcut | Toolbar |
| --- | --- | --- |
| Fit (frame all visible objects) | Shift+F1 | ⛶ |
| Iso · Top · Bottom · Front · Back · Left · Right | Shift+F2 … Shift+F8 | cube icons |
| Zoom to Selection | View menu, or double-click | |

View changes are animated. Turn this off with *Animate view changes*. With
*Fit automatically* (on by default), the view frames the model after every
run. The first model shown is always framed.

## Display modes and projection

| Command | Shortcut |
| --- | --- |
| Wireframe (edges only) | Shift+F9 |
| Shaded | Shift+F10 |
| Perspective Projection (toggle) | View menu |

The default projection is **orthographic**, the usual choice for CAD because
parallel lines stay parallel. The menu toggle and *Preferences > 3D View >
Projection* change the same setting, which is remembered.

## Selection

Selection works at two levels.

**Objects.** Clicking an object selects it in the view and in the Objects
panel. The selected object is tinted and the property sheet shows its
details. Export, upload, *Zoom to Selection* and *Remove selected* act on the
selected objects.

**Topological entities.** The click also identifies the exact **face, edge or
vertex** under the pointer:

* the entity is highlighted on its own;
* the status bar reports it, for example *Selected Face 6 of bracket*;
* the property sheet in the Objects panel shows the id and part, for example
  *Face 6 · part 1*.

![A selected face](../images/topology-selection.png)

Faces are hit-tested exactly. Edges and vertices only need the pointer within
6 pixels, and a face in front of them hides them. Ids are 1-based and follow
OpenCascade's topology order for the shape, so they are stable while the
geometry does not change. Use them to check that your CadQuery selectors pick
what you expect.

Hovering an object highlights it before you click.

## Appearance

These options live in *Preferences > 3D View*:

| Setting | Default |
| --- | --- |
| Background color | dark gray (95, 95, 95) |
| Use gradient, Background color (aux) | off; (30, 30, 30) at the bottom |
| Default object color | gold (255, 215, 0) |
| Show edges, Edge color, Edge width | on; black; 1.5 px |

Individual objects can be recolored from the script (`show_object` options)
or in the Objects panel.

## Mesh quality

The view displays a triangle mesh of the exact CAD geometry. Two settings
control how fine it is (*Preferences > 3D View*). They apply from the next
run.

| Setting | Default | Meaning |
| --- | --- | --- |
| Deviation | 0.001 | Maximum distance between mesh and surface, relative to the object size |
| Angular deviation | 0.2 rad | Maximum angle between neighboring mesh facets |

Smaller values give smoother curves but take more time and memory. Meshes
are cached by content: re-running a script whose parts did not change reuses
the previous meshes, and identical parts in an assembly are meshed only once.

Export precision is set separately (see [Exporting](exporting.md)).

## Screenshots

**Tools > Screenshot…** (toolbar camera icon) saves the 3D view as a PNG
image.
