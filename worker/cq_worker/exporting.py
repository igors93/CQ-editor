"""Write displayed objects to CAD and mesh files."""

from __future__ import annotations

import cadquery as cq

from .scene import Registry, to_compound

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
    anything else is fused into one compound.
    """

    file_format = file_format.lower()
    if file_format not in FORMATS:
        raise ValueError(f"Unsupported export format {file_format!r}")
    if not keys:
        raise ValueError("Nothing to export")

    if file_format == "step" and len(keys) == 1:
        obj = registry.get(keys[0]).obj
        if isinstance(obj, cq.Assembly):
            obj.export(path, "STEP")
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
