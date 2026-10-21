# Exporting and sharing

## Export formats

| Format | Menu | Best for |
| --- | --- | --- |
| **STEP** (`.step`) | Tools > Export as STEP… | Exchanging exact CAD geometry with other CAD programs. Single assemblies keep names, structure and colors |
| **STL** (`.stl`) | Tools > Export as STL… | 3D printing, mesh tools |
| **BREP** (`.brep`) | Tools > Export as BREP… | OpenCascade's native format: lossless, for other OCCT-based tools |

The same commands are in the **Export** button of the Objects panel and in
the right-click menu of an object.

**What gets exported.** The **selected** objects (with their children), or
**all objects** when nothing is selected. Several objects are fused into one
compound. A single selected `cq.Assembly` exported to STEP is written with
CadQuery's assembly exporter instead, so other programs see the parts by
name and color.

The file dialog proposes the script's name (`bracket.step`) and adds the
extension if you leave it out. The status bar shows *Exporting …* while the
file is written.

**STL precision.** STL is a mesh, so its fineness is set by two options in
*Preferences > Objects*. They are separate from the display mesh.

| Setting | Default | Meaning |
| --- | --- | --- |
| STL precision | 0.1 (model units) | Maximum distance between the mesh and the true surface |
| STL angular precision | 0.1 rad | Maximum angle between neighboring facets |

Lower values give smoother prints and bigger files.

### Exporting from the script

For repeatable output, or formats the menu does not offer (DXF, SVG, glTF,
VRML, AMF, …), export from the script with CadQuery:

```python
cq.exporters.export(bracket, "bracket.step")
cq.exporters.export(bracket.section(), "bracket.dxf")
assy.export("assembly.step")
```

With *Change working dir to script dir* on (the default), relative paths are
written next to the script.

## Screenshots

**Tools > Screenshot…** (camera icon in the toolbar) saves the 3D view, at
its current size, as a PNG image.

## Upload Model

**Tools > Upload Model…** sends the model to a web service, for example to
order a part from a manufacturing service.

* **URL**: the endpoint that receives the file. **Presets** fills it in for
  known services. **PCBWay** is built in.
* **File type**: **STEP**, **STL** or **CadQuery script** (the editor's
  source code).
* The selected objects are uploaded, or all objects when nothing is
  selected.

The file is sent as a multipart form field named `file`. If the service
answers with a link (as the PCBWay preset expects), the dialog shows it so
you can continue on the service's website. Otherwise the raw HTTP response
is shown.

> **Privacy.** Uploading sends your model, or your script, to the URL in the
> dialog. Nothing is sent unless you press **Upload**.

## Network access

CQ-editor works offline. It only contacts the network when you ask it to:

| Action | Contacts |
| --- | --- |
| Tools > Upload Model… | The URL you enter |
| Help > Check for CadQuery Updates | `api.github.com`, to list CadQuery releases newer than the installed one |
| Help > Documentation / CadQuery Documentation | Opens your web browser |
