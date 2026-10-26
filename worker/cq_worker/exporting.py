"""Write displayed objects to CAD and mesh files."""

from __future__ import annotations

import cadquery as cq

from .scene import Registry, RegistryEntry, to_compound

FORMATS = ("step", "stl", "brep")


def export(
    registry: Registry,
    keys: list[str],
    file_format: str,
    path: str,
    tolerance: float = 0.1,
    angular_tolerance: float = 0.1,
) -> None:
    """Export the objects behind ``keys`` (with their descendants).

    A single assembly exported to STEP keeps its structure, names and colors;
    anything else is fused into one compound. Either way the shapes are where
    the view shows them.
    """

    file_format = file_format.lower()
    if file_format not in FORMATS:
        raise ValueError(f"Unsupported export format {file_format!r}")
    if not keys:
        raise ValueError("Nothing to export")

    if file_format == "step" and len(keys) == 1:
        entry = registry.get(keys[0])
        if isinstance(entry.obj, cq.Assembly):
            placed_assembly(entry).export(path, "STEP")
            return

    compound = to_compound(registry.subtree_shapes(keys))
    if file_format == "stl":
        compound.exportStl(
            path, tolerance=tolerance, angularTolerance=angular_tolerance
        )
    elif file_format == "step":
        compound.exportStep(path)
    else:
        compound.exportBrep(path)


def placed_assembly(entry: RegistryEntry) -> cq.Assembly:
    """A copy of the assembly of ``entry`` as the view shows it.

    A node below the top of an assembly holds its own ``loc``, relative to
    its parent: exported alone, it is placed with its parents' location.
    The view also draws a node without a color in the color it inherits,
    while CadQuery's STEP writer only colors nodes that have one, so the
    copy gives every node its inherited color. The script's own assembly
    is left as it is.
    """

    assembly = entry.obj
    holder = cq.Assembly()
    holder.add(
        assembly,
        loc=(
            None
            if entry.parent_location is None
            else entry.parent_location * assembly.loc
        ),
    )
    placed = holder.children[0]
    _push_colors(placed, entry.inherited_color)
    return placed


def _push_colors(node: cq.Assembly, inherited: cq.Color | None) -> None:
    color = node.color or inherited
    node.color = color
    for child in node.children:
        _push_colors(child, color)
