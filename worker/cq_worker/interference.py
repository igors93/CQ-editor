"""Exact, world-space interference checks between displayed solid instances.

The display mesh is deliberately never used as evidence of clearance.  A
compound can be topologically valid while two of its solids occupy the same
space, so this analysis compares the solids individually.
"""

from __future__ import annotations

import math
from collections.abc import Callable
from dataclasses import dataclass

import cadquery as cq
from OCP.BRepExtrema import BRepExtrema_DistShapeShape

from .measurement import exact_bounds
from .scene import Registry, RegistryEntry


@dataclass(frozen=True)
class SolidInstance:
    key: str
    name: str
    part: int
    solid: int
    shape: cq.Solid
    bounds: tuple[float, float, float, float, float, float]
    source: int
    placement: tuple[float, ...]
    root: str

    def reference(self) -> dict:
        return {
            "key": self.key,
            "name": self.name,
            "part": self.part,
            "solid": self.solid,
        }


def _merged_names(assembly: cq.Assembly) -> list[str]:
    names: list[str] = []

    def visit(node: cq.Assembly, parents: tuple[str, ...]) -> None:
        path = (*parents, node.name or "part")
        if node.obj is not None:
            names.append("/".join(path))
        for child in node.children:
            visit(child, path)

    visit(assembly, ())
    return names


def collect_solids(
    registry: Registry, keys: list[str], flat_parts: bool = False
) -> list[SolidInstance]:
    """Flatten actual solids, retaining each assembly occurrence and placement.

    The same source displayed independently and inside an assembly at the same
    placement is a display alias, not an extra manufactured part.  Two children
    of one assembly remain distinct even if they use one source and location.
    """

    result: list[SolidInstance] = []
    visited: set[str] = set()
    aliases: dict[tuple[int, tuple[float, ...]], str] = {}

    def visit(key: str, root: str) -> None:
        if key in visited:
            return
        visited.add(key)
        entry: RegistryEntry = registry.get(key)
        shapes = entry.part_shapes or ([entry.shape] if entry.shape is not None else [])
        names = (
            _merged_names(entry.obj)
            if entry.part_shapes and isinstance(entry.obj, cq.Assembly)
            else [entry.name or key] * len(shapes)
        )
        for part, shape in enumerate(shapes):
            if shape is None:
                continue
            source = entry.sources[part] if part < len(entry.sources) else None
            identity = id(source.obj if source is not None else entry.obj)
            location = source.location if source is not None else cq.Location()
            placement = tuple(round(v, 12) for v in location_matrix(location))
            alias = (identity, placement)
            if alias in aliases and aliases[alias] != root:
                continue
            aliases.setdefault(alias, root)
            for solid_index, solid in enumerate(shape.Solids()):
                bounds = exact_bounds(solid)
                if not all(math.isfinite(value) for value in bounds):
                    raise ValueError(f"Non-finite bounds for {entry.name or key}")
                result.append(
                    SolidInstance(
                        key,
                        names[part] if part < len(names) else entry.name or key,
                        part,
                        solid_index,
                        solid,
                        bounds,
                        identity,
                        placement,
                        root,
                    )
                )
        if not flat_parts:
            for child in entry.children:
                visit(child, root)

    for key in dict.fromkeys(keys):
        visit(key, key)
    return result


def location_matrix(location: cq.Location) -> list[float]:
    """Rigid placement identity; the same convention as scene messages."""

    transform = location.wrapped.Transformation()
    return [
        transform.Value(row, column) for column in range(1, 5) for row in range(1, 4)
    ]


def _candidate(first: SolidInstance, second: SolidInstance, margin: float) -> bool:
    a, b = first.bounds, second.bounds
    return all(
        a[axis] <= b[axis + 3] + margin and b[axis] <= a[axis + 3] + margin
        for axis in range(3)
    )


def _distance(first: cq.Shape, second: cq.Shape) -> float:
    measure = BRepExtrema_DistShapeShape(first.wrapped, second.wrapped)
    if not measure.IsDone() or measure.NbSolution() == 0:
        raise ValueError("Open Cascade could not measure the pair")
    return float(measure.Value())


def inspect_pair(first: SolidInstance, second: SolidInstance, tolerance: float) -> dict:
    """Classify a candidate using exact B-rep intersection and distance."""

    report = {
        "first": first.reference(),
        "second": second.reference(),
        "kind": "inconclusive",
        "overlap_volume": None,
        "distance": None,
        "point": None,
        "reason": None,
    }
    try:
        if not first.shape.isValid() or not second.shape.isValid():
            raise ValueError("A solid has invalid topology")
        common = first.shape.intersect(second.shape)
        if not common.isValid():
            raise ValueError("Open Cascade returned an invalid intersection")
        volume = float(common.Volume())
        if not math.isfinite(volume) or volume < 0:
            raise ValueError("Open Cascade returned an invalid overlap volume")
        report["overlap_volume"] = volume
        threshold = max(
            tolerance**3, min(first.shape.Volume(), second.shape.Volume()) * 1e-12
        )
        if volume > threshold:
            report["kind"] = "interference"
            report["distance"] = 0.0
            center = common.Center()
            point = [center.x, center.y, center.z]
            if not all(math.isfinite(value) for value in point):
                raise ValueError("Open Cascade returned a non-finite overlap center")
            report["point"] = point
        elif volume > 0:
            report["reason"] = "Overlap is below the numerical tolerance"
        else:
            distance = _distance(first.shape, second.shape)
            if not math.isfinite(distance):
                raise ValueError("Open Cascade returned a non-finite distance")
            report["distance"] = distance
            report["kind"] = "contact" if distance <= tolerance else "clearance"
    except (RuntimeError, ValueError, OverflowError) as error:
        report["reason"] = str(error)
    return report


def analyze_interferences(
    registry: Registry,
    keys: list[str],
    *,
    flat_parts: bool = False,
    tolerance: float = 1e-6,
    clearance: float = 0.0,
    cancelled: Callable[[], bool] | None = None,
) -> list[dict]:
    """Report nearby pairs; never claim a failed exact calculation is safe."""

    if not math.isfinite(tolerance) or tolerance <= 0:
        raise ValueError("Interference tolerance must be finite and positive")
    if not math.isfinite(clearance) or clearance < 0:
        raise ValueError("Clearance limit must be finite and non-negative")
    parts = sorted(
        collect_solids(registry, keys, flat_parts), key=lambda part: part.bounds[0]
    )
    active: list[SolidInstance] = []
    reports: list[dict] = []
    margin = max(tolerance, clearance)
    for part in parts:
        if cancelled is not None and cancelled():
            raise InterruptedError("Interference check cancelled")
        active = [
            other for other in active if other.bounds[3] + margin >= part.bounds[0]
        ]
        for other in active:
            if not _candidate(other, part, margin):
                continue
            report = inspect_pair(other, part, tolerance)
            if report["kind"] != "clearance" or report["distance"] <= clearance:
                reports.append(report)
        active.append(part)
    return reports


def summarize(reports: list[dict]) -> dict:
    """A tri-state verdict: numerical uncertainty must never count as safe."""

    counts = {
        kind: 0 for kind in ("interference", "inconclusive", "contact", "clearance")
    }
    for report in reports:
        counts[report["kind"]] += 1
    status = (
        "interference"
        if counts["interference"]
        else "inconclusive" if counts["inconclusive"] else "clear"
    )
    return {"status": status, "counts": counts}
