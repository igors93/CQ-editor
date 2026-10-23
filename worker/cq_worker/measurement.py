"""Exact measurements on the live OCCT subshapes kept by the scene registry.

The mesh is used only to decide what the user clicked.  Every value returned
from this module is calculated from the corresponding OCCT shape, in world
coordinates, so tessellation tolerance and the viewer's ``f32`` coordinates
do not affect the result.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any, Mapping

import cadquery as cq
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.BRepExtrema import BRepExtrema_DistShapeShape
from OCP.GeomAbs import GeomAbs_Cylinder, GeomAbs_Sphere

from .scene import Registry

#: Entities the GUI can select at once (``MAX_TOPOLOGY_SELECTIONS``).
MAX_SELECTIONS = 64


class MeasurementError(ValueError):
    """Invalid or stale topology selection supplied by the GUI."""


@dataclass(frozen=True)
class TopologyReference:
    key: str
    part: int
    kind: str
    identifier: int

    @classmethod
    def from_message(cls, value: Any) -> TopologyReference:
        if not isinstance(value, Mapping):
            raise MeasurementError("Each measurement selection must be an object")
        key = value.get("key")
        kind = value.get("kind")
        part = value.get("part")
        identifier = value.get("identifier")
        if not isinstance(key, str) or not key:
            raise MeasurementError("A measurement selection needs an object key")
        if kind not in {"face", "edge", "vertex"}:
            raise MeasurementError(f"Unknown topology kind {kind!r}")
        if isinstance(part, bool) or not isinstance(part, int) or part < 0:
            raise MeasurementError("Topology part must be a non-negative integer")
        if (
            isinstance(identifier, bool)
            or not isinstance(identifier, int)
            or identifier < 1
        ):
            raise MeasurementError("Topology identifier must be a positive integer")
        return cls(key, part, kind, identifier)


def measure(registry: Registry, raw_selections: Any) -> dict[str, Any]:
    """Describe each selected entity; for exactly two, also their relationship."""

    if (
        not isinstance(raw_selections, list)
        or not 1 <= len(raw_selections) <= MAX_SELECTIONS
    ):
        raise MeasurementError(
            f"Select between one and {MAX_SELECTIONS} topological entities to measure"
        )

    references = [TopologyReference.from_message(value) for value in raw_selections]
    shapes = []
    for reference in references:
        try:
            shape = registry.get(reference.key).subshape(
                reference.part, reference.kind, reference.identifier
            )
        except (IndexError, KeyError, ValueError) as error:
            raise MeasurementError(str(error)) from None
        shapes.append(shape)

    result: dict[str, Any] = {
        # OCCT's indexed subshape order is deterministic only for this live
        # registry entry.  It is deliberately not advertised as a persistent
        # topological name after the model is rebuilt.
        "identity": {"scope": "scene_entry", "persistent": False},
        "entities": [
            _describe(reference, shape)
            for reference, shape in zip(references, shapes, strict=True)
        ],
        "pair": None,
    }
    if len(shapes) == 2:
        result["pair"] = _describe_pair(shapes[0], shapes[1])
    return result


def _describe(reference: TopologyReference, shape: cq.Shape) -> dict[str, Any]:
    geometry_type = shape.geomType().lower()
    center = _point(shape.Center())
    result: dict[str, Any] = {
        "key": reference.key,
        "part": reference.part,
        "kind": reference.kind,
        "identifier": reference.identifier,
        "geometry_type": geometry_type,
        "center": center,
        "position": center if reference.kind == "vertex" else None,
        "length": shape.Length() if reference.kind == "edge" else None,
        "area": shape.Area() if reference.kind == "face" else None,
        "radius": None,
        "diameter": None,
    }
    radius = _radius(shape, reference.kind, geometry_type)
    if radius is not None:
        result["radius"] = radius
        result["diameter"] = radius * 2.0
    return result


def _radius(shape: cq.Shape, kind: str, geometry_type: str) -> float | None:
    if kind == "edge" and geometry_type == "circle":
        return float(shape.radius())
    if kind != "face":
        return None

    surface = BRepAdaptor_Surface(shape.wrapped)
    surface_type = surface.GetType()
    if surface_type == GeomAbs_Cylinder:
        return float(surface.Cylinder().Radius())
    if surface_type == GeomAbs_Sphere:
        return float(surface.Sphere().Radius())

    # A circular planar face (disc) has a planar surface, so its radius lives
    # on the boundary rather than on the surface adaptor.  Return a radius
    # only when all circular boundaries agree; an annulus is ambiguous.
    radii = {
        round(float(edge.radius()), 12)
        for edge in shape.Edges()
        if edge.geomType() == "CIRCLE"
    }
    return next(iter(radii)) if len(radii) == 1 else None


def _describe_pair(first: cq.Shape, second: cq.Shape) -> dict[str, Any]:
    distance, first_point, second_point = _distance(first, second)
    angle = _angle(first, second)
    return {
        "distance": distance,
        "first_point": first_point,
        "second_point": second_point,
        "angle_degrees": angle[0] if angle else None,
        "angle_kind": angle[1] if angle else None,
    }


def _distance(
    first: cq.Shape, second: cq.Shape
) -> tuple[float, list[float], list[float]]:
    operation = BRepExtrema_DistShapeShape(first.wrapped, second.wrapped)
    if not operation.IsDone() or operation.NbSolution() < 1:
        raise MeasurementError("OCCT could not calculate the distance")
    return (
        float(operation.Value()),
        _point(operation.PointOnShape1(1)),
        _point(operation.PointOnShape2(1)),
    )


def _angle(first: cq.Shape, second: cq.Shape) -> tuple[float, str] | None:
    first_kind, second_kind = type(first), type(second)
    if first_kind is cq.Edge and second_kind is cq.Edge:
        if first.geomType() != "LINE" or second.geomType() != "LINE":
            return None
        return _vector_angle(first.tangentAt(), second.tangentAt()), "edge_edge"
    if first_kind is cq.Face and second_kind is cq.Face:
        if first.geomType() != "PLANE" or second.geomType() != "PLANE":
            return None
        return _vector_angle(first.normalAt(), second.normalAt()), "face_face"
    if first_kind is cq.Edge and second_kind is cq.Face:
        return _edge_face_angle(first, second)
    if first_kind is cq.Face and second_kind is cq.Edge:
        return _edge_face_angle(second, first)
    return None


def _edge_face_angle(edge: cq.Edge, face: cq.Face) -> tuple[float, str] | None:
    if edge.geomType() != "LINE" or face.geomType() != "PLANE":
        return None
    tangent = edge.tangentAt().normalized()
    normal = face.normalAt().normalized()
    # An edge lying in the plane is 0 degrees; one normal to it is 90.
    degrees = math.degrees(math.asin(abs(_clamped_dot(tangent, normal))))
    return degrees, "edge_face"


def _vector_angle(first: cq.Vector, second: cq.Vector) -> float:
    # Lines and planes are unoriented geometric entities, so report the
    # smaller angle regardless of edge direction or face orientation.
    dot = abs(_clamped_dot(first.normalized(), second.normalized()))
    return math.degrees(math.acos(dot))


def _clamped_dot(first: cq.Vector, second: cq.Vector) -> float:
    return max(-1.0, min(1.0, first.dot(second)))


def _point(value: Any) -> list[float]:
    if hasattr(value, "toTuple"):
        value = value.toTuple()
    else:
        value = (value.X(), value.Y(), value.Z())
    return [float(component) for component in value]
